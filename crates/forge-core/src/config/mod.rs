pub mod cluster;
mod database;
pub mod signals;

pub use cluster::ClusterConfig;
pub use database::{DatabaseConfig, PoolConfig};
pub use signals::SignalsConfig;

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::error::{ForgeError, Result};

/// Root configuration for FORGE.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgeConfig {
    /// Project metadata.
    #[serde(default)]
    pub project: ProjectConfig,

    /// Database configuration.
    pub database: DatabaseConfig,

    /// Node configuration.
    #[serde(default)]
    pub node: NodeConfig,

    /// Gateway configuration.
    #[serde(default)]
    pub gateway: GatewayConfig,

    /// Function execution configuration.
    #[serde(default)]
    pub function: FunctionConfig,

    /// Worker configuration.
    #[serde(default)]
    pub worker: WorkerConfig,

    /// Cluster configuration.
    #[serde(default)]
    pub cluster: ClusterConfig,

    /// Security configuration.
    #[serde(default)]
    pub security: SecurityConfig,

    /// Authentication configuration.
    #[serde(default)]
    pub auth: AuthConfig,

    /// Observability configuration.
    #[serde(default)]
    pub observability: ObservabilityConfig,

    /// MCP server configuration.
    #[serde(default)]
    pub mcp: McpConfig,

    /// Signals configuration for product analytics and diagnostics.
    #[serde(default)]
    pub signals: SignalsConfig,
}

impl ForgeConfig {
    /// Load configuration from a TOML file.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let content = std::fs::read_to_string(path.as_ref())
            .map_err(|e| ForgeError::Config(format!("Failed to read config file: {}", e)))?;

        Self::parse_toml(&content)
    }

    /// Parse configuration from a TOML string.
    pub fn parse_toml(content: &str) -> Result<Self> {
        // Substitute environment variables
        let content = substitute_env_vars(content);

        let config: Self = toml::from_str(&content)
            .map_err(|e| ForgeError::Config(format!("Failed to parse config: {}", e)))?;

        config.validate()?;
        Ok(config)
    }

    /// Validate the configuration for invalid combinations.
    pub fn validate(&self) -> Result<()> {
        self.database.validate()?;
        self.auth.validate()?;
        self.mcp.validate()?;
        let body_limit = self.gateway.max_body_size_bytes()?;
        let file_limit = self.gateway.max_file_size_bytes()?;
        if file_limit > body_limit {
            return Err(ForgeError::Config(format!(
                "gateway.max_file_size ({}) cannot exceed gateway.max_body_size ({})",
                self.gateway.max_file_size, self.gateway.max_body_size
            )));
        }
        self.gateway.tls.validate()?;

        // Cross-field: OAuth requires jwt_secret for signing tokens
        if self.mcp.oauth && self.auth.jwt_secret.is_none() {
            return Err(ForgeError::Config(
                "mcp.oauth = true requires auth.jwt_secret to be set. \
                 OAuth-issued tokens are signed with this secret, even when using \
                 an external provider (JWKS) for identity verification."
                    .into(),
            ));
        }
        if self.mcp.oauth && !self.mcp.enabled {
            return Err(ForgeError::Config(
                "mcp.oauth = true requires mcp.enabled = true".into(),
            ));
        }

        Ok(())
    }

    /// Load configuration with defaults.
    pub fn default_with_database_url(url: &str) -> Self {
        Self {
            project: ProjectConfig::default(),
            database: DatabaseConfig::new(url),
            node: NodeConfig::default(),
            gateway: GatewayConfig::default(),
            function: FunctionConfig::default(),
            worker: WorkerConfig::default(),
            cluster: ClusterConfig::default(),
            security: SecurityConfig::default(),
            auth: AuthConfig::default(),
            observability: ObservabilityConfig::default(),
            mcp: McpConfig::default(),
            signals: SignalsConfig::default(),
        }
    }
}

/// Project metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    /// Project name.
    #[serde(default = "default_project_name")]
    pub name: String,

    /// Project version.
    #[serde(default = "default_version")]
    pub version: String,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            name: default_project_name(),
            version: default_version(),
        }
    }
}

fn default_project_name() -> String {
    "forge-app".to_string()
}

fn default_version() -> String {
    "0.1.0".to_string()
}

/// Node role configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    /// Roles this node should assume.
    #[serde(default = "default_roles")]
    pub roles: Vec<NodeRole>,

    /// Worker capabilities for job routing.
    #[serde(default = "default_capabilities")]
    pub worker_capabilities: Vec<String>,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            roles: default_roles(),
            worker_capabilities: default_capabilities(),
        }
    }
}

fn default_roles() -> Vec<NodeRole> {
    vec![
        NodeRole::Gateway,
        NodeRole::Function,
        NodeRole::Worker,
        NodeRole::Scheduler,
    ]
}

fn default_capabilities() -> Vec<String> {
    vec!["general".to_string()]
}

/// Available node roles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeRole {
    Gateway,
    Function,
    Worker,
    Scheduler,
}

