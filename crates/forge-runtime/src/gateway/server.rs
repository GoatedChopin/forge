use std::sync::Arc;
use std::time::Duration;

use axum::{
    Extension, Json, Router,
    error_handling::HandleErrorLayer,
    extract::DefaultBodyLimit,
    http::StatusCode,
    middleware,
    response::IntoResponse,
    routing::{get, post},
};
use serde::Serialize;
use tower::BoxError;
use tower::ServiceBuilder;
use tower::limit::ConcurrencyLimitLayer;
use tower::timeout::TimeoutLayer;
use tower_http::cors::{Any, CorsLayer};

use forge_core::cluster::NodeId;
use forge_core::config::McpConfig;
use forge_core::function::{JobDispatch, WorkflowDispatch};
#[cfg(feature = "otel")]
use opentelemetry::global;
#[cfg(feature = "otel")]
use opentelemetry::propagation::Extractor;
use tracing::Instrument;
#[cfg(feature = "otel")]
use tracing_opentelemetry::OpenTelemetrySpanExt;

use super::auth::{AuthConfig, AuthMiddleware, HmacTokenIssuer, auth_middleware};
use super::mcp::{McpState, mcp_get_handler, mcp_post_handler};
use super::multipart::{MultipartConfig, rpc_multipart_handler};
use super::response::{RpcError, RpcResponse};
use super::rpc::{RpcHandler, rpc_batch_handler, rpc_function_handler, rpc_handler};
use super::sse::{
    SseState, sse_handler, sse_job_subscribe_handler, sse_subscribe_handler,
    sse_unsubscribe_handler, sse_workflow_subscribe_handler,
};
use super::tracing::{REQUEST_ID_HEADER, SPAN_ID_HEADER, TRACE_ID_HEADER, TracingState};
use crate::db::Database;
use crate::function::FunctionRegistry;
use crate::mcp::McpToolRegistry;
use crate::realtime::{Reactor, ReactorConfig};

const DEFAULT_MAX_JSON_BODY_SIZE: usize = 1024 * 1024;
const DEFAULT_MAX_MULTIPART_BODY_SIZE: usize = 20 * 1024 * 1024;
const DEFAULT_MAX_FILE_SIZE: usize = 10 * 1024 * 1024;
const MAX_MULTIPART_CONCURRENCY: usize = 32;
/// Fallback for visitor ID hashing when no JWT secret is configured (dev only).
const DEFAULT_SIGNAL_SECRET: &str = "forge-default-signal-secret";

/// Gateway server configuration.
#[derive(Debug, Clone)]
pub struct GatewayConfig {
    /// Port to listen on.
    pub port: u16,
    /// Maximum number of connections.
    pub max_connections: usize,
    /// Maximum number of active SSE sessions.
    pub sse_max_sessions: usize,
    /// Request timeout in seconds.
    pub request_timeout_secs: u64,
    /// Enable CORS.
    pub cors_enabled: bool,
    /// Allowed CORS origins.
    pub cors_origins: Vec<String>,
    /// Authentication configuration.
    pub auth: AuthConfig,
    /// MCP configuration.
    pub mcp: McpConfig,
    /// Routes excluded from request logs, metrics, and traces.
    pub quiet_routes: Vec<String>,
    /// Token TTL configuration for refresh token management.
    pub token_ttl: forge_core::AuthTokenTtl,
    /// Project name (displayed on OAuth consent page).
    pub project_name: String,
    /// Maximum body size in bytes for uploads. Defaults to 20 MB.
    pub max_body_size_bytes: usize,
    /// Default per-file cap in bytes for multipart uploads. Applies when
    /// a mutation does not declare its own `max_size`. Defaults to 10 MB.
    pub max_file_size_bytes: usize,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            port: 9081,
            max_connections: 512,
            sse_max_sessions: 10_000,
            request_timeout_secs: 30,
            cors_enabled: false,
            cors_origins: Vec::new(),
            auth: AuthConfig::default(),
            mcp: McpConfig::default(),
            quiet_routes: Vec::new(),
            token_ttl: forge_core::AuthTokenTtl::default(),
            project_name: "forge-app".to_string(),
            max_body_size_bytes: DEFAULT_MAX_MULTIPART_BODY_SIZE,
            max_file_size_bytes: DEFAULT_MAX_FILE_SIZE,
        }
    }
}

