mod cluster;
mod database;
mod observability;

pub use cluster::ClusterConfig;
pub use database::DatabaseConfig;
pub use observability::ObservabilityConfig;

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

    /// Observability configuration.
    #[serde(default)]
    pub observability: ObservabilityConfig,

    /// Security configuration.
    #[serde(default)]
    pub security: SecurityConfig,

    /// Authentication configuration.
    #[serde(default)]
    pub auth: AuthConfig,
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

        toml::from_str(&content)
            .map_err(|e| ForgeError::Config(format!("Failed to parse config: {}", e)))
    }

    /// Load configuration with defaults.
    pub fn default_with_database_url(url: &str) -> Self {
        Self {
            project: ProjectConfig::default(),
            database: DatabaseConfig {
                url: url.to_string(),
                ..Default::default()
            },
            node: NodeConfig::default(),
            gateway: GatewayConfig::default(),
            function: FunctionConfig::default(),
            worker: WorkerConfig::default(),
            cluster: ClusterConfig::default(),
            observability: ObservabilityConfig::default(),
            security: SecurityConfig::default(),
            auth: AuthConfig::default(),
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
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            port: default_http_port(),
            grpc_port: default_grpc_port(),
            max_connections: default_max_connections(),
            request_timeout_secs: default_request_timeout(),
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
    10000
}

fn default_request_timeout() -> u64 {
    30
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

    /// Allow unauthenticated requests to reach public functions.
    #[serde(default = "default_true")]
    pub allow_anonymous: bool,

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
            allow_anonymous: true,
            jwks_cache_ttl_secs: default_jwks_cache_ttl(),
            session_ttl_secs: default_session_ttl(),
        }
    }
}

impl AuthConfig {
    /// Validate that the configuration is complete for the chosen algorithm.
    pub fn validate(&self) -> Result<()> {
        match self.jwt_algorithm {
            JwtAlgorithm::HS256 | JwtAlgorithm::HS384 | JwtAlgorithm::HS512 => {
                if self.jwt_secret.is_none() {
                    return Err(ForgeError::Config(
                        "jwt_secret is required for HMAC algorithms (HS256, HS384, HS512)".into(),
                    ));
                }
            }
            JwtAlgorithm::RS256 | JwtAlgorithm::RS384 | JwtAlgorithm::RS512 => {
                if self.jwks_url.is_none() {
                    return Err(ForgeError::Config(
                        "jwks_url is required for RSA algorithms (RS256, RS384, RS512)".into(),
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

fn default_true() -> bool {
    true
}

fn default_jwks_cache_ttl() -> u64 {
    3600 // 1 hour
}

fn default_session_ttl() -> u64 {
    7 * 24 * 60 * 60 // 7 days
}

/// Substitute environment variables in the format ${VAR_NAME}.
fn substitute_env_vars(content: &str) -> String {
    let mut result = content.to_string();
    let re = regex_lite::Regex::new(r"\$\{([A-Z_][A-Z0-9_]*)\}").unwrap();

    for cap in re.captures_iter(content) {
        let var_name = &cap[1];
        if let Ok(value) = std::env::var(var_name) {
            result = result.replace(&cap[0], &value);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ForgeConfig::default_with_database_url("postgres://localhost/test");
        assert_eq!(config.gateway.port, 8080);
        assert_eq!(config.node.roles.len(), 4);
    }

    #[test]
    fn test_parse_minimal_config() {
        let toml = r#"
            [database]
            url = "postgres://localhost/myapp"
        "#;

        let config = ForgeConfig::parse_toml(toml).unwrap();
        assert_eq!(config.database.url, "postgres://localhost/myapp");
        assert_eq!(config.gateway.port, 8080);
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
        assert_eq!(config.database.url, "postgres://test:test@localhost/test");

        unsafe {
            std::env::remove_var("TEST_DB_URL");
        }
    }
}
