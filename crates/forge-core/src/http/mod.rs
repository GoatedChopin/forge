//! HTTP client with circuit breaker pattern.
//!
//! Wraps `reqwest::Client` with automatic failure tracking per host.
//! After repeated failures, requests fail fast to prevent cascade failures.

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};

use reqwest::{IntoUrl, Method, Request, RequestBuilder, Response};

/// Circuit breaker state for a single host.
#[derive(Debug, Clone)]
pub struct CircuitState {
    /// Current state of the circuit.
    pub state: CircuitStatus,
    /// Number of consecutive failures.
    pub failure_count: u32,
    /// Number of consecutive successes (used in half-open state).
    pub success_count: u32,
    /// When the circuit was opened (for timeout calculation).
    pub opened_at: Option<Instant>,
    /// Current backoff duration.
    pub current_backoff: Duration,
}

/// Circuit breaker status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitStatus {
    /// Normal operation, requests pass through.
    Closed,
    /// Circuit tripped, requests fail fast.
    Open,
    /// Testing if service recovered, limited requests allowed.
    HalfOpen,
}

impl Default for CircuitState {
    fn default() -> Self {
        Self {
            state: CircuitStatus::Closed,
            failure_count: 0,
            success_count: 0,
            opened_at: None,
            current_backoff: Duration::from_secs(30),
        }
    }
}

/// Configuration for the circuit breaker.
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of failures before opening the circuit.
    pub failure_threshold: u32,
    /// Number of successes in half-open state before closing.
    pub success_threshold: u32,
    /// Initial timeout before trying half-open.
    pub base_timeout: Duration,
    /// Maximum backoff duration.
    pub max_backoff: Duration,
    /// Backoff multiplier for exponential backoff.
    pub backoff_multiplier: f64,
    /// Whether the circuit breaker is enabled.
    pub enabled: bool,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 2,
            base_timeout: Duration::from_secs(30),
            max_backoff: Duration::from_secs(600), // 10 minutes
            backoff_multiplier: 1.5,
            enabled: true,
        }
    }
}

/// Error returned when circuit breaker is open.
#[derive(Debug, Clone)]
pub struct CircuitBreakerOpen {
    /// The host that is being blocked.
    pub host: String,
    /// Time until the circuit may try again.
    pub retry_after: Duration,
}

impl std::fmt::Display for CircuitBreakerOpen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Circuit breaker open for {}: retry after {:?}",
            self.host, self.retry_after
        )
    }
}

impl std::error::Error for CircuitBreakerOpen {}

/// HTTP client with circuit breaker pattern.
///
/// Tracks failure rates per host and fails fast when a host is unhealthy.
#[derive(Clone)]
pub struct CircuitBreakerClient {
    inner: reqwest::Client,
    states: std::sync::Arc<RwLock<HashMap<String, CircuitState>>>,
    config: CircuitBreakerConfig,
}