/// Health check response.
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}

/// Readiness check response.
#[derive(Debug, Serialize)]
pub struct ReadinessResponse {
    pub ready: bool,
    pub database: bool,
    pub reactor: bool,
    pub workflows: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_workflow_runs: Option<i64>,
    pub version: String,
}

/// State for readiness check.
#[derive(Clone)]
pub struct ReadinessState {
    db_pool: sqlx::PgPool,
    reactor: Arc<Reactor>,
}

/// Gateway HTTP server.
pub struct GatewayServer {
    config: GatewayConfig,
    registry: FunctionRegistry,
    db: Database,
    reactor: Arc<Reactor>,
    job_dispatcher: Option<Arc<dyn JobDispatch>>,
    workflow_dispatcher: Option<Arc<dyn WorkflowDispatch>>,
    mcp_registry: Option<McpToolRegistry>,
    token_ttl: forge_core::AuthTokenTtl,
    signals_collector: Option<crate::signals::SignalsCollector>,
    signals_anonymize_ip: bool,
    signals_geoip: Option<crate::signals::geoip::GeoIpResolver>,
    custom_routes: Option<Router>,
}

impl GatewayServer {
    /// Create a new gateway server.
    pub fn new(config: GatewayConfig, registry: FunctionRegistry, db: Database) -> Self {
        let node_id = NodeId::new();
        let reactor = Arc::new(Reactor::new(
            node_id,
            db.primary().clone(),
            registry.clone(),
            ReactorConfig::default(),
        ));

        let token_ttl = config.token_ttl.clone();
        Self {
            config,
            registry,
            db,
            reactor,
            job_dispatcher: None,
            workflow_dispatcher: None,
            mcp_registry: None,
            token_ttl,
            signals_collector: None,
            signals_anonymize_ip: false,
            signals_geoip: None,
            custom_routes: None,
        }
    }

    /// Set the job dispatcher.
    pub fn with_job_dispatcher(mut self, dispatcher: Arc<dyn JobDispatch>) -> Self {
        self.job_dispatcher = Some(dispatcher);
        self
    }

    /// Set the workflow dispatcher.
    pub fn with_workflow_dispatcher(mut self, dispatcher: Arc<dyn WorkflowDispatch>) -> Self {
        self.workflow_dispatcher = Some(dispatcher);
        self
    }

    /// Set the MCP tool registry.
    pub fn with_mcp_registry(mut self, registry: McpToolRegistry) -> Self {
        self.mcp_registry = Some(registry);
        self
    }

    /// Set the signals collector for auto-capturing RPC events and
    /// registering client signal ingestion endpoints.
    ///
    /// Also installs the collector into the process-wide emit module so
    /// background executions (jobs, crons, workflows, daemons, webhooks,
    /// auth failures) can emit signals without threading through plumbing.
    pub fn with_signals_collector(mut self, collector: crate::signals::SignalsCollector) -> Self {
        crate::signals::install_global(Some(collector.clone()));
        self.signals_collector = Some(collector);
        self
    }

    /// Enable IP anonymization for signal events.
    /// When true, raw client IPs are not stored in event records.
    pub fn with_signals_anonymize_ip(mut self, anonymize: bool) -> Self {
        self.signals_anonymize_ip = anonymize;
        self
    }

    /// Set the GeoIP resolver for country code lookups from client IPs.
    pub fn with_signals_geoip(mut self, resolver: crate::signals::geoip::GeoIpResolver) -> Self {
        self.signals_geoip = Some(resolver);
        self
    }

    /// Set additional routes that receive the full middleware stack
    /// (auth, CORS, tracing, concurrency limits, timeouts).
    pub fn with_custom_routes(mut self, router: Router) -> Self {
        self.custom_routes = Some(router);
        self
    }

    /// Get a reference to the reactor.
    pub fn reactor(&self) -> Arc<Reactor> {
        self.reactor.clone()
    }

