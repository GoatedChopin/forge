use serde::{Deserialize, Serialize};

use crate::error::{ForgeError, Result};

/// Database source configuration.
/// This enum makes invalid states unrepresentable: you either use embedded
/// PostgreSQL or connect to a remote server, never both or neither.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "mode", rename_all = "lowercase")]
pub enum DatabaseSource {
    /// Connect to an external PostgreSQL instance.
    Remote {
        /// PostgreSQL connection URL.
        url: String,
    },
    /// Use embedded PostgreSQL (zero external dependencies).
    /// Starts a bundled PostgreSQL instance automatically.
    /// Requires the `embedded-db` feature.
    Embedded {
        /// Data directory for embedded PostgreSQL.
        /// Defaults to `.forge/postgres` in the current directory.
        #[serde(default)]
        data_dir: Option<String>,
    },
}

impl Default for DatabaseSource {
    fn default() -> Self {
        Self::Remote { url: String::new() }
    }
}

/// Database configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// Database source: remote URL or embedded.
    #[serde(flatten)]
    pub source: DatabaseSource,

    /// Connection pool size.
    #[serde(default = "default_pool_size")]
    pub pool_size: u32,

    /// Pool checkout timeout in seconds.
    #[serde(default = "default_pool_timeout")]
    pub pool_timeout_secs: u64,

    /// Statement timeout in seconds.
    #[serde(default = "default_statement_timeout")]
    pub statement_timeout_secs: u64,

    /// Read replica URLs for scaling reads.
    #[serde(default)]
    pub replica_urls: Vec<String>,

    /// Whether to route read queries to replicas.
    #[serde(default)]
    pub read_from_replica: bool,

    /// Connection pool isolation configuration.
    #[serde(default)]
    pub pools: PoolsConfig,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            source: DatabaseSource::default(),
            pool_size: default_pool_size(),
            pool_timeout_secs: default_pool_timeout(),
            statement_timeout_secs: default_statement_timeout(),
            replica_urls: Vec::new(),
            read_from_replica: false,
            pools: PoolsConfig::default(),
        }
    }
}

impl DatabaseConfig {
    /// Create a config for a remote database.
    pub fn remote(url: impl Into<String>) -> Self {
        Self {
            source: DatabaseSource::Remote { url: url.into() },
            ..Default::default()
        }
    }

    /// Create a config for embedded PostgreSQL.
    pub fn embedded() -> Self {
        Self {
            source: DatabaseSource::Embedded { data_dir: None },
            ..Default::default()
        }
    }

    /// Create a config for embedded PostgreSQL with a custom data directory.
    pub fn embedded_with_data_dir(data_dir: impl Into<String>) -> Self {
        Self {
            source: DatabaseSource::Embedded {
                data_dir: Some(data_dir.into()),
            },
            ..Default::default()
        }
    }

    /// Check if this config uses embedded PostgreSQL.
    pub fn is_embedded(&self) -> bool {
        matches!(self.source, DatabaseSource::Embedded { .. })
    }

    /// Get the remote URL if configured.
    pub fn url(&self) -> Option<&str> {
        match &self.source {
            DatabaseSource::Remote { url } => Some(url),
            DatabaseSource::Embedded { .. } => None,
        }
    }

    /// Get the data directory for embedded mode.
    pub fn data_dir(&self) -> Option<&str> {
        match &self.source {
            DatabaseSource::Remote { .. } => None,
            DatabaseSource::Embedded { data_dir } => data_dir.as_deref(),
        }
    }

    /// Validate the database configuration.
    pub fn validate(&self) -> Result<()> {
        if let DatabaseSource::Remote { url } = &self.source
            && url.is_empty()
        {
            return Err(ForgeError::Config(
                "database.url is required when mode = \"remote\". \
                 Set database.url to a PostgreSQL connection string \
                 (e.g., \"postgres://user:pass@localhost/mydb\"), \
                 or use mode = \"embedded\" for zero-dependency development."
                    .into(),
            ));
        }
        Ok(())
    }
}

fn default_pool_size() -> u32 {
    50
}

fn default_pool_timeout() -> u64 {
    30
}

