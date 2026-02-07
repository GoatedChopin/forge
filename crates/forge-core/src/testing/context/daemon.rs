//! Test context for daemon functions.

#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use std::collections::HashMap;
use std::sync::Arc;

use sqlx::PgPool;
use tokio::sync::{Mutex, watch};
use uuid::Uuid;

use super::super::mock_http::{MockHttp, MockRequest, MockResponse};
use crate::Result;
use crate::env::{EnvAccess, EnvProvider, MockEnvProvider};

/// Test context for daemon functions.
///
/// Provides an isolated testing environment for daemons with shutdown signal
/// control, HTTP mocking, and optional database access.
///
/// # Example
///
/// ```ignore
/// let ctx = TestDaemonContext::builder("heartbeat_daemon")
///     .with_pool(db_pool)
///     .build();
///
/// // Test shutdown handling
/// assert!(!ctx.is_shutdown_requested());
/// ctx.request_shutdown();
/// assert!(ctx.is_shutdown_requested());
/// ```
pub struct TestDaemonContext {
    /// Daemon name.
    pub daemon_name: String,
    /// Unique instance ID.
    pub instance_id: Uuid,
    /// Optional database pool.
    pool: Option<PgPool>,
    /// Mock HTTP client.
    http: Arc<MockHttp>,
    /// Shutdown signal sender (for triggering shutdown in tests).
    pub shutdown_tx: watch::Sender<bool>,
    /// Shutdown signal receiver (wrapped in Mutex for interior mutability).
    shutdown_rx: Mutex<watch::Receiver<bool>>,
    /// Mock environment provider.
    env_provider: Arc<MockEnvProvider>,
}

impl TestDaemonContext {
    /// Create a new builder.
    pub fn builder(daemon_name: impl Into<String>) -> TestDaemonContextBuilder {
        TestDaemonContextBuilder::new(daemon_name)
    }

    /// Get the database pool (if available).
    pub fn db(&self) -> Option<&PgPool> {
        self.pool.as_ref()
    }

    /// Get the mock HTTP client.
    pub fn http(&self) -> &MockHttp {
        &self.http
    }

    /// Check if shutdown has been requested.
    pub fn is_shutdown_requested(&self) -> bool {
        self.shutdown_rx
            .try_lock()
            .map(|rx| *rx.borrow())
            .unwrap_or(false)
    }

    /// Request shutdown (trigger from test).
    pub fn request_shutdown(&self) {
        let _ = self.shutdown_tx.send(true);
    }

    /// Wait for shutdown signal.
    ///
    /// Use this in a `tokio::select!` to handle graceful shutdown:
    ///
    /// ```ignore
    /// tokio::select! {
    ///     _ = tokio::time::sleep(Duration::from_secs(60)) => {}
    ///     _ = ctx.shutdown_signal() => break,
    /// }
    /// ```
    pub async fn shutdown_signal(&self) {
        let mut rx = self.shutdown_rx.lock().await;
        while !*rx.borrow_and_update() {
            if rx.changed().await.is_err() {
                break;
            }
        }
    }

    /// Simulate heartbeat (no-op in tests).
    pub async fn heartbeat(&self) -> Result<()> {
        Ok(())
    }

    /// Get the mock env provider for verification.
    pub fn env_mock(&self) -> &MockEnvProvider {
        &self.env_provider
    }
}

impl EnvAccess for TestDaemonContext {
    fn env_provider(&self) -> &dyn EnvProvider {
        self.env_provider.as_ref()
    }
}

/// Builder for TestDaemonContext.
pub struct TestDaemonContextBuilder {
    daemon_name: String,
    instance_id: Option<Uuid>,
    pool: Option<PgPool>,
    http: MockHttp,
    env_vars: HashMap<String, String>,
}

impl TestDaemonContextBuilder {
    /// Create a new builder with daemon name.
    pub fn new(daemon_name: impl Into<String>) -> Self {
        Self {
            daemon_name: daemon_name.into(),
            instance_id: None,
            pool: None,
            http: MockHttp::new(),
            env_vars: HashMap::new(),
        }
    }

    /// Set a specific instance ID.
    pub fn with_instance_id(mut self, id: Uuid) -> Self {
        self.instance_id = Some(id);
        self
    }

    /// Set the database pool.
    pub fn with_pool(mut self, pool: PgPool) -> Self {
        self.pool = Some(pool);
        self
    }

    /// Add an HTTP mock with a custom handler.
    pub fn mock_http<F>(self, pattern: &str, handler: F) -> Self
    where
        F: Fn(&MockRequest) -> MockResponse + Send + Sync + 'static,
    {
        self.http.add_mock_sync(pattern, handler);
        self
    }

    /// Add an HTTP mock that returns a JSON response.
    pub fn mock_http_json<T: serde::Serialize>(self, pattern: &str, response: T) -> Self {
        let json = serde_json::to_value(response).unwrap_or(serde_json::Value::Null);
        self.mock_http(pattern, move |_| MockResponse::json(json.clone()))
    }

    /// Set a single environment variable.
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env_vars.insert(key.into(), value.into());
        self
    }

    /// Set multiple environment variables.
    pub fn with_envs(mut self, vars: HashMap<String, String>) -> Self {
        self.env_vars.extend(vars);
        self
    }

    /// Build the test context.
    pub fn build(self) -> TestDaemonContext {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        TestDaemonContext {
            daemon_name: self.daemon_name,
            instance_id: self.instance_id.unwrap_or_else(Uuid::new_v4),
            pool: self.pool,
            http: Arc::new(self.http),
            shutdown_tx,
            shutdown_rx: Mutex::new(shutdown_rx),
            env_provider: Arc::new(MockEnvProvider::with_vars(self.env_vars)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_daemon_context_creation() {
        let ctx = TestDaemonContext::builder("heartbeat_daemon").build();

        assert_eq!(ctx.daemon_name, "heartbeat_daemon");
        assert!(!ctx.is_shutdown_requested());
    }

    #[test]
    fn test_shutdown_request() {
        let ctx = TestDaemonContext::builder("test").build();

        assert!(!ctx.is_shutdown_requested());
        ctx.request_shutdown();
        assert!(ctx.is_shutdown_requested());
    }

    #[tokio::test]
    async fn test_shutdown_signal() {
        let ctx = TestDaemonContext::builder("test").build();

        // Spawn a task to request shutdown after a delay
        let shutdown_tx = ctx.shutdown_tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            let _ = shutdown_tx.send(true);
        });

        // Wait for shutdown signal
        tokio::time::timeout(std::time::Duration::from_millis(200), ctx.shutdown_signal())
            .await
            .expect("Shutdown signal should complete");

        assert!(ctx.is_shutdown_requested());
    }

    #[test]
    fn test_with_instance_id() {
        let id = Uuid::new_v4();
        let ctx = TestDaemonContext::builder("test")
            .with_instance_id(id)
            .build();

        assert_eq!(ctx.instance_id, id);
    }
}