    /// Build an OAuth router (bypasses auth middleware). Returns None if OAuth is disabled.
    pub fn oauth_router(&self) -> Option<(Router, Arc<super::oauth::OAuthState>)> {
        if !self.config.mcp.oauth {
            return None;
        }

        let token_issuer = HmacTokenIssuer::from_config(&self.config.auth)
            .map(|issuer| Arc::new(issuer) as Arc<dyn forge_core::TokenIssuer>)?;

        let auth_middleware_state = Arc::new(AuthMiddleware::new(self.config.auth.clone()));

        let jwt_secret = self.config.auth.jwt_secret.clone().unwrap_or_default();

        let oauth_state = Arc::new(super::oauth::OAuthState::new(
            self.db.primary().clone(),
            auth_middleware_state,
            token_issuer,
            self.token_ttl.access_token_secs,
            self.token_ttl.refresh_token_days,
            self.config.auth.is_hmac(),
            self.config.project_name.clone(),
            jwt_secret,
        ));

        let router = Router::new()
            .route(
                "/oauth/authorize",
                get(super::oauth::oauth_authorize_get).post(super::oauth::oauth_authorize_post),
            )
            .route("/oauth/token", post(super::oauth::oauth_token))
            .route("/oauth/register", post(super::oauth::oauth_register))
            .with_state(oauth_state.clone());

        Some((router, oauth_state))
    }