fn default_statement_timeout() -> u64 {
    30
}

/// Pool isolation configuration for different workloads.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PoolsConfig {
    /// Default pool for queries/mutations.
    #[serde(default)]
    pub default: Option<PoolConfig>,

    /// Pool for background jobs.
    #[serde(default)]
    pub jobs: Option<PoolConfig>,

    /// Pool for observability writes.
    #[serde(default)]
    pub observability: Option<PoolConfig>,

    /// Pool for long-running analytics.
    #[serde(default)]
    pub analytics: Option<PoolConfig>,
}

/// Individual pool configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolConfig {
    /// Pool size.
    pub size: u32,

    /// Checkout timeout in seconds.
    #[serde(default = "default_pool_timeout")]
    pub timeout_secs: u64,

    /// Statement timeout in seconds (optional override).
    pub statement_timeout_secs: Option<u64>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn test_default_database_config() {
        let config = DatabaseConfig::default();
        assert_eq!(config.pool_size, 50);
        assert_eq!(config.pool_timeout_secs, 30);
        assert!(!config.is_embedded());
    }

    #[test]
    fn test_remote_config() {
        let config = DatabaseConfig::remote("postgres://localhost/test");
        assert_eq!(config.url(), Some("postgres://localhost/test"));
        assert!(!config.is_embedded());
        assert!(config.data_dir().is_none());
    }

    #[test]
    fn test_embedded_config() {
        let config = DatabaseConfig::embedded();
        assert!(config.is_embedded());
        assert!(config.url().is_none());
        assert!(config.data_dir().is_none());
    }

    #[test]
    fn test_embedded_with_data_dir() {
        let config = DatabaseConfig::embedded_with_data_dir("/var/forge/data");
        assert!(config.is_embedded());
        assert_eq!(config.data_dir(), Some("/var/forge/data"));
    }

    #[test]
    fn test_parse_remote_config() {
        let toml = r#"
            mode = "remote"
            url = "postgres://localhost/test"
            pool_size = 100
            replica_urls = ["postgres://replica1/test", "postgres://replica2/test"]
            read_from_replica = true
        "#;

        let config: DatabaseConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.pool_size, 100);
        assert_eq!(config.url(), Some("postgres://localhost/test"));
        assert_eq!(config.replica_urls.len(), 2);
        assert!(config.read_from_replica);
    }

    #[test]
    fn test_parse_embedded_config() {
        let toml = r#"
            mode = "embedded"
            data_dir = ".forge/data"
            pool_size = 20
        "#;

        let config: DatabaseConfig = toml::from_str(toml).unwrap();
        assert!(config.is_embedded());
        assert_eq!(config.data_dir(), Some(".forge/data"));
        assert_eq!(config.pool_size, 20);
    }

    #[test]
    fn test_parse_embedded_no_data_dir() {
        let toml = r#"
            mode = "embedded"
        "#;

        let config: DatabaseConfig = toml::from_str(toml).unwrap();
        assert!(config.is_embedded());
        assert!(config.data_dir().is_none());
    }

    #[test]
    fn test_serialize_remote() {
        let config = DatabaseConfig::remote("postgres://localhost/test");
        let toml_str = toml::to_string(&config).unwrap();
        assert!(toml_str.contains("mode = \"remote\""));
        assert!(toml_str.contains("url = \"postgres://localhost/test\""));
    }

    #[test]
    fn test_serialize_embedded() {
        let config = DatabaseConfig::embedded_with_data_dir(".forge/data");
        let toml_str = toml::to_string(&config).unwrap();
        assert!(toml_str.contains("mode = \"embedded\""));
        assert!(toml_str.contains("data_dir = \".forge/data\""));
    }

    #[test]
    fn test_validate_remote_with_url() {
        let config = DatabaseConfig::remote("postgres://localhost/test");
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_remote_empty_url() {
        let config = DatabaseConfig::default();
        let result = config.validate();
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("database.url is required"));
    }

    #[test]
    fn test_validate_embedded() {
        let config = DatabaseConfig::embedded();
        assert!(config.validate().is_ok());
    }
}