/// Gateway configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayConfig {
    /// HTTP port.
    #[serde(default = "default_http_port")]
    pub port: u16,

    /// gRPC port for inter-node communication (reserved for future use).
    ///
    /// This port is registered in the cluster node info but a gRPC listener
    /// is not yet started. It will be used for efficient binary inter-node
    /// RPC in a future release.
    #[serde(default = "default_grpc_port")]
    pub grpc_port: u16,

    /// Maximum concurrent connections.
    #[serde(default = "default_max_connections")]
    pub max_connections: usize,

    /// Maximum active SSE sessions.
    #[serde(default = "default_sse_max_sessions")]
    pub sse_max_sessions: usize,

    /// Request timeout in seconds.
    #[serde(default = "default_request_timeout")]
    pub request_timeout_secs: u64,

    /// Enable CORS handling.
    #[serde(default = "default_cors_enabled")]
    pub cors_enabled: bool,

    /// Allowed CORS origins.
    #[serde(default = "default_cors_origins")]
    pub cors_origins: Vec<String>,

    /// Routes excluded from request logs, metrics, and traces.
    /// Defaults to `["/_api/health", "/_api/ready"]`. Set to `[]` to monitor everything.
    #[serde(default = "default_quiet_routes")]
    pub quiet_routes: Vec<String>,

    /// Maximum request body size (e.g. "100mb", "1gb"). Defaults to "20mb".
    #[serde(default = "default_max_body_size")]
    pub max_body_size: String,

    /// Default per-file cap for multipart uploads (e.g. "10mb", "200mb").
    /// Applies when a mutation does not declare its own `max_size`. Set to
    /// the same value as `max_body_size` to disable the per-file guard.
    /// Defaults to "10mb".
    #[serde(default = "default_max_file_size")]
    pub max_file_size: String,

    /// TLS configuration for the gateway listener.
    #[serde(default)]
    pub tls: TlsConfig,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            port: default_http_port(),
            grpc_port: default_grpc_port(),
            max_connections: default_max_connections(),
            sse_max_sessions: default_sse_max_sessions(),
            request_timeout_secs: default_request_timeout(),
            cors_enabled: default_cors_enabled(),
            cors_origins: default_cors_origins(),
            quiet_routes: default_quiet_routes(),
            max_body_size: default_max_body_size(),
            max_file_size: default_max_file_size(),
            tls: TlsConfig::default(),
        }
    }
}

impl GatewayConfig {
    /// Parse `max_body_size` into bytes.
    pub fn max_body_size_bytes(&self) -> crate::Result<usize> {
        crate::util::parse_size(&self.max_body_size).ok_or_else(|| {
            crate::ForgeError::Config(format!(
                "invalid gateway.max_body_size '{}'. Expected a size like '20mb', '1gb', or '1048576'",
                self.max_body_size
            ))
        })
    }

    /// Parse `max_file_size` into bytes.
    pub fn max_file_size_bytes(&self) -> crate::Result<usize> {
        crate::util::parse_size(&self.max_file_size).ok_or_else(|| {
            crate::ForgeError::Config(format!(
                "invalid gateway.max_file_size '{}'. Expected a size like '10mb', '200mb', or '1048576'",
                self.max_file_size
            ))
        })
    }
}

/// TLS configuration for the gateway listener.
///
/// Two modes are supported, inferred from which fields are set:
///
/// - **Self-signed**: `enabled = true` with no `cert_path`/`key_path`. The runtime
///   generates an ephemeral self-signed certificate at startup. Intended for
///   zero-trust deployments behind a load balancer that terminates public TLS.
/// - **File-based**: `enabled = true` with both `cert_path` and `key_path` set.
///   The runtime loads the PEM-encoded certificate chain and private key from
///   disk at startup.
///
/// Setting only one of `cert_path`/`key_path` is a configuration error.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    /// Enable TLS on the gateway listener.
    #[serde(default)]
    pub enabled: bool,

    /// Path to a PEM-encoded certificate chain file. Omit for self-signed mode.
    #[serde(default)]
    pub cert_path: Option<String>,

    /// Path to a PEM-encoded private key file. Omit for self-signed mode.
    #[serde(default)]
    pub key_path: Option<String>,

    /// Subject Alternative Names (SANs) for the self-signed certificate.
    /// Defaults to `["localhost"]`. Ignored when `cert_path` and `key_path`
    /// are set.
    #[serde(default = "default_tls_hostnames")]
    pub hostnames: Vec<String>,
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            cert_path: None,
            key_path: None,
            hostnames: default_tls_hostnames(),
        }
    }
}