impl CircuitBreakerClient {
    /// Create a new circuit breaker client wrapping the given reqwest client.
    pub fn new(client: reqwest::Client, config: CircuitBreakerConfig) -> Self {
        Self {
            inner: client,
            states: std::sync::Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    /// Create with default configuration.
    pub fn with_defaults(client: reqwest::Client) -> Self {
        Self::new(client, CircuitBreakerConfig::default())
    }

    /// Get the underlying reqwest client for building requests.
    pub fn inner(&self) -> &reqwest::Client {
        &self.inner
    }

    /// Create a request client view with an optional default request timeout.
    pub fn with_timeout(&self, timeout: Option<Duration>) -> HttpClient {
        HttpClient::new(self.clone(), timeout)
    }

    /// Extract host from URL for tracking.
    fn extract_host(url: &reqwest::Url) -> String {
        format!(
            "{}://{}{}",
            url.scheme(),
            url.host_str().unwrap_or("unknown"),
            url.port().map(|p| format!(":{}", p)).unwrap_or_default()
        )
    }

    /// Check if a request to the given host should be allowed.
    pub fn should_allow(&self, host: &str) -> Result<(), CircuitBreakerOpen> {
        if !self.config.enabled {
            return Ok(());
        }

        let states = self.states.read().unwrap_or_else(|e| {
            tracing::error!("Circuit breaker lock was poisoned, recovering");
            e.into_inner()
        });
        let state = match states.get(host) {
            Some(s) => s,
            None => return Ok(()), // No state = first request, allow
        };

        match state.state {
            CircuitStatus::Closed => Ok(()),
            CircuitStatus::HalfOpen => Ok(()), // Allow test requests
            CircuitStatus::Open => {
                let opened_at = state.opened_at.unwrap_or_else(Instant::now);
                let elapsed = opened_at.elapsed();

                if elapsed >= state.current_backoff {
                    // Timeout expired, will transition to half-open
                    Ok(())
                } else {
                    Err(CircuitBreakerOpen {
                        host: host.to_string(),
                        retry_after: state.current_backoff - elapsed,
                    })
                }
            }
        }
    }

    /// Record a successful request.
    pub fn record_success(&self, host: &str) {
        if !self.config.enabled {
            return;
        }

        let mut states = self.states.write().unwrap_or_else(|e| {
            tracing::error!("Circuit breaker lock was poisoned, recovering");
            e.into_inner()
        });
        let state = states.entry(host.to_string()).or_default();

        match state.state {
            CircuitStatus::Closed => {
                // Reset failure count on success
                state.failure_count = 0;
            }
            CircuitStatus::HalfOpen => {
                state.success_count += 1;
                if state.success_count >= self.config.success_threshold {
                    // Service recovered, close the circuit
                    tracing::info!(host = %host, "Circuit breaker closed, service recovered");
                    state.state = CircuitStatus::Closed;
                    state.failure_count = 0;
                    state.success_count = 0;
                    state.opened_at = None;
                    state.current_backoff = self.config.base_timeout;
                }
            }
            CircuitStatus::Open => {
                // Transition to half-open on first success after timeout
                tracing::info!(host = %host, "Circuit breaker half-open, testing service");
                state.state = CircuitStatus::HalfOpen;
                state.success_count = 1;
            }
        }
    }

    /// Record a failed request.
    pub fn record_failure(&self, host: &str) {
        if !self.config.enabled {
            return;
        }

        let mut states = self.states.write().unwrap_or_else(|e| {
            tracing::error!("Circuit breaker lock was poisoned, recovering");
            e.into_inner()
        });
        let state = states.entry(host.to_string()).or_default();

        match state.state {
            CircuitStatus::Closed => {
                state.failure_count += 1;
                if state.failure_count >= self.config.failure_threshold {
                    // Trip the circuit
                    tracing::warn!(
                        host = %host,
                        failures = state.failure_count,
                        "Circuit breaker opened, service unhealthy"
                    );
                    state.state = CircuitStatus::Open;
                    state.opened_at = Some(Instant::now());
                }
            }
            CircuitStatus::HalfOpen => {
                // Failed during test, reopen with increased backoff
                let new_backoff = Duration::from_secs_f64(
                    (state.current_backoff.as_secs_f64() * self.config.backoff_multiplier)
                        .min(self.config.max_backoff.as_secs_f64()),
                );
                tracing::warn!(
                    host = %host,
                    backoff_secs = new_backoff.as_secs(),
                    "Circuit breaker reopened, service still unhealthy"
                );
                state.state = CircuitStatus::Open;
                state.opened_at = Some(Instant::now());
                state.current_backoff = new_backoff;
                state.success_count = 0;
            }
            CircuitStatus::Open => {
                // Already open, just update timestamp
                state.opened_at = Some(Instant::now());
            }
        }
    }

    /// Execute a request with circuit breaker protection.
    pub async fn execute(&self, request: Request) -> Result<Response, CircuitBreakerError> {
        let host = Self::extract_host(request.url());

        // Check circuit state
        self.should_allow(&host)
            .map_err(CircuitBreakerError::CircuitOpen)?;

        // If circuit is open but timeout expired, transition to half-open
        {
            let mut states = self.states.write().unwrap_or_else(|e| {
                tracing::error!("Circuit breaker lock was poisoned, recovering");
                e.into_inner()
            });
            if let Some(state) = states.get_mut(&host)
                && state.state == CircuitStatus::Open
                && let Some(opened_at) = state.opened_at
                && opened_at.elapsed() >= state.current_backoff
            {
                tracing::info!(host = %host, "Circuit breaker half-open, testing service");
                state.state = CircuitStatus::HalfOpen;
                state.success_count = 0;
            }
        }

        // Execute the request
        match self.inner.execute(request).await {
            Ok(response) => {
                // Check if response indicates server error
                if response.status().is_server_error() {
                    self.record_failure(&host);
                } else {
                    self.record_success(&host);
                }
                Ok(response)
            }
            Err(e) => {
                self.record_failure(&host);
                Err(CircuitBreakerError::Request(e))
            }
        }
    }

    /// Get the current state for a host.
    pub fn get_state(&self, host: &str) -> Option<CircuitState> {
        self.states
            .read()
            .unwrap_or_else(|e| {
                tracing::error!("Circuit breaker lock was poisoned, recovering");
                e.into_inner()
            })
            .get(host)
            .cloned()
    }

    /// Reset the circuit breaker state for a host.
    pub fn reset(&self, host: &str) {
        self.states
            .write()
            .unwrap_or_else(|e| {
                tracing::error!("Circuit breaker lock was poisoned, recovering");
                e.into_inner()
            })
            .remove(host);
    }

    /// Reset all circuit breaker states.
    pub fn reset_all(&self) {
        self.states
            .write()
            .unwrap_or_else(|e| {
                tracing::error!("Circuit breaker lock was poisoned, recovering");
                e.into_inner()
            })
            .clear();
    }
}

/// Error type for circuit breaker operations.
#[derive(Debug)]
pub enum CircuitBreakerError {
    /// The circuit is open, request was not attempted.
    CircuitOpen(CircuitBreakerOpen),
    /// The request failed.
    Request(reqwest::Error),
}

impl std::fmt::Display for CircuitBreakerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CircuitBreakerError::CircuitOpen(e) => write!(f, "{}", e),
            CircuitBreakerError::Request(e) => write!(f, "HTTP request failed: {}", e),
        }
    }
}

