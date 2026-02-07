use std::sync::Arc;

use tokio::sync::{Mutex, watch};
use tracing::Span;
use uuid::Uuid;

use crate::env::{EnvAccess, EnvProvider, RealEnvProvider};

/// Context available to daemon handlers.
pub struct DaemonContext {
    /// Daemon name.
    pub daemon_name: String,
    /// Unique instance ID for this daemon execution.
    pub instance_id: Uuid,
    /// Database pool.
    db_pool: sqlx::PgPool,
    /// HTTP client for external calls.
    http_client: reqwest::Client,
    /// Shutdown signal receiver (wrapped in Mutex for interior mutability).
    shutdown_rx: Mutex<watch::Receiver<bool>>,
    /// Environment variable provider.
    env_provider: Arc<dyn EnvProvider>,
    /// Parent span for trace propagation.
    span: Span,
}

impl DaemonContext {
    /// Create a new daemon context.
    pub fn new(
        daemon_name: String,
        instance_id: Uuid,
        db_pool: sqlx::PgPool,
        http_client: reqwest::Client,
        shutdown_rx: watch::Receiver<bool>,
    ) -> Self {
        Self {
            daemon_name,
            instance_id,
            db_pool,
            http_client,
            shutdown_rx: Mutex::new(shutdown_rx),
            env_provider: Arc::new(RealEnvProvider::new()),
            span: Span::current(),
        }
    }

    /// Set environment provider.
    pub fn with_env_provider(mut self, provider: Arc<dyn EnvProvider>) -> Self {
        self.env_provider = provider;
        self
    }

    pub fn db(&self) -> &sqlx::PgPool {
        &self.db_pool
    }

    pub fn http(&self) -> &reqwest::Client {
        &self.http_client
    }

    /// Check if shutdown has been requested.
    pub fn is_shutdown_requested(&self) -> bool {
        // Use try_lock to avoid blocking; if can't lock, assume not shutdown
        self.shutdown_rx
            .try_lock()
            .map(|rx| *rx.borrow())
            .unwrap_or(false)
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
        // Wait until the value becomes true
        while !*rx.borrow_and_update() {
            if rx.changed().await.is_err() {
                // Channel closed, treat as shutdown
                break;
            }
        }
    }

    /// Send heartbeat to indicate daemon is alive.
    pub async fn heartbeat(&self) -> crate::Result<()> {
        tracing::trace!(daemon.name = %self.daemon_name, "Sending heartbeat");

        sqlx::query(
            r#"
            UPDATE forge_daemons
            SET last_heartbeat = NOW()
            WHERE name = $1 AND instance_id = $2
            "#,
        )
        .bind(&self.daemon_name)
        .bind(self.instance_id)
        .execute(&self.db_pool)
        .await
        .map_err(|e| crate::ForgeError::Database(e.to_string()))?;

        Ok(())
    }

    /// Get the trace ID for this daemon execution.
    ///
    /// Returns the instance_id as a correlation ID.
    pub fn trace_id(&self) -> String {
        self.instance_id.to_string()
    }

    /// Get the parent span for trace propagation.
    ///
    /// Use this to create child spans within daemon handlers.
    pub fn span(&self) -> &Span {
        &self.span
    }
}

impl EnvAccess for DaemonContext {
    fn env_provider(&self) -> &dyn EnvProvider {
        self.env_provider.as_ref()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_daemon_context_creation() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect_lazy("postgres://localhost/nonexistent")
            .expect("Failed to create mock pool");

        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let instance_id = Uuid::new_v4();

        let ctx = DaemonContext::new(
            "test_daemon".to_string(),
            instance_id,
            pool,
            reqwest::Client::new(),
            shutdown_rx,
        );

        assert_eq!(ctx.daemon_name, "test_daemon");
        assert_eq!(ctx.instance_id, instance_id);
        assert!(!ctx.is_shutdown_requested());

        // Signal shutdown
        shutdown_tx.send(true).unwrap();
        assert!(ctx.is_shutdown_requested());
    }

    #[tokio::test]
    async fn test_shutdown_signal() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect_lazy("postgres://localhost/nonexistent")
            .expect("Failed to create mock pool");

        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        let ctx = DaemonContext::new(
            "test_daemon".to_string(),
            Uuid::new_v4(),
            pool,
            reqwest::Client::new(),
            shutdown_rx,
        );

        // Spawn a task to signal shutdown after a delay
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            shutdown_tx.send(true).unwrap();
        });

        // Wait for shutdown signal
        tokio::time::timeout(std::time::Duration::from_millis(200), ctx.shutdown_signal())
            .await
            .expect("Shutdown signal should complete");

        assert!(ctx.is_shutdown_requested());
    }
}