impl TlsConfig {
    /// Validate the TLS configuration.
    ///
    /// When `enabled` is `false`, no other fields are inspected. When `enabled`
    /// is `true`, either both `cert_path` and `key_path` must be non-empty
    /// (file-based mode) or both must be absent (self-signed mode). Mixing
    /// the two, or supplying empty strings, is rejected.
    pub fn validate(&self) -> crate::Result<()> {
        if !self.enabled {
            return Ok(());
        }

        if let Some(cert) = self.cert_path.as_deref()
            && cert.trim().is_empty()
        {
            return Err(crate::ForgeError::Config(
                "gateway.tls.cert_path must be non-empty when set".into(),
            ));
        }
        if let Some(key) = self.key_path.as_deref()
            && key.trim().is_empty()
        {
            return Err(crate::ForgeError::Config(
                "gateway.tls.key_path must be non-empty when set".into(),
            ));
        }

        match (self.cert_path.as_deref(), self.key_path.as_deref()) {
            (Some(_), Some(_)) => Ok(()),
            (None, None) => {
                if self.hostnames.is_empty() {
                    return Err(crate::ForgeError::Config(
                        "gateway.tls.hostnames must contain at least one entry when \
                         using self-signed mode"
                            .into(),
                    ));
                }
                if self.hostnames.iter().any(|h| h.trim().is_empty()) {
                    return Err(crate::ForgeError::Config(
                        "gateway.tls.hostnames entries must be non-empty".into(),
                    ));
                }
                Ok(())
            }
            (Some(_), None) => Err(crate::ForgeError::Config(
                "gateway.tls.cert_path is set but gateway.tls.key_path is missing. \
                 Set both for file-based TLS, or neither for a self-signed certificate."
                    .into(),
            )),
            (None, Some(_)) => Err(crate::ForgeError::Config(
                "gateway.tls.key_path is set but gateway.tls.cert_path is missing. \
                 Set both for file-based TLS, or neither for a self-signed certificate."
                    .into(),
            )),
        }
    }
}

fn default_tls_hostnames() -> Vec<String> {
    vec!["localhost".to_string()]
}

fn default_http_port() -> u16 {
    9081
}

fn default_grpc_port() -> u16 {
    9000
}

fn default_max_connections() -> usize {
    4096
}

fn default_sse_max_sessions() -> usize {
    10_000
}

fn default_request_timeout() -> u64 {
    30
}

fn default_cors_enabled() -> bool {
    false
}

fn default_cors_origins() -> Vec<String> {
    Vec::new()
}

fn default_quiet_routes() -> Vec<String> {
    vec![
        "/_api/health".to_string(),
        "/_api/ready".to_string(),
        "/_api/signal/event".to_string(),
        "/_api/signal/view".to_string(),
        "/_api/signal/user".to_string(),
        "/_api/signal/report".to_string(),
    ]
}

fn default_max_body_size() -> String {
    "20mb".to_string()
}

fn default_max_file_size() -> String {
    "10mb".to_string()
}

/// Function execution configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionConfig {
    /// Maximum concurrent function executions.
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent: usize,

    /// Function timeout in seconds.
    #[serde(default = "default_function_timeout")]
    pub timeout_secs: u64,

    /// Advisory memory limit per function execution (in bytes).
    ///
    /// This value is exposed as configuration metadata for orchestrators
    /// (e.g., Kubernetes resource requests) and observability dashboards.
    /// It is not enforced at the process level since Rust does not provide
    /// per-function memory sandboxing. Use container-level limits for hard
    /// enforcement.
    #[serde(default = "default_memory_limit")]
    pub memory_limit: usize,
}

impl Default for FunctionConfig {
    fn default() -> Self {
        Self {
            max_concurrent: default_max_concurrent(),
            timeout_secs: default_function_timeout(),
            memory_limit: default_memory_limit(),
        }
    }
}

fn default_max_concurrent() -> usize {
    1000
}

fn default_function_timeout() -> u64 {
    30
}

fn default_memory_limit() -> usize {
    512 * 1024 * 1024 // 512 MiB
}

/// Worker configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerConfig {
    /// Maximum concurrent jobs.
    #[serde(default = "default_max_concurrent_jobs")]
    pub max_concurrent_jobs: usize,

    /// Job timeout in seconds.
    #[serde(default = "default_job_timeout")]
    pub job_timeout_secs: u64,

    /// Poll interval in milliseconds.
    #[serde(default = "default_poll_interval")]
    pub poll_interval_ms: u64,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            max_concurrent_jobs: default_max_concurrent_jobs(),
            job_timeout_secs: default_job_timeout(),
            poll_interval_ms: default_poll_interval(),
        }
    }
}

fn default_max_concurrent_jobs() -> usize {
    50
}

fn default_job_timeout() -> u64 {
    3600 // 1 hour
}

fn default_poll_interval() -> u64 {
    100
}

/// Security configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SecurityConfig {
    /// Secret key for signing.
    pub secret_key: Option<String>,
}

/// JWT signing algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum JwtAlgorithm {
    /// HMAC using SHA-256 (symmetric, requires jwt_secret).
    #[default]
    HS256,
    /// HMAC using SHA-384 (symmetric, requires jwt_secret).
    HS384,
    /// HMAC using SHA-512 (symmetric, requires jwt_secret).
    HS512,
    /// RSA using SHA-256 (asymmetric, requires jwks_url).
    RS256,
    /// RSA using SHA-384 (asymmetric, requires jwks_url).
    RS384,
    /// RSA using SHA-512 (asymmetric, requires jwks_url).
    RS512,
}