    /// Build the Axum router.
    pub fn router(&self) -> Router {
        let token_issuer = HmacTokenIssuer::from_config(&self.config.auth)
            .map(|issuer| Arc::new(issuer) as Arc<dyn forge_core::TokenIssuer>);

        let mut rpc = RpcHandler::with_dispatch_and_issuer(
            self.registry.clone(),
            self.db.clone(),
            self.job_dispatcher.clone(),
            self.workflow_dispatcher.clone(),
            token_issuer,
        );
        rpc.set_token_ttl(self.token_ttl.clone());
        if let Some(collector) = &self.signals_collector {
            let secret = self.config.auth.jwt_secret.clone().unwrap_or_else(|| {
                tracing::warn!(
                    "No jwt_secret configured; using default signal secret for visitor ID hashing. \
                         Visitor IDs will be predictable. Set [auth] jwt_secret in forge.toml."
                );
                DEFAULT_SIGNAL_SECRET.to_string()
            });
            rpc.set_signals_collector(collector.clone(), secret);
        }
        let rpc_handler_state = Arc::new(rpc);

        let auth_middleware_state = Arc::new(AuthMiddleware::new(self.config.auth.clone()));

        // Build CORS layer. When specific origins are configured, allow
        // credentials so the browser accepts cross-origin API responses
        // (the forge-svelte client sends `credentials: "include"` for
        // the SSE session cookie). Wildcard methods/headers are incompatible
        // with credentials per the CORS spec, so we enumerate them.
        let cors = if self.config.cors_enabled {
            if self.config.cors_origins.iter().any(|o| o == "*") {
                // Wildcard origin can't use credentials
                CorsLayer::new()
                    .allow_origin(Any)
                    .allow_methods(Any)
                    .allow_headers(Any)
            } else {
                use axum::http::Method;
                let origins: Vec<_> = self
                    .config
                    .cors_origins
                    .iter()
                    .filter_map(|o| o.parse().ok())
                    .collect();
                CorsLayer::new()
                    .allow_origin(origins)
                    .allow_methods([
                        Method::GET,
                        Method::POST,
                        Method::PUT,
                        Method::DELETE,
                        Method::PATCH,
                        Method::OPTIONS,
                    ])
                    .allow_headers([
                        axum::http::header::CONTENT_TYPE,
                        axum::http::header::AUTHORIZATION,
                        axum::http::header::ACCEPT,
                        axum::http::HeaderName::from_static("x-webhook-signature"),
                        axum::http::HeaderName::from_static("x-idempotency-key"),
                        axum::http::HeaderName::from_static("x-correlation-id"),
                        axum::http::HeaderName::from_static("x-session-id"),
                        axum::http::HeaderName::from_static("x-forge-platform"),
                    ])
                    .allow_credentials(true)
            }
        } else {
            CorsLayer::new()
        };

        // SSE state for Server-Sent Events
        let sse_state = Arc::new(SseState::with_config(
            self.reactor.clone(),
            auth_middleware_state.clone(),
            super::sse::SseConfig {
                max_sessions: self.config.sse_max_sessions,
                ..Default::default()
            },
        ));

        // Readiness state for DB + reactor health check
        let readiness_state = Arc::new(ReadinessState {
            db_pool: self.db.primary().clone(),
            reactor: self.reactor.clone(),
        });

        // Build the main router with middleware
        let mut main_router = Router::new()
            // Health check endpoint (liveness)
            .route("/health", get(health_handler))
            // Readiness check endpoint (checks DB)
            .route("/ready", get(readiness_handler).with_state(readiness_state))
            // RPC endpoint
            .route("/rpc", post(rpc_handler))
            // Batch RPC endpoint
            .route("/rpc/batch", post(rpc_batch_handler))
            // REST-style function endpoint (JSON)
            .route("/rpc/{function}", post(rpc_function_handler))
            // Prevent oversized JSON payloads from exhausting memory.
            .layer(DefaultBodyLimit::max(DEFAULT_MAX_JSON_BODY_SIZE))
            // Add state
            .with_state(rpc_handler_state.clone());

        // Multipart RPC router. The Axum layer limit is set to the highest
        // configured size (global or any per-mutation override) so that
        // per-mutation max_size values aren't rejected at the HTTP layer.
        // The handler still enforces per-function limits chunk-by-chunk.
        let max_per_mutation = self
            .registry
            .functions()
            .filter_map(|(_, entry)| entry.info().max_upload_size_bytes)
            .max()
            .unwrap_or(0);
        let layer_limit = self.config.max_body_size_bytes.max(max_per_mutation);
        let mp_config = MultipartConfig {
            max_body_size_bytes: self.config.max_body_size_bytes,
            max_file_size_bytes: self.config.max_file_size_bytes,
        };
        let multipart_router = Router::new()
            .route("/rpc/{function}/upload", post(rpc_multipart_handler))
            .layer(DefaultBodyLimit::max(layer_limit))
            .layer(Extension(mp_config))
            // Cap upload fan-out; each request buffers data in memory.
            .layer(ConcurrencyLimitLayer::new(MAX_MULTIPART_CONCURRENCY))
            .with_state(rpc_handler_state);

        // SSE router
        let sse_router = Router::new()
            .route("/events", get(sse_handler))
            .route("/subscribe", post(sse_subscribe_handler))
            .route("/unsubscribe", post(sse_unsubscribe_handler))
            .route("/subscribe-job", post(sse_job_subscribe_handler))
            .route("/subscribe-workflow", post(sse_workflow_subscribe_handler))
            .with_state(sse_state);

        let mut mcp_router = Router::new();
        if self.config.mcp.enabled {
            let path = self.config.mcp.path.clone();
            let mcp_state = Arc::new(McpState::new(
                self.config.mcp.clone(),
                self.mcp_registry.clone().unwrap_or_default(),
                self.db.primary().clone(),
                self.job_dispatcher.clone(),
                self.workflow_dispatcher.clone(),
            ));
            mcp_router = mcp_router.route(
                &path,
                post(mcp_post_handler)
                    .get(mcp_get_handler)
                    .with_state(mcp_state),
            );
        }

        // Signal ingestion endpoints (product analytics + diagnostics)
        let mut signals_router = Router::new();
        if let Some(collector) = &self.signals_collector {
            let signals_state = Arc::new(crate::signals::endpoints::SignalsState {
                collector: collector.clone(),
                pool: self.db.analytics_pool().clone(),
                server_secret: self
                    .config
                    .auth
                    .jwt_secret
                    .clone()
                    .unwrap_or_else(|| {
                        tracing::warn!(
                            "No jwt_secret configured; using default signal secret for visitor ID hashing. \
                             Visitor IDs will be predictable. Set [auth] jwt_secret in forge.toml."
                        );
                        DEFAULT_SIGNAL_SECRET.to_string()
                    }),
                anonymize_ip: self.signals_anonymize_ip,
                geoip: self.signals_geoip.clone(),
            });
            signals_router = Router::new()
                .route(
                    "/signal/event",
                    post(crate::signals::endpoints::event_handler),
                )
                .route(
                    "/signal/view",
                    post(crate::signals::endpoints::view_handler),
                )
                .route(
                    "/signal/user",
                    post(crate::signals::endpoints::user_handler),
                )
                .route(
                    "/signal/report",
                    post(crate::signals::endpoints::report_handler),
                )
                .route(
                    "/signal/vital",
                    post(crate::signals::endpoints::vital_handler),
                )
                .with_state(signals_state);
        }

        main_router = main_router
            .merge(multipart_router)
            .merge(sse_router)
            .merge(mcp_router)
            .merge(signals_router);

        if let Some(custom) = &self.custom_routes {
            main_router = main_router.merge(custom.clone());
        }

        // Build middleware stack
        let service_builder = ServiceBuilder::new()
            .layer(HandleErrorLayer::new(handle_middleware_error))
            .layer(ConcurrencyLimitLayer::new(self.config.max_connections))
            .layer(TimeoutLayer::new(Duration::from_secs(
                self.config.request_timeout_secs,
            )))
            .layer(cors.clone())
            .layer(middleware::from_fn_with_state(
                auth_middleware_state,
                auth_middleware,
            ))
            .layer(middleware::from_fn_with_state(
                Arc::new(self.config.quiet_routes.clone()),
                tracing_middleware,
            ));

        // Apply the remaining middleware layers
        main_router.layer(service_builder)
    }

