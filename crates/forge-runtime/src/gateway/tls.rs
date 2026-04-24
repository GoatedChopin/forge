//! TLS configuration and listener for the gateway.
//!
//! PEM-encoded certificate and key are loaded from disk at startup.
//! [`bind`] returns a [`tls_listener::TlsListener`] whose `axum` feature
//! implements [`axum::serve::Listener`], so the gateway's single
//! `axum::serve(listener, service).await` hotpath handles both HTTP and HTTPS.

use std::fs::File;
use std::io::BufReader;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::Once;

use forge_core::error::{ForgeError, Result};
use rustls::ServerConfig;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

/// Resolved TLS source for the gateway listener.
#[derive(Debug, Clone)]
pub struct TlsListenConfig {
    /// Path to the PEM-encoded certificate chain.
    pub cert_path: String,
    /// Path to the PEM-encoded private key.
    pub key_path: String,
}

/// The listener type produced by [`bind`]. A re-export so callers don't need
/// to depend on `tls-listener` directly.
pub type TlsListener = tls_listener::TlsListener<TcpListener, TlsAcceptor>;

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
    let server_config = build_from_files(&cfg.cert_path, &cfg.key_path)?;
    Ok(Arc::new(server_config))
}

/// Bind a [`TlsListener`] on `addr` using `cfg`.
///
/// Config errors (I/O, parse, invalid key pair) are mapped to
/// [`std::io::Error`] so the gateway's serve path can surface them uniformly
/// alongside bind failures.
pub async fn bind(addr: SocketAddr, cfg: &TlsListenConfig) -> std::io::Result<TlsListener> {
    let rustls_config = load_rustls_config(cfg).map_err(std::io::Error::other)?;
    tracing::info!(
        addr = %addr,
        cert_path = %cfg.cert_path,
        key_path = %cfg.key_path,
        "Gateway listening with TLS"
    );
    let tcp = TcpListener::bind(addr).await?;
    Ok(tls_listener::builder(TlsAcceptor::from(rustls_config)).listen(tcp))
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

    // The happy path (a valid PEM pair round-tripping through rustls) is
    // covered by the end-to-end TLS validation suite. The unit tests focus
    // on error surfaces that don't require a real cert fixture.

    #[tokio::test]
    async fn from_files_missing_cert_path_errors() {
        let cfg = TlsListenConfig {
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

        let cfg = TlsListenConfig {
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