/// Authentication configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// JWT secret for HMAC algorithms (HS256, HS384, HS512).
    /// Required when using HMAC algorithms.
    pub jwt_secret: Option<String>,

    /// JWT signing algorithm.
    /// HMAC algorithms (HS256, HS384, HS512) require jwt_secret.
    /// RSA algorithms (RS256, RS384, RS512) require jwks_url.
    #[serde(default)]
    pub jwt_algorithm: JwtAlgorithm,

    /// Expected token issuer (iss claim).
    /// If set, tokens with a different issuer are rejected.
    pub jwt_issuer: Option<String>,

    /// Expected audience (aud claim).
    /// If set, tokens with a different audience are rejected.
    pub jwt_audience: Option<String>,

    /// Access token lifetime (e.g., "15m", "1h").
    /// Used by `ctx.issue_token_pair()`. Defaults to "1h".
    pub access_token_ttl: Option<String>,

    /// Refresh token lifetime (e.g., "7d", "30d").
    /// Used by `ctx.issue_token_pair()`. Defaults to "30d".
    pub refresh_token_ttl: Option<String>,

    /// JWKS URL for RSA algorithms (RS256, RS384, RS512).
    /// Keys are fetched and cached automatically.
    pub jwks_url: Option<String>,

    /// JWKS cache TTL in seconds.
    #[serde(default = "default_jwks_cache_ttl")]
    pub jwks_cache_ttl_secs: u64,

    /// Session TTL in seconds (for WebSocket sessions).
    #[serde(default = "default_session_ttl")]
    pub session_ttl_secs: u64,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            jwt_secret: None,
            jwt_algorithm: JwtAlgorithm::default(),
            jwt_issuer: None,
            jwt_audience: None,
            access_token_ttl: None,
            refresh_token_ttl: None,
            jwks_url: None,
            jwks_cache_ttl_secs: default_jwks_cache_ttl(),
            session_ttl_secs: default_session_ttl(),
        }
    }
}

impl AuthConfig {
    /// Resolved access token TTL in seconds.
    /// Parses `access_token_ttl`, default 3600s (1h).
    /// Minimum 1 second to prevent zero-lifetime tokens.
    pub fn access_token_ttl_secs(&self) -> i64 {
        self.access_token_ttl
            .as_deref()
            .and_then(crate::util::parse_duration)
            .map(|d| (d.as_secs() as i64).max(1))
            .unwrap_or(3600)
    }

    /// Resolved refresh token TTL in days.
    /// Parses `refresh_token_ttl`, default 30 days.
    pub fn refresh_token_ttl_days(&self) -> i64 {
        self.refresh_token_ttl
            .as_deref()
            .and_then(crate::util::parse_duration)
            .map(|d| (d.as_secs() / 86400) as i64)
            .map(|d| if d == 0 { 1 } else { d })
            .unwrap_or(30)
    }

    /// Check if auth is configured (any credential or claim validation is set).
    fn is_configured(&self) -> bool {
        self.jwt_secret.is_some()
            || self.jwks_url.is_some()
            || self.jwt_issuer.is_some()
            || self.jwt_audience.is_some()
    }

    /// Validate that the configuration is complete for the chosen algorithm.
    /// Skips validation if no auth settings are configured (auth disabled).
    pub fn validate(&self) -> Result<()> {
        if !self.is_configured() {
            return Ok(());
        }

        match self.jwt_algorithm {
            JwtAlgorithm::HS256 | JwtAlgorithm::HS384 | JwtAlgorithm::HS512 => {
                if self.jwt_secret.is_none() {
                    return Err(ForgeError::Config(
                        "auth.jwt_secret is required for HMAC algorithms (HS256, HS384, HS512). \
                         Set auth.jwt_secret to a secure random string, \
                         or switch to RS256 and provide auth.jwks_url for external identity providers."
                            .into(),
                    ));
                }
            }
            JwtAlgorithm::RS256 | JwtAlgorithm::RS384 | JwtAlgorithm::RS512 => {
                if self.jwks_url.is_none() {
                    return Err(ForgeError::Config(
                        "auth.jwks_url is required for RSA algorithms (RS256, RS384, RS512). \
                         Set auth.jwks_url to your identity provider's JWKS endpoint, \
                         or switch to HS256 and provide auth.jwt_secret for symmetric signing."
                            .into(),
                    ));
                }
            }
        }
        Ok(())
    }

    /// Check if this config uses HMAC (symmetric) algorithms.
    pub fn is_hmac(&self) -> bool {
        matches!(
            self.jwt_algorithm,
            JwtAlgorithm::HS256 | JwtAlgorithm::HS384 | JwtAlgorithm::HS512
        )
    }

    /// Check if this config uses RSA (asymmetric) algorithms.
    pub fn is_rsa(&self) -> bool {
        matches!(
            self.jwt_algorithm,
            JwtAlgorithm::RS256 | JwtAlgorithm::RS384 | JwtAlgorithm::RS512
        )
    }
}

fn default_jwks_cache_ttl() -> u64 {
    3600 // 1 hour
}

fn default_session_ttl() -> u64 {
    7 * 24 * 60 * 60 // 7 days
}