    /// Get the socket address to bind to.
    pub fn addr(&self) -> std::net::SocketAddr {
        std::net::SocketAddr::from(([0, 0, 0, 0], self.config.port))
    }

    /// Run the server (blocking).
    pub async fn run(self) -> Result<(), std::io::Error> {
        let addr = self.addr();
        let router = self.router();

        // Start the reactor for real-time updates
        self.reactor
            .start()
            .await
            .map_err(|e| std::io::Error::other(format!("Failed to start reactor: {}", e)))?;
        tracing::info!("Reactor started for real-time updates");

        tracing::info!("Gateway server listening on {}", addr);

        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, router.into_make_service()).await
    }
}

/// Health check handler (liveness probe).
async fn health_handler() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

/// Readiness check handler (readiness probe).
async fn readiness_handler(
    axum::extract::State(state): axum::extract::State<Arc<ReadinessState>>,
) -> (axum::http::StatusCode, Json<ReadinessResponse>) {
    // Check database connectivity
    let db_ok = sqlx::query_scalar!("SELECT 1 as \"v!\"")
        .fetch_one(&state.db_pool)
        .await
        .is_ok();

    // Check reactor health (change listener must be running for real-time updates)
    let reactor_stats = state.reactor.stats().await;
    let reactor_ok = reactor_stats.listener_running;

    // Check for blocked workflow runs (strict mode: unhealthy if any runs are blocked)
    let (workflows_ok, blocked_count) = if db_ok {
        match sqlx::query_scalar!(
            r#"SELECT COUNT(*) as "count!" FROM forge_workflow_runs WHERE status LIKE 'blocked_%'"#,
        )
        .fetch_one(&state.db_pool)
        .await
        {
            Ok(count) => (count == 0, if count > 0 { Some(count) } else { None }),
            Err(_) => (true, None), // if query fails, don't block on this check
        }
    } else {
        (true, None)
    };

    let ready = db_ok && reactor_ok && workflows_ok;
    let status_code = if ready {
        axum::http::StatusCode::OK
    } else {
        axum::http::StatusCode::SERVICE_UNAVAILABLE
    };

    (
        status_code,
        Json(ReadinessResponse {
            ready,
            database: db_ok,
            reactor: reactor_ok,
            workflows: workflows_ok,
            blocked_workflow_runs: blocked_count,
            version: env!("CARGO_PKG_VERSION").to_string(),
        }),
    )
}

async fn handle_middleware_error(err: BoxError) -> axum::response::Response {
    let (status, code, message) = if err.is::<tower::timeout::error::Elapsed>() {
        (StatusCode::REQUEST_TIMEOUT, "TIMEOUT", "Request timed out")
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "SERVICE_UNAVAILABLE",
            "Server overloaded",
        )
    };
    (
        status,
        Json(RpcResponse::error(RpcError::new(code, message))),
    )
        .into_response()
}