impl std::error::Error for CircuitBreakerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CircuitBreakerError::CircuitOpen(e) => Some(e),
            CircuitBreakerError::Request(e) => Some(e),
        }
    }
}

impl From<reqwest::Error> for CircuitBreakerError {
    fn from(e: reqwest::Error) -> Self {
        CircuitBreakerError::Request(e)
    }
}

/// HTTP client facade that routes requests through a circuit breaker and can
/// apply a default timeout to requests that do not set one explicitly.
#[derive(Clone)]
pub struct HttpClient {
    circuit_breaker: CircuitBreakerClient,
    default_timeout: Option<Duration>,
}

impl HttpClient {
    /// Create a new HTTP client facade.
    pub fn new(circuit_breaker: CircuitBreakerClient, default_timeout: Option<Duration>) -> Self {
        Self {
            circuit_breaker,
            default_timeout,
        }
    }

    /// Get the underlying reqwest client.
    pub fn inner(&self) -> &reqwest::Client {
        self.circuit_breaker.inner()
    }

    /// Get the underlying circuit breaker client.
    pub fn circuit_breaker(&self) -> &CircuitBreakerClient {
        &self.circuit_breaker
    }

    /// Get the default timeout applied to requests that do not override it.
    pub fn default_timeout(&self) -> Option<Duration> {
        self.default_timeout
    }

    /// Create a request builder.
    pub fn request<U: IntoUrl>(&self, method: Method, url: U) -> HttpRequestBuilder {
        HttpRequestBuilder::new(self.clone(), self.inner().request(method, url))
    }

    pub fn get<U: IntoUrl>(&self, url: U) -> HttpRequestBuilder {
        self.request(Method::GET, url)
    }

    pub fn post<U: IntoUrl>(&self, url: U) -> HttpRequestBuilder {
        self.request(Method::POST, url)
    }

    pub fn put<U: IntoUrl>(&self, url: U) -> HttpRequestBuilder {
        self.request(Method::PUT, url)
    }

    pub fn patch<U: IntoUrl>(&self, url: U) -> HttpRequestBuilder {
        self.request(Method::PATCH, url)
    }

    pub fn delete<U: IntoUrl>(&self, url: U) -> HttpRequestBuilder {
        self.request(Method::DELETE, url)
    }

    pub fn head<U: IntoUrl>(&self, url: U) -> HttpRequestBuilder {
        self.request(Method::HEAD, url)
    }

    /// Execute a pre-built request through the circuit breaker.
    pub async fn execute(&self, mut request: Request) -> crate::Result<Response> {
        self.apply_default_timeout(&mut request);
        self.circuit_breaker
            .execute(request)
            .await
            .map_err(Into::into)
    }

    fn apply_default_timeout(&self, request: &mut Request) {
        if request.timeout().is_none()
            && let Some(timeout) = self.default_timeout
        {
            *request.timeout_mut() = Some(timeout);
        }
    }
}

/// Request builder paired with a circuit-breaker-backed HTTP client.
pub struct HttpRequestBuilder {
    client: HttpClient,
    request: RequestBuilder,
}

impl HttpRequestBuilder {
    fn new(client: HttpClient, request: RequestBuilder) -> Self {
        Self { client, request }
    }