/// Observability configuration for OTLP telemetry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservabilityConfig {
    /// Enable observability (traces, metrics, logs).
    #[serde(default)]
    pub enabled: bool,

    /// OTLP endpoint for telemetry export.
    #[serde(default = "default_otlp_endpoint")]
    pub otlp_endpoint: String,

    /// Service name for telemetry identification.
    pub service_name: Option<String>,

    /// Enable distributed tracing.
    #[serde(default = "default_true")]
    pub enable_traces: bool,

    /// Enable metrics collection.
    #[serde(default = "default_true")]
    pub enable_metrics: bool,

    /// Enable log export via OTLP.
    #[serde(default = "default_true")]
    pub enable_logs: bool,

    /// Trace sampling ratio (0.0 to 1.0).
    #[serde(default = "default_sampling_ratio")]
    pub sampling_ratio: f64,

    /// Metrics export interval in seconds. OTLP collectors typically prefer 15s-60s.
    #[serde(default = "default_metrics_interval_secs")]
    pub metrics_interval_secs: u64,

    /// Log level for the tracing subscriber (e.g., "debug", "info", "warn").
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            otlp_endpoint: default_otlp_endpoint(),
            service_name: None,
            enable_traces: true,
            enable_metrics: true,
            enable_logs: true,
            sampling_ratio: default_sampling_ratio(),
            metrics_interval_secs: default_metrics_interval_secs(),
            log_level: default_log_level(),
        }
    }
}

impl ObservabilityConfig {
    pub fn otlp_active(&self) -> bool {
        self.enabled && (self.enable_traces || self.enable_metrics || self.enable_logs)
    }
}

fn default_otlp_endpoint() -> String {
    "http://localhost:4318".to_string()
}

pub(crate) fn default_true() -> bool {
    true
}

fn default_sampling_ratio() -> f64 {
    1.0
}

fn default_metrics_interval_secs() -> u64 {
    15
}

fn default_log_level() -> String {
    "info".to_string()
}

/// MCP server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpConfig {
    /// Enable MCP endpoint exposure.
    #[serde(default)]
    pub enabled: bool,

    /// Enable OAuth 2.1 Authorization Code + PKCE for MCP clients.
    /// When true, Forge acts as an OAuth 2.1 Authorization Server so MCP
    /// clients like Claude Code can auto-authenticate via browser login.
    /// Requires `auth.jwt_secret` to be set.
    #[serde(default)]
    pub oauth: bool,

    /// MCP endpoint path under the gateway API namespace.
    #[serde(default = "default_mcp_path")]
    pub path: String,

    /// Session TTL in seconds.
    #[serde(default = "default_mcp_session_ttl_secs")]
    pub session_ttl_secs: u64,

    /// Allowed origins for Origin header validation.
    #[serde(default)]
    pub allowed_origins: Vec<String>,

    /// Enforce MCP-Protocol-Version header on post-initialize requests.
    #[serde(default = "default_true")]
    pub require_protocol_version_header: bool,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            oauth: false,
            path: default_mcp_path(),
            session_ttl_secs: default_mcp_session_ttl_secs(),
            allowed_origins: Vec::new(),
            require_protocol_version_header: default_true(),
        }
    }
}

impl McpConfig {
    /// Paths reserved by the gateway that MCP must not collide with.
    const RESERVED_PATHS: &[&str] = &[
        "/health",
        "/ready",
        "/rpc",
        "/events",
        "/subscribe",
        "/unsubscribe",
        "/subscribe-job",
        "/subscribe-workflow",
        "/metrics",
    ];

    pub fn validate(&self) -> Result<()> {
        if self.path.is_empty() || !self.path.starts_with('/') {
            return Err(ForgeError::Config(
                "mcp.path must start with '/' (example: /mcp)".to_string(),
            ));
        }
        if self.path.contains(' ') {
            return Err(ForgeError::Config(
                "mcp.path cannot contain spaces".to_string(),
            ));
        }
        if Self::RESERVED_PATHS.contains(&self.path.as_str()) {
            return Err(ForgeError::Config(format!(
                "mcp.path '{}' conflicts with a reserved gateway route",
                self.path
            )));
        }
        if self.session_ttl_secs == 0 {
            return Err(ForgeError::Config(
                "mcp.session_ttl_secs must be greater than 0".to_string(),
            ));
        }
        Ok(())
    }
}

fn default_mcp_path() -> String {
    "/mcp".to_string()
}

fn default_mcp_session_ttl_secs() -> u64 {
    60 * 60
}

/// Substitute environment variables in the format `${VAR_NAME}`.
///
/// Supports default values with `${VAR-default}` or `${VAR:-default}`.
/// When the env var is unset, the default is used. Without a default,
/// the literal `${VAR}` is preserved (so TOML parsing can still fail
/// loudly if a required variable is missing).
#[allow(clippy::indexing_slicing)]
pub fn substitute_env_vars(content: &str) -> String {
    let mut result = String::with_capacity(content.len());
    let bytes = content.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if i + 1 < len
            && bytes[i] == b'$'
            && bytes[i + 1] == b'{'
            && let Some(end) = content[i + 2..].find('}')
        {
            let inner = &content[i + 2..i + 2 + end];

            // Split on first `-` or `:-` for default value support
            let (var_name, default_value) = parse_var_with_default(inner);

            if is_valid_env_var_name(var_name) {
                if let Ok(value) = std::env::var(var_name) {
                    result.push_str(&value);
                } else if let Some(default) = default_value {
                    result.push_str(default);
                } else {
                    result.push_str(&content[i..i + 2 + end + 1]);
                }
                i += 2 + end + 1;
                continue;
            }
        }
        result.push(bytes[i] as char);
        i += 1;
    }

    result
}

