use serde::{Deserialize, Serialize};

fn parse_duration_secs(s: &str, default_secs: u64) -> u64 {
    crate::util::parse_duration(s)
        .map(|d| d.as_secs())
        .unwrap_or(default_secs)
}

/// Cluster configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterConfig {
    /// Cluster name.
    #[serde(default = "default_cluster_name")]
    pub name: String,

    /// Discovery method.
    #[serde(default)]
    pub discovery: DiscoveryMethod,

    /// Heartbeat interval duration (e.g. "5s", "10s").
    #[serde(default = "default_heartbeat_interval")]
    pub heartbeat_interval: String,

    /// Threshold duration for marking nodes as dead (e.g. "15s", "30s").
    #[serde(default = "default_dead_threshold")]
    pub dead_threshold: String,

    /// Static seed nodes (for static discovery).
    #[serde(default)]
    pub seed_nodes: Vec<String>,

    /// DNS name for discovery (for DNS discovery).
    pub dns_name: Option<String>,
}

impl Default for ClusterConfig {
    fn default() -> Self {
        Self {
            name: default_cluster_name(),
            discovery: DiscoveryMethod::default(),
            heartbeat_interval: default_heartbeat_interval(),
            dead_threshold: default_dead_threshold(),
            seed_nodes: Vec::new(),
            dns_name: None,
        }
    }
}

impl ClusterConfig {
    /// Heartbeat interval in seconds, parsed from the `heartbeat_interval` string.
    pub fn heartbeat_interval_secs(&self) -> u64 {
        parse_duration_secs(&self.heartbeat_interval, 5)
    }

    /// Dead threshold in seconds, parsed from the `dead_threshold` string.
    pub fn dead_threshold_secs(&self) -> u64 {
        parse_duration_secs(&self.dead_threshold, 15)
    }
}

fn default_cluster_name() -> String {
    "default".to_string()
}

fn default_heartbeat_interval() -> String {
    "5s".to_string()
}

fn default_dead_threshold() -> String {
    "15s".to_string()
}

/// Cluster discovery method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum DiscoveryMethod {
    /// Use PostgreSQL table for discovery.
    #[default]
    Postgres,

    /// Use DNS for discovery.
    Dns,

    /// Use Kubernetes for discovery.
    Kubernetes,

    /// Use static seed nodes.
    Static,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn test_default_cluster_config() {
        let config = ClusterConfig::default();
        assert_eq!(config.name, "default");
        assert_eq!(config.discovery, DiscoveryMethod::Postgres);
        assert_eq!(config.heartbeat_interval_secs(), 5);
    }

    #[test]
    fn test_parse_cluster_config() {
        let toml = r#"
            name = "production"
            discovery = "kubernetes"
            heartbeat_interval = "10s"
        "#;

        let config: ClusterConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.name, "production");
        assert_eq!(config.discovery, DiscoveryMethod::Kubernetes);
    }
}
