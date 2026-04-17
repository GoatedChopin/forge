//! TLS configuration and listener for the gateway.
//!
//! Two cert sources are supported:
//!
//! - [`TlsListenConfig::SelfSigned`]: generate an ephemeral self-signed
//!   certificate at startup via `rcgen`. Intended for zero-trust deployments
//!   behind a load balancer that terminates public TLS.
//! - [`TlsListenConfig::FromFiles`]: load a PEM-encoded certificate chain and
//!   private key from disk.
//!
//! [`bind`] returns a [`TlsListener`] that implements [`axum::serve::Listener`],
//! so the gateway's single `axum::serve(listener, service).await` hotpath
//! handles both HTTP and HTTPS — TLS is a transport-layer concern layered on
//! top of a plain [`TcpListener`], not a separate server runtime.

use std::fs::File;
use std::io::BufReader;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::Once;

use forge_core::error::{ForgeError, Result};
use rustls::ServerConfig;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsAcceptor;
use tokio_rustls::server::TlsStream;

/// Resolved TLS source for the gateway listener.
#[derive(Debug, Clone)]
pub enum TlsListenConfig {
    /// Generate a self-signed certificate at startup. Intended for zero-trust
    /// deployments behind a load balancer that terminates public TLS.
    SelfSigned {
        /// Subject Alternative Names to include in the generated certificate.
        hostnames: Vec<String>,
    },
    /// Load a certificate chain and private key from disk.
    FromFiles {
        /// Path to the PEM-encoded certificate chain.
        cert_path: String,
        /// Path to the PEM-encoded private key.
        key_path: String,
    },
}

static CRYPTO_PROVIDER_INIT: Once = Once::new();

/// Install the `ring` default crypto provider for rustls, exactly once.
///
/// `rustls` 0.23+ requires an explicit default provider; calling this before
/// building a `ServerConfig` ensures cipher suites are wired up. Safe to call
/// repeatedly (idempotent via [`Once`]).
fn install_default_crypto_provider() {
    CRYPTO_PROVIDER_INIT.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

/// Build a [`rustls::ServerConfig`] from a [`TlsListenConfig`].
///
/// Returns a [`ForgeError::Config`] for any I/O or parse failure. Failures
/// are surfaced at server startup so operators see them immediately, not at
/// the first HTTPS request.
pub fn load_rustls_config(cfg: &TlsListenConfig) -> Result<Arc<ServerConfig>> {
    install_default_crypto_provider();

    let server_config = match cfg {
        TlsListenConfig::SelfSigned { hostnames } => build_self_signed(hostnames)?,
        TlsListenConfig::FromFiles {
            cert_path,
            key_path,
        } => build_from_files(cert_path, key_path)?,
    };

    Ok(Arc::new(server_config))
}

/// A TCP listener that terminates TLS before handing streams to `axum::serve`.
///
/// Handshakes run serially in [`accept`](axum::serve::Listener::accept) — a
/// slow handshake delays the next accept. That's fine for deployments behind a
/// load balancer (long-lived connections, handshake frequency is low) and for
/// dev. If concurrent handshakes become necessary, move the handshake into a
/// spawned task feeding an `mpsc` channel and pull from the channel here.
pub struct TlsListener {
    tcp: TcpListener,
    acceptor: TlsAcceptor,
}

impl axum::serve::Listener for TlsListener {
    type Io = TlsStream<TcpStream>;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            let (stream, peer) = match self.tcp.accept().await {
                Ok(pair) => pair,
                Err(err) => {
                    // Mirror axum's own TcpListener behavior: log and keep
                    // looping rather than killing the serve future.
                    tracing::warn!(error = %err, "TCP accept error");
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    continue;
                }
            };
            match self.acceptor.accept(stream).await {
                Ok(tls) => return (tls, peer),
                Err(err) => {
                    tracing::warn!(peer = %peer, error = %err, "TLS handshake failed");
                }
            }
        }
    }

    fn local_addr(&self) -> std::io::Result<Self::Addr> {
        self.tcp.local_addr()
    }
}

/// Bind a [`TlsListener`] on `addr` using `cfg`.
///
/// Emits a per-mode `tracing` line so operators see the cert source at startup.
/// Config errors (I/O, parse, invalid key pair) are mapped to
/// [`std::io::Error`] so the gateway's serve path can surface them uniformly
/// alongside bind failures.
pub async fn bind(addr: SocketAddr, cfg: &TlsListenConfig) -> std::io::Result<TlsListener> {
    let rustls_config = load_rustls_config(cfg).map_err(std::io::Error::other)?;
    match cfg {
        TlsListenConfig::SelfSigned { hostnames } => {
            tracing::warn!(
                addr = %addr,
                hostnames = ?hostnames,
                "Gateway listening with ephemeral self-signed TLS certificate. \
                 Not suitable for public-facing use; terminate public TLS at a \
                 load balancer."
            );
        }
        TlsListenConfig::FromFiles {
            cert_path,
            key_path,
        } => {
            tracing::info!(
                addr = %addr,
                cert_path = %cert_path,
                key_path = %key_path,
                "Gateway listening with TLS (file-based certificate)"
            );
        }
    }
    let tcp = TcpListener::bind(addr).await?;
    Ok(TlsListener {
        tcp,
        acceptor: TlsAcceptor::from(rustls_config),
    })
}