/// Parse `VAR-default` or `VAR:-default` into (name, optional default).
/// Both forms behave identically (fallback when unset). `:-` is checked
/// first so its `-` doesn't get matched by the plain `-` branch.
fn parse_var_with_default(inner: &str) -> (&str, Option<&str>) {
    if let Some(pos) = inner.find(":-") {
        return (&inner[..pos], Some(&inner[pos + 2..]));
    }
    if let Some(pos) = inner.find('-') {
        return (&inner[..pos], Some(&inner[pos + 1..]));
    }
    (inner, None)
}

fn is_valid_env_var_name(name: &str) -> bool {
    let first = match name.as_bytes().first() {
        Some(b) => b,
        None => return false,
    };
    (first.is_ascii_uppercase() || *first == b'_')
        && name
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, unsafe_code)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ForgeConfig::default_with_database_url("postgres://localhost/test");
        assert_eq!(config.gateway.port, 9081);
        assert_eq!(config.node.roles.len(), 4);
        assert_eq!(config.mcp.path, "/mcp");
        assert!(!config.mcp.enabled);
    }

    #[test]
    fn test_parse_minimal_config() {
        let toml = r#"
            [database]
            url = "postgres://localhost/myapp"
        "#;

        let config = ForgeConfig::parse_toml(toml).unwrap();
        assert_eq!(config.database.url(), "postgres://localhost/myapp");
        assert_eq!(config.gateway.port, 9081);
    }

    #[test]
    fn test_parse_full_config() {
        let toml = r#"
            [project]
            name = "my-app"
            version = "1.0.0"

            [database]
            url = "postgres://localhost/myapp"
            pool_size = 100

            [node]
            roles = ["gateway", "worker"]
            worker_capabilities = ["media", "general"]

            [gateway]
            port = 3000
            grpc_port = 9001
        "#;

        let config = ForgeConfig::parse_toml(toml).unwrap();
        assert_eq!(config.project.name, "my-app");
        assert_eq!(config.database.pool_size, 100);
        assert_eq!(config.node.roles.len(), 2);
        assert_eq!(config.gateway.port, 3000);
    }

    #[test]
    fn test_env_var_substitution() {
        unsafe {
            std::env::set_var("TEST_DB_URL", "postgres://test:test@localhost/test");
        }

        let toml = r#"
            [database]
            url = "${TEST_DB_URL}"
        "#;

        let config = ForgeConfig::parse_toml(toml).unwrap();
        assert_eq!(config.database.url(), "postgres://test:test@localhost/test");

        unsafe {
            std::env::remove_var("TEST_DB_URL");
        }
    }

    #[test]
    fn test_auth_validation_no_config() {
        let auth = AuthConfig::default();
        assert!(auth.validate().is_ok());
    }

    #[test]
    fn test_auth_validation_hmac_with_secret() {
        let auth = AuthConfig {
            jwt_secret: Some("my-secret".into()),
            jwt_algorithm: JwtAlgorithm::HS256,
            ..Default::default()
        };
        assert!(auth.validate().is_ok());
    }

    #[test]
    fn test_auth_validation_hmac_missing_secret() {
        let auth = AuthConfig {
            jwt_issuer: Some("my-issuer".into()),
            jwt_algorithm: JwtAlgorithm::HS256,
            ..Default::default()
        };
        let result = auth.validate();
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("jwt_secret is required"));
    }

    #[test]
    fn test_auth_validation_rsa_with_jwks() {
        let auth = AuthConfig {
            jwks_url: Some("https://example.com/.well-known/jwks.json".into()),
            jwt_algorithm: JwtAlgorithm::RS256,
            ..Default::default()
        };
        assert!(auth.validate().is_ok());
    }

    #[test]
    fn test_auth_validation_rsa_missing_jwks() {
        let auth = AuthConfig {
            jwt_issuer: Some("my-issuer".into()),
            jwt_algorithm: JwtAlgorithm::RS256,
            ..Default::default()
        };
        let result = auth.validate();
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("jwks_url is required"));
    }

    #[test]
    fn test_forge_config_validation_fails_on_empty_url() {
        let toml = r#"
            [database]

            url = ""
        "#;

        let result = ForgeConfig::parse_toml(toml);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("database.url is required"));
    }

    #[test]
    fn test_forge_config_validation_fails_on_invalid_auth() {
        let toml = r#"
            [database]

            url = "postgres://localhost/test"

            [auth]
            jwt_issuer = "my-issuer"
            jwt_algorithm = "RS256"
        "#;

        let result = ForgeConfig::parse_toml(toml);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("jwks_url is required"));
    }

    #[test]
    fn test_env_var_default_used_when_unset() {
        // Ensure the var is definitely not set
        unsafe {
            std::env::remove_var("TEST_FORGE_OTEL_UNSET");
        }

        let input = r#"enabled = ${TEST_FORGE_OTEL_UNSET-false}"#;
        let result = substitute_env_vars(input);
        assert_eq!(result, "enabled = false");
    }

    #[test]
    fn test_env_var_default_overridden_when_set() {
        unsafe {
            std::env::set_var("TEST_FORGE_OTEL_SET", "true");
        }

        let input = r#"enabled = ${TEST_FORGE_OTEL_SET-false}"#;
        let result = substitute_env_vars(input);
        assert_eq!(result, "enabled = true");

        unsafe {
            std::env::remove_var("TEST_FORGE_OTEL_SET");
        }
    }

    #[test]
    fn test_env_var_colon_dash_default() {
        unsafe {
            std::env::remove_var("TEST_FORGE_ENDPOINT_UNSET");
        }

        let input = r#"endpoint = "${TEST_FORGE_ENDPOINT_UNSET:-http://localhost:4318}""#;
        let result = substitute_env_vars(input);
        assert_eq!(result, r#"endpoint = "http://localhost:4318""#);
    }

    #[test]
    fn test_env_var_no_default_preserves_literal() {
        unsafe {
            std::env::remove_var("TEST_FORGE_MISSING");
        }

        let input = r#"url = "${TEST_FORGE_MISSING}""#;
        let result = substitute_env_vars(input);
        assert_eq!(result, r#"url = "${TEST_FORGE_MISSING}""#);
    }

    #[test]
    fn test_env_var_default_empty_string() {
        unsafe {
            std::env::remove_var("TEST_FORGE_EMPTY_DEFAULT");
        }

        let input = r#"val = "${TEST_FORGE_EMPTY_DEFAULT-}""#;
        let result = substitute_env_vars(input);
        assert_eq!(result, r#"val = """#);
    }

    #[test]
    fn test_observability_config_default_disabled() {
        let toml = r#"
            [database]
            url = "postgres://localhost/test"
        "#;

        let config = ForgeConfig::parse_toml(toml).unwrap();
        assert!(!config.observability.enabled);
        assert!(!config.observability.otlp_active());
    }

    #[test]
    fn test_observability_config_with_env_default() {
        // Simulates what the template produces when no env vars are set
        unsafe {
            std::env::remove_var("TEST_OTEL_ENABLED");
        }

        let toml = r#"
            [database]
            url = "postgres://localhost/test"

            [observability]
            enabled = ${TEST_OTEL_ENABLED-false}
        "#;

        let config = ForgeConfig::parse_toml(toml).unwrap();
        assert!(!config.observability.enabled);
    }

    #[test]
    fn test_mcp_config_validation_rejects_invalid_path() {
        let toml = r#"
            [database]

            url = "postgres://localhost/test"

            [mcp]
            enabled = true
            path = "mcp"
        "#;

        let result = ForgeConfig::parse_toml(toml);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("mcp.path must start with '/'"));
    }

    #[test]
    fn test_access_token_ttl_defaults() {
        let auth = AuthConfig::default();
        assert_eq!(auth.access_token_ttl_secs(), 3600);
        assert_eq!(auth.refresh_token_ttl_days(), 30);
    }

    #[test]
    fn test_access_token_ttl_custom() {
        let auth = AuthConfig {
            access_token_ttl: Some("15m".into()),
            refresh_token_ttl: Some("7d".into()),
            ..Default::default()
        };
        assert_eq!(auth.access_token_ttl_secs(), 900);
        assert_eq!(auth.refresh_token_ttl_days(), 7);
    }

    #[test]
    fn test_access_token_ttl_minimum_enforced() {
        let auth = AuthConfig {
            access_token_ttl: Some("0s".into()),
            ..Default::default()
        };
        // Should floor at 1, not 0
        assert_eq!(auth.access_token_ttl_secs(), 1);
    }

    #[test]
    fn test_refresh_token_ttl_minimum_enforced() {
        let auth = AuthConfig {
            refresh_token_ttl: Some("1h".into()),
            ..Default::default()
        };
        // 1 hour < 1 day, so should floor at 1 day
        assert_eq!(auth.refresh_token_ttl_days(), 1);
    }

    #[test]
    fn test_max_body_size_defaults() {
        let gw = GatewayConfig::default();
        assert_eq!(gw.max_body_size_bytes().unwrap(), 20 * 1024 * 1024);
    }

    #[test]
    fn test_max_body_size_custom() {
        let gw = GatewayConfig {
            max_body_size: "100mb".into(),
            ..Default::default()
        };
        assert_eq!(gw.max_body_size_bytes().unwrap(), 100 * 1024 * 1024);
    }

    #[test]
    fn test_max_body_size_invalid_errors() {
        let gw = GatewayConfig {
            max_body_size: "not-a-size".into(),
            ..Default::default()
        };
        assert!(gw.max_body_size_bytes().is_err());
    }

    #[test]
    fn test_max_file_size_defaults() {
        let gw = GatewayConfig::default();
        assert_eq!(gw.max_file_size_bytes().unwrap(), 10 * 1024 * 1024);
    }

    #[test]
    fn test_max_file_size_custom() {
        let gw = GatewayConfig {
            max_file_size: "200mb".into(),
            max_body_size: "500mb".into(),
            ..Default::default()
        };
        assert_eq!(gw.max_file_size_bytes().unwrap(), 200 * 1024 * 1024);
    }

    #[test]
    fn test_max_file_size_invalid_errors() {
        let gw = GatewayConfig {
            max_file_size: "nope".into(),
            ..Default::default()
        };
        assert!(gw.max_file_size_bytes().is_err());
    }

    #[test]
    fn test_validate_rejects_file_larger_than_body() {
        let toml = r#"
            [database]
            url = "postgres://localhost/test"

            [gateway]
            max_body_size = "10mb"
            max_file_size = "20mb"
        "#;
        let err = ForgeConfig::parse_toml(toml).unwrap_err().to_string();
        assert!(
            err.contains("max_file_size"),
            "Expected max_file_size error, got: {err}"
        );
    }

    #[test]
    fn test_mcp_config_rejects_reserved_paths() {
        for reserved in McpConfig::RESERVED_PATHS {
            let toml = format!(
                r#"
                [database]
                url = "postgres://localhost/test"

                [mcp]
                enabled = true
                path = "{reserved}"
                "#
            );

            let result = ForgeConfig::parse_toml(&toml);
            assert!(result.is_err(), "Expected {reserved} to be rejected");
            let err_msg = result.unwrap_err().to_string();
            assert!(
                err_msg.contains("conflicts with a reserved gateway route"),
                "Wrong error for {reserved}: {err_msg}"
            );
        }
    }

    #[test]
    fn test_tls_disabled_default() {
        let config = ForgeConfig::default_with_database_url("postgres://localhost/test");
        assert!(!config.gateway.tls.enabled);
        assert!(config.gateway.tls.cert_path.is_none());
        assert!(config.gateway.tls.key_path.is_none());
        assert_eq!(config.gateway.tls.hostnames, vec!["localhost".to_string()]);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_tls_disabled_ignores_other_fields() {
        let toml = r#"
            [database]
            url = "postgres://localhost/test"

            [gateway.tls]
            enabled = false
            cert_path = ""
        "#;

        assert!(ForgeConfig::parse_toml(toml).is_ok());
    }

    #[test]
    fn test_tls_self_signed_default_hostnames() {
        let toml = r#"
            [database]
            url = "postgres://localhost/test"

            [gateway.tls]
            enabled = true
        "#;

        let config = ForgeConfig::parse_toml(toml).unwrap();
        assert!(config.gateway.tls.enabled);
        assert_eq!(config.gateway.tls.hostnames, vec!["localhost".to_string()]);
    }

    #[test]
    fn test_tls_self_signed_custom_hostnames() {
        let toml = r#"
            [database]
            url = "postgres://localhost/test"

            [gateway.tls]
            enabled = true
            hostnames = ["app.internal", "localhost"]
        "#;

        let config = ForgeConfig::parse_toml(toml).unwrap();
        assert_eq!(
            config.gateway.tls.hostnames,
            vec!["app.internal".to_string(), "localhost".to_string()]
        );
    }

    #[test]
    fn test_tls_file_based_valid() {
        let toml = r#"
            [database]
            url = "postgres://localhost/test"

            [gateway.tls]
            enabled = true
            cert_path = "/etc/forge/cert.pem"
            key_path = "/etc/forge/key.pem"
        "#;

        let config = ForgeConfig::parse_toml(toml).unwrap();
        assert_eq!(
            config.gateway.tls.cert_path.as_deref(),
            Some("/etc/forge/cert.pem")
        );
        assert_eq!(
            config.gateway.tls.key_path.as_deref(),
            Some("/etc/forge/key.pem")
        );
    }

    #[test]
    fn test_tls_only_cert_path_fails() {
        let toml = r#"
            [database]
            url = "postgres://localhost/test"

            [gateway.tls]
            enabled = true
            cert_path = "/etc/forge/cert.pem"
        "#;

        let result = ForgeConfig::parse_toml(toml);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("key_path is missing"),
            "Unexpected error: {err_msg}"
        );
    }

    #[test]
    fn test_tls_only_key_path_fails() {
        let toml = r#"
            [database]
            url = "postgres://localhost/test"

            [gateway.tls]
            enabled = true
            key_path = "/etc/forge/key.pem"
        "#;

        let result = ForgeConfig::parse_toml(toml);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("cert_path is missing"),
            "Unexpected error: {err_msg}"
        );
    }

    #[test]
    fn test_tls_empty_cert_path_fails() {
        let toml = r#"
            [database]
            url = "postgres://localhost/test"

            [gateway.tls]
            enabled = true
            cert_path = ""
            key_path = "/etc/forge/key.pem"
        "#;

        let result = ForgeConfig::parse_toml(toml);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("cert_path must be non-empty"),
            "Unexpected error: {err_msg}"
        );
    }

    #[test]
    fn test_tls_empty_hostnames_fails() {
        let toml = r#"
            [database]
            url = "postgres://localhost/test"

            [gateway.tls]
            enabled = true
            hostnames = []
        "#;

        let result = ForgeConfig::parse_toml(toml);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("hostnames must contain at least one entry"),
            "Unexpected error: {err_msg}"
        );
    }

    #[test]
    fn test_tls_empty_hostname_entry_fails() {
        let toml = r#"
            [database]
            url = "postgres://localhost/test"

            [gateway.tls]
            enabled = true
            hostnames = ["localhost", ""]
        "#;

        let result = ForgeConfig::parse_toml(toml);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("hostnames entries must be non-empty"),
            "Unexpected error: {err_msg}"
        );
    }
}
