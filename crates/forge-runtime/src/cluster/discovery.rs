//! Cluster discovery implementations.
//!
//! Supports multiple discovery methods for finding peer nodes:
//! - **Postgres**: Default. Nodes register in the `forge_nodes` table.
//! - **DNS**: Resolve a DNS name to find peer node IPs.
//! - **Kubernetes**: Use the Kubernetes API to discover pods in a headless service.
//! - **Static**: Use a fixed list of seed node addresses.

use std::net::{IpAddr, SocketAddr, ToSocketAddrs};

use forge_core::config::cluster::{ClusterConfig, DiscoveryMethod};
use forge_core::{ForgeError, Result};

/// Discovered peer node address.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PeerAddress {
    /// IP address of the peer.
    pub ip: IpAddr,
    /// HTTP port of the peer (defaults to gateway port).
    pub port: u16,
}

impl std::fmt::Display for PeerAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.ip, self.port)
    }
}

/// Discover peer nodes using the configured discovery method.
///
/// For `Postgres` discovery, peers are found via the `forge_nodes` table
/// (handled by `NodeRegistry`), so this function returns an empty list.
/// For other methods, this resolves peers from the configured source.
pub async fn discover_peers(config: &ClusterConfig, default_port: u16) -> Result<Vec<PeerAddress>> {
    match config.discovery {
        DiscoveryMethod::Postgres => {
            // Postgres discovery is handled by NodeRegistry directly.
            Ok(Vec::new())
        }
        DiscoveryMethod::Dns => discover_dns(config, default_port).await,
        DiscoveryMethod::Kubernetes => discover_kubernetes(config, default_port).await,
        DiscoveryMethod::Static => discover_static(config, default_port),
    }
}

/// DNS-based discovery.
///
/// Resolves the configured `dns_name` to a set of IP addresses. This is
/// commonly used with Kubernetes headless services, where the DNS name
/// resolves to all pod IPs in the service.
async fn discover_dns(config: &ClusterConfig, default_port: u16) -> Result<Vec<PeerAddress>> {
    let dns_name = config.dns_name.as_deref().ok_or_else(|| {
        ForgeError::Config(
            "DNS discovery requires 'dns_name' to be set in [cluster] config".to_string(),
        )
    })?;

    let lookup_name = if dns_name.contains(':') {
        dns_name.to_string()
    } else {
        format!("{}:{}", dns_name, default_port)
    };

    // Perform DNS resolution (blocking, but fast for local DNS)
    let addrs: Vec<SocketAddr> = tokio::task::spawn_blocking(move || {
        lookup_name
            .to_socket_addrs()
            .map(|iter| iter.collect::<Vec<_>>())
    })
    .await
    .map_err(|e| ForgeError::Cluster(format!("DNS lookup task failed: {}", e)))?
    .map_err(|e| ForgeError::Cluster(format!("DNS resolution failed for '{}': {}", dns_name, e)))?;

    let peers: Vec<PeerAddress> = addrs
        .into_iter()
        .map(|addr| PeerAddress {
            ip: addr.ip(),
            port: addr.port(),
        })
        .collect();

    tracing::debug!(
        dns_name,
        peer_count = peers.len(),
        "DNS discovery completed"
    );
    Ok(peers)
}

/// Kubernetes-based discovery.
///
/// Uses the Kubernetes downward API and DNS to discover peer pods.
/// This expects the service to be a headless service (ClusterIP: None)
/// so that DNS returns individual pod IPs.
///
/// Falls back to DNS discovery using the `dns_name` config, which should
/// be set to the headless service FQDN (e.g., `my-app.default.svc.cluster.local`).
async fn discover_kubernetes(
    config: &ClusterConfig,
    default_port: u16,
) -> Result<Vec<PeerAddress>> {
    // Kubernetes discovery uses DNS under the hood via headless services.
    // The dns_name should point to the headless service FQDN.
    if config.dns_name.is_some() {
        return discover_dns(config, default_port).await;
    }

    // Attempt to construct the service DNS name from environment variables
    // set by the Kubernetes downward API.
    let namespace = std::env::var("POD_NAMESPACE")
        .or_else(|_| std::env::var("KUBERNETES_NAMESPACE"))
        .unwrap_or_else(|_| "default".to_string());

    let service_name = std::env::var("SERVICE_NAME").or_else(|_| {
        std::env::var("HOSTNAME").map(|h| {
            // Extract service name from pod hostname (e.g., "my-app-0" -> "my-app")
            h.rsplit_once('-')
                .map(|(prefix, _)| prefix.to_string())
                .unwrap_or(h)
        })
    });

    match service_name {
        Ok(svc) => {
            let dns_name = format!("{}.{}.svc.cluster.local", svc, namespace);
            tracing::info!(
                dns_name = %dns_name,
                "Kubernetes discovery: constructed service DNS from environment"
            );

            let k8s_config = ClusterConfig {
                dns_name: Some(dns_name),
                ..config.clone()
            };
            discover_dns(&k8s_config, default_port).await
        }
        Err(_) => Err(ForgeError::Config(
            "Kubernetes discovery requires either 'dns_name' in [cluster] config, \
             or SERVICE_NAME/HOSTNAME and POD_NAMESPACE environment variables"
                .to_string(),
        )),
    }
}

/// Static discovery from configured seed nodes.
///
/// Parses the `seed_nodes` list from config. Each entry should be
/// in the format `host:port` or just `host` (uses default port).
fn discover_static(config: &ClusterConfig, default_port: u16) -> Result<Vec<PeerAddress>> {
    if config.seed_nodes.is_empty() {
        return Err(ForgeError::Config(
            "Static discovery requires 'seed_nodes' to be set in [cluster] config".to_string(),
        ));
    }

    let mut peers = Vec::with_capacity(config.seed_nodes.len());

    for node in &config.seed_nodes {
        let (host, port) = if let Some((h, p)) = node.rsplit_once(':') {
            let port = p.parse::<u16>().map_err(|_| {
                ForgeError::Config(format!("Invalid port in seed node '{}': '{}'", node, p))
            })?;
            (h, port)
        } else {
            (node.as_str(), default_port)
        };

        let ip: IpAddr = host.parse().map_err(|e| {
            ForgeError::Config(format!("Invalid IP address in seed node '{}': {}", node, e))
        })?;

        peers.push(PeerAddress { ip, port });
    }

    tracing::debug!(
        seed_count = peers.len(),
        "Static discovery loaded seed nodes"
    );
    Ok(peers)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_static_discovery_parses_addresses() {
        let config = ClusterConfig {
            seed_nodes: vec![
                "10.0.0.1:9081".to_string(),
                "10.0.0.2:9081".to_string(),
                "10.0.0.3".to_string(),
            ],
            ..Default::default()
        };

        let peers = discover_static(&config, 9081).unwrap();
        assert_eq!(peers.len(), 3);
        let first = peers.first().expect("expected at least one peer");
        assert_eq!(first.ip, "10.0.0.1".parse::<IpAddr>().unwrap());
        assert_eq!(first.port, 9081);
        let third = peers.get(2).expect("expected three peers");
        assert_eq!(third.port, 9081); // default port used
    }

    #[test]
    fn test_static_discovery_empty_seed_nodes_errors() {
        let config = ClusterConfig::default();
        let result = discover_static(&config, 9081);
        assert!(result.is_err());
    }

    #[test]
    fn test_peer_address_display() {
        let peer = PeerAddress {
            ip: "10.0.0.1".parse().unwrap(),
            port: 9081,
        };
        assert_eq!(peer.to_string(), "10.0.0.1:9081");
    }
}