fn set_tracing_headers(response: &mut axum::response::Response, trace_id: &str, request_id: &str) {
    if let Ok(val) = trace_id.parse() {
        response.headers_mut().insert(TRACE_ID_HEADER, val);
    }
    if let Ok(val) = request_id.parse() {
        response.headers_mut().insert(REQUEST_ID_HEADER, val);
    }
}

/// Extracts W3C traceparent context from HTTP headers.
#[cfg(feature = "otel")]
struct HeaderExtractor<'a>(&'a axum::http::HeaderMap);

#[cfg(feature = "otel")]
impl<'a> Extractor for HeaderExtractor<'a> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(|v| v.to_str().ok())
    }

    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(|k| k.as_str()).collect()
    }
}

/// Wraps each request in a span with HTTP semantics and OpenTelemetry
/// context propagation. Incoming `traceparent` headers are extracted so
/// that spans join the caller's distributed trace.
/// Quiet routes skip spans, logs, and metrics to avoid noise from
/// probes or high-frequency internal endpoints.
async fn tracing_middleware(
    axum::extract::State(quiet_routes): axum::extract::State<Arc<Vec<String>>>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let headers = req.headers();

    // Extract W3C traceparent from incoming headers for distributed tracing.
    // Without `otel`, there's no propagator and no parent context to extract.
    #[cfg(feature = "otel")]
    let parent_cx =
        global::get_text_map_propagator(|propagator| propagator.extract(&HeaderExtractor(headers)));

    let trace_id = headers
        .get(TRACE_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(String::from)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let parent_span_id = headers
        .get(SPAN_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    let method = req.method().to_string();
    let path = req.uri().path().to_string();

    let mut tracing_state = TracingState::with_trace_id(trace_id.clone());
    if let Some(span_id) = parent_span_id {
        tracing_state = tracing_state.with_parent_span(span_id);
    }

    let mut req = req;
    req.extensions_mut().insert(tracing_state.clone());

    if req
        .extensions()
        .get::<forge_core::function::AuthContext>()
        .is_none()
    {
        req.extensions_mut()
            .insert(forge_core::function::AuthContext::unauthenticated());
    }

    // Config uses full paths (/_api/health) but axum strips the prefix
    // for nested routers, so the middleware sees /health not /_api/health.
    let full_path = format!("/_api{}", path);
    let is_quiet = quiet_routes.iter().any(|r| *r == full_path || *r == path);

    if is_quiet {
        let mut response = next.run(req).await;
        set_tracing_headers(&mut response, &trace_id, &tracing_state.request_id);
        return response;
    }

    let span = tracing::info_span!(
        "http.request",
        http.method = %method,
        http.route = %path,
        http.status_code = tracing::field::Empty,
        trace_id = %trace_id,
        request_id = %tracing_state.request_id,
    );

    // Link this span to the incoming distributed trace context so
    // fn.execute and all downstream spans share the caller's trace ID.
    #[cfg(feature = "otel")]
    span.set_parent(parent_cx);

    let mut response = next.run(req).instrument(span.clone()).await;

    let status = response.status().as_u16();
    let elapsed = tracing_state.elapsed();

    span.record("http.status_code", status);
    let duration_ms = elapsed.as_millis() as u64;
    match status {
        500..=599 => tracing::error!(parent: &span, duration_ms, "Request failed"),
        400..=499 => tracing::warn!(parent: &span, duration_ms, "Request rejected"),
        200..=299 => tracing::info!(parent: &span, duration_ms, "Request completed"),
        _ => tracing::trace!(parent: &span, duration_ms, "Request completed"),
    }
    crate::observability::record_http_request(&method, &path, status, elapsed.as_secs_f64());

    set_tracing_headers(&mut response, &trace_id, &tracing_state.request_id);
    response
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_gateway_config_default() {
        let config = GatewayConfig::default();
        assert_eq!(config.port, 9081);
        assert_eq!(config.max_connections, 512);
        assert!(!config.cors_enabled);
    }

    #[test]
    fn test_health_response_serialization() {
        let resp = HealthResponse {
            status: "healthy".to_string(),
            version: "0.1.0".to_string(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("healthy"));
    }
}