    pub fn header(self, key: impl AsRef<str>, value: impl AsRef<str>) -> Self {
        Self {
            request: self.request.header(key.as_ref(), value.as_ref()),
            ..self
        }
    }

    pub fn headers(self, headers: reqwest::header::HeaderMap) -> Self {
        Self {
            request: self.request.headers(headers),
            ..self
        }
    }

    pub fn bearer_auth(self, token: impl std::fmt::Display) -> Self {
        Self {
            request: self.request.bearer_auth(token),
            ..self
        }
    }

    pub fn basic_auth(
        self,
        username: impl std::fmt::Display,
        password: Option<impl std::fmt::Display>,
    ) -> Self {
        Self {
            request: self.request.basic_auth(username, password),
            ..self
        }
    }

    pub fn body(self, body: impl Into<reqwest::Body>) -> Self {
        Self {
            request: self.request.body(body),
            ..self
        }
    }

    pub fn json(self, json: &impl serde::Serialize) -> Self {
        Self {
            request: self.request.json(json),
            ..self
        }
    }

    pub fn form(self, form: &impl serde::Serialize) -> Self {
        Self {
            request: self.request.form(form),
            ..self
        }
    }

    pub fn query(self, query: &impl serde::Serialize) -> Self {
        Self {
            request: self.request.query(query),
            ..self
        }
    }

    pub fn timeout(self, timeout: Duration) -> Self {
        Self {
            request: self.request.timeout(timeout),
            ..self
        }
    }

    pub fn version(self, version: reqwest::Version) -> Self {
        Self {
            request: self.request.version(version),
            ..self
        }
    }

    pub fn try_clone(&self) -> Option<Self> {
        self.request.try_clone().map(|request| Self {
            client: self.client.clone(),
            request,
        })
    }

    pub fn build(self) -> crate::Result<Request> {
        self.request
            .build()
            .map_err(|e| crate::ForgeError::Internal(e.to_string()))
    }

    pub async fn send(self) -> crate::Result<Response> {
        let client = self.client.clone();
        let request = self.build()?;
        client.execute(request).await
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn test_circuit_breaker_defaults() {
        let config = CircuitBreakerConfig::default();
        assert_eq!(config.failure_threshold, 5);
        assert_eq!(config.success_threshold, 2);
        assert!(config.enabled);
    }

    #[test]
    fn test_circuit_state_transitions() {
        let client = reqwest::Client::new();
        let breaker = CircuitBreakerClient::with_defaults(client);
        let host = "https://api.example.com";

        // Initial state should allow
        assert!(breaker.should_allow(host).is_ok());

        // Record failures to trip the circuit
        for _ in 0..5 {
            breaker.record_failure(host);
        }

        // Circuit should be open
        let state = breaker.get_state(host).unwrap();
        assert_eq!(state.state, CircuitStatus::Open);

        // Should be blocked
        assert!(breaker.should_allow(host).is_err());

        // Reset and verify
        breaker.reset(host);
        assert!(breaker.should_allow(host).is_ok());
    }

    #[test]
    fn test_extract_host() {
        let url = reqwest::Url::parse("https://api.example.com:8080/path").unwrap();
        assert_eq!(
            CircuitBreakerClient::extract_host(&url),
            "https://api.example.com:8080"
        );

        let url2 = reqwest::Url::parse("http://localhost/api").unwrap();
        assert_eq!(
            CircuitBreakerClient::extract_host(&url2),
            "http://localhost"
        );
    }

    #[test]
    fn test_http_client_applies_default_timeout_when_missing() {
        let breaker = CircuitBreakerClient::with_defaults(reqwest::Client::new());
        let client = breaker.with_timeout(Some(Duration::from_secs(5)));
        let mut request = reqwest::Request::new(
            Method::GET,
            reqwest::Url::parse("https://example.com").unwrap(),
        );

        client.apply_default_timeout(&mut request);

        assert_eq!(request.timeout(), Some(&Duration::from_secs(5)));
    }

    #[test]
    fn test_http_client_preserves_explicit_timeout() {
        let breaker = CircuitBreakerClient::with_defaults(reqwest::Client::new());
        let client = breaker.with_timeout(Some(Duration::from_secs(5)));
        let mut request = reqwest::Request::new(
            Method::GET,
            reqwest::Url::parse("https://example.com").unwrap(),
        );
        *request.timeout_mut() = Some(Duration::from_secs(1));

        client.apply_default_timeout(&mut request);

        assert_eq!(request.timeout(), Some(&Duration::from_secs(1)));
    }
}
