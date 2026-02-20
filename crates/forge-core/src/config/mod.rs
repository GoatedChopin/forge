mod cluster;
mod database;

pub use cluster::ClusterConfig;
pub use database::{DatabaseConfig, DatabaseSource};

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
        Ok(())
    }

    /// Load configuration with defaults.
    pub fn default_with_database_url(url: &str) -> Self {
        Self {
            project: ProjectConfig::default(),
            database: DatabaseConfig::remote(url),
            node: NodeConfig::default(),
            gateway: GatewayConfig::default(),
            function: FunctionConfig::default(),
            worker: WorkerConfig::default(),
            cluster: ClusterConfig::default(),
            security: SecurityConfig::default(),
            auth: AuthConfig::default(),
            observability: ObservabilityConfig::default(),
            mcp: McpConfig::default(),
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

    /// gRPC port for inter-node communication.
    #[serde(default = "default_grpc_port")]
    pub grpc_port: u16,

    /// Maximum concurrent connections.
    #[serde(default = "default_max_connections")]
    pub max_connections: usize,

    /// Request timeout in seconds.
    #[serde(default = "default_request_timeout")]
    pub request_timeout_secs: u64,

    /// Enable CORS handling.
    #[serde(default = "default_cors_enabled")]
    pub cors_enabled: bool,

    /// Allowed CORS origins.
    #[serde(default = "default_cors_origins")]
    pub cors_origins: Vec<String>,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            port: default_http_port(),
            grpc_port: default_grpc_port(),
            max_connections: default_max_connections(),
            request_timeout_secs: default_request_timeout(),
            cors_enabled: default_cors_enabled(),
            cors_origins: default_cors_origins(),
        }
    }
}

fn default_http_port() -> u16 {
    8080
}

fn default_grpc_port() -> u16 {
    9000
}

fn default_max_connections() -> usize {
    512
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

/// Function execution configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionConfig {
    /// Maximum concurrent function executions.
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent: usize,

    /// Function timeout in seconds.
    #[serde(default = "default_function_timeout")]
    pub timeout_secs: u64,

    /// Memory limit per function (in bytes).
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

    /// Token expiry duration (e.g., "15m", "1h", "7d").
    pub token_expiry: Option<String>,

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
            token_expiry: None,
            jwks_url: None,
            jwks_cache_ttl_secs: default_jwks_cache_ttl(),
            session_ttl_secs: default_session_ttl(),
        }
    }
}

impl AuthConfig {
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
            log_level: default_log_level(),
        }
    }
}

fn default_otlp_endpoint() -> String {
    "http://localhost:4317".to_string()
}

fn default_true() -> bool {
    true
}

fn default_sampling_ratio() -> f64 {
    1.0
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
            path: default_mcp_path(),
            session_ttl_secs: default_mcp_session_ttl_secs(),
            allowed_origins: Vec::new(),
            require_protocol_version_header: default_true(),
        }
    }
}

impl McpConfig {
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

/// Substitute environment variables in the format ${VAR_NAME}.
fn substitute_env_vars(content: &str) -> String {
    let mut result = content.to_string();
    let re = regex_lite::Regex::new(r"\$\{([A-Z_][A-Z0-9_]*)\}").expect("valid regex pattern");

    for cap in re.captures_iter(content) {
        let var_name = &cap[1];
        if let Ok(value) = std::env::var(var_name) {
            result = result.replace(&cap[0], &value);
        }
    }

    result
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, unsafe_code)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ForgeConfig::default_with_database_url("postgres://localhost/test");
        assert_eq!(config.gateway.port, 8080);
        assert_eq!(config.node.roles.len(), 4);
        assert_eq!(config.mcp.path, "/mcp");
        assert!(!config.mcp.enabled);
    }

    #[test]
    fn test_parse_minimal_config() {
        let toml = r#"
            [database]
            mode = "remote"
            url = "postgres://localhost/myapp"
        "#;

        let config = ForgeConfig::parse_toml(toml).unwrap();
        assert_eq!(config.database.url(), Some("postgres://localhost/myapp"));
        assert_eq!(config.gateway.port, 8080);
    }

    #[test]
    fn test_parse_full_config() {
        let toml = r#"
            [project]
            name = "my-app"
            version = "1.0.0"

            [database]
            mode = "remote"
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
            mode = "remote"
            url = "${TEST_DB_URL}"
        "#;

        let config = ForgeConfig::parse_toml(toml).unwrap();
        assert_eq!(
            config.database.url(),
            Some("postgres://test:test@localhost/test")
        );

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
            mode = "remote"
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
            mode = "remote"
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
    fn test_mcp_config_validation_rejects_invalid_path() {
        let toml = r#"
            [database]
            mode = "remote"
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
}