fn build_self_signed(hostnames: &[String]) -> Result<ServerConfig> {
    let cert = rcgen::generate_simple_self_signed(hostnames.to_vec()).map_err(|e| {
        ForgeError::Config(format!("failed to generate self-signed certificate: {e}"))
    })?;

    let cert_der = CertificateDer::from(cert.cert.der().to_vec());
    let key_der = PrivateKeyDer::Pkcs8(cert.key_pair.serialize_der().into());

    ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert_der], key_der)
        .map_err(|e| ForgeError::Config(format!("invalid generated self-signed certificate: {e}")))
}

fn build_from_files(cert_path: &str, key_path: &str) -> Result<ServerConfig> {
    let cert_chain = read_pem_certs(cert_path)?;
    let key = read_pem_key(key_path)?;

    ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(cert_chain, key)
        .map_err(|e| ForgeError::Config(format!("invalid TLS certificate or key: {e}")))
}

fn read_pem_certs(path: &str) -> Result<Vec<CertificateDer<'static>>> {
    let file = File::open(path).map_err(|e| {
        ForgeError::Config(format!(
            "failed to open gateway.tls.cert_path '{path}': {e}"
        ))
    })?;
    let mut reader = BufReader::new(file);

    let certs: std::result::Result<Vec<_>, _> = rustls_pemfile::certs(&mut reader).collect();
    let certs = certs.map_err(|e| {
        ForgeError::Config(format!("failed to parse PEM certificates in '{path}': {e}"))
    })?;

    if certs.is_empty() {
        return Err(ForgeError::Config(format!(
            "no PEM certificates found in '{path}'"
        )));
    }

    Ok(certs)
}

fn read_pem_key(path: &str) -> Result<PrivateKeyDer<'static>> {
    let file = File::open(path).map_err(|e| {
        ForgeError::Config(format!("failed to open gateway.tls.key_path '{path}': {e}"))
    })?;
    let mut reader = BufReader::new(file);

    rustls_pemfile::private_key(&mut reader)
        .map_err(|e| {
            ForgeError::Config(format!("failed to parse PEM private key in '{path}': {e}"))
        })?
        .ok_or_else(|| ForgeError::Config(format!("no PEM private key found in '{path}'")))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn self_signed_builds_valid_server_config() {
        let cfg = TlsListenConfig::SelfSigned {
            hostnames: vec!["localhost".to_string(), "app.internal".to_string()],
        };
        let server_config = load_rustls_config(&cfg).expect("should build");
        // We can't compare ServerConfig directly, but reaching this point
        // means rustls accepted the cert/key pair.
        assert!(Arc::strong_count(&server_config) >= 1);
    }

    #[tokio::test]
    async fn from_files_loads_rcgen_generated_cert() {
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
            .expect("rcgen should succeed");

        let mut cert_file = NamedTempFile::new().unwrap();
        cert_file.write_all(cert.cert.pem().as_bytes()).unwrap();

        let mut key_file = NamedTempFile::new().unwrap();
        key_file
            .write_all(cert.key_pair.serialize_pem().as_bytes())
            .unwrap();

        let cfg = TlsListenConfig::FromFiles {
            cert_path: cert_file.path().to_string_lossy().into_owned(),
            key_path: key_file.path().to_string_lossy().into_owned(),
        };

        load_rustls_config(&cfg).expect("should load from files");
    }

    #[tokio::test]
    async fn from_files_missing_cert_path_errors() {
        let cfg = TlsListenConfig::FromFiles {
            cert_path: "/nonexistent/cert.pem".to_string(),
            key_path: "/nonexistent/key.pem".to_string(),
        };
        let err = load_rustls_config(&cfg).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("failed to open gateway.tls.cert_path"),
            "unexpected error: {msg}"
        );
    }

    #[tokio::test]
    async fn from_files_malformed_cert_errors() {
        let mut cert_file = NamedTempFile::new().unwrap();
        cert_file.write_all(b"not a certificate").unwrap();

        let mut key_file = NamedTempFile::new().unwrap();
        key_file.write_all(b"not a key").unwrap();

        let cfg = TlsListenConfig::FromFiles {
            cert_path: cert_file.path().to_string_lossy().into_owned(),
            key_path: key_file.path().to_string_lossy().into_owned(),
        };
        let err = load_rustls_config(&cfg).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("no PEM certificates found"),
            "unexpected error: {msg}"
        );
    }
}
