//! Daemon runner with restart logic and leader election.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use forge_core::daemon::{DaemonContext, DaemonStatus};
use futures_util::FutureExt;
use forge_core::Result;
use sqlx::PgPool;
use tokio::sync::{broadcast, watch};
use tracing::{error, info, warn};
use uuid::Uuid;

use super::registry::DaemonRegistry;

/// Configuration for the daemon runner.
#[derive(Debug, Clone)]
pub struct DaemonRunnerConfig {
    /// How often to check daemon health.
    pub health_check_interval: Duration,
    /// How often to send heartbeats.
    pub heartbeat_interval: Duration,
}

impl Default for DaemonRunnerConfig {
    fn default() -> Self {
        Self {
            health_check_interval: Duration::from_secs(30),
            heartbeat_interval: Duration::from_secs(10),
        }
    }
}

/// Manages running all registered daemons.
pub struct DaemonRunner {
    registry: Arc<DaemonRegistry>,
    pool: PgPool,
    http_client: reqwest::Client,
    node_id: Uuid,
    config: DaemonRunnerConfig,
    shutdown_rx: broadcast::Receiver<()>,
}

impl DaemonRunner {
    /// Create a new daemon runner.
    pub fn new(
        registry: Arc<DaemonRegistry>,
        pool: PgPool,
        http_client: reqwest::Client,
        node_id: Uuid,
        shutdown_rx: broadcast::Receiver<()>,
    ) -> Self {
        Self {
            registry,
            pool,
            http_client,
            node_id,
            config: DaemonRunnerConfig::default(),
            shutdown_rx,
        }
    }

    /// Set custom configuration.
    pub fn with_config(mut self, config: DaemonRunnerConfig) -> Self {
        self.config = config;
        self
    }

    /// Run all registered daemons.
    pub async fn run(mut self) -> Result<()> {
        if self.registry.is_empty() {
            info!("No daemons registered, daemon runner idle");
            // Wait for shutdown
            let _ = self.shutdown_rx.recv().await;
            return Ok(());
        }

        info!(
            "Starting daemon runner with {} daemons",
            self.registry.len()
        );

        // Create individual shutdown channels for each daemon
        let mut daemon_handles: HashMap<String, DaemonHandle> = HashMap::new();

        // Start each daemon
        for (name, entry) in self.registry.daemons() {
            let info = &entry.info;

            // Create shutdown channel for this daemon
            let (shutdown_tx, shutdown_rx) = watch::channel(false);

            let handle = DaemonHandle {
                name: name.to_string(),
                instance_id: Uuid::new_v4(),
                shutdown_tx,
                restarts: 0,
                status: DaemonStatus::Pending,
            };

            // Record daemon in database
            if let Err(e) = self.record_daemon_start(&handle).await {
                error!(daemon = name, error = %e, "Failed to record daemon start");
            }

            // Spawn daemon task
            let daemon_entry = entry.clone();
            let pool = self.pool.clone();
            let http_client = self.http_client.clone();
            let daemon_name = name.to_string();
            let startup_delay = info.startup_delay;
            let restart_on_panic = info.restart_on_panic;
            let restart_delay = info.restart_delay;
            let max_restarts = info.max_restarts;
            let leader_elected = info.leader_elected;
            let node_id = self.node_id;

            tokio::spawn(async move {
                run_daemon_loop(
                    daemon_name,
                    daemon_entry,
                    pool,
                    http_client,
                    shutdown_rx,
                    node_id,
                    startup_delay,
                    restart_on_panic,
                    restart_delay,
                    max_restarts,
                    leader_elected,
                )
                .await
            });

            daemon_handles.insert(name.to_string(), handle);
        }

        // Wait for shutdown signal
        let _ = self.shutdown_rx.recv().await;
        info!("Daemon runner received shutdown signal");

        // Signal all daemons to stop
        for (name, handle) in &daemon_handles {
            info!(daemon = name, "Signaling daemon to stop");
            let _ = handle.shutdown_tx.send(true);
        }

        // Give daemons time to clean up
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Update daemon status in database
        for (name, handle) in &daemon_handles {
            if let Err(e) = self.record_daemon_stop(handle).await {
                warn!(daemon = name, error = %e, "Failed to record daemon stop");
            }
        }

        Ok(())
    }

    async fn record_daemon_start(&self, handle: &DaemonHandle) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO forge_daemons (name, node_id, instance_id, status, restarts, started_at, last_heartbeat)
            VALUES ($1, $2, $3, $4, $5, NOW(), NOW())
            ON CONFLICT (name) DO UPDATE SET
                node_id = EXCLUDED.node_id,
                instance_id = EXCLUDED.instance_id,
                status = EXCLUDED.status,
                restarts = EXCLUDED.restarts,
                started_at = NOW(),
                last_heartbeat = NOW(),
                last_error = NULL
            "#,
        )
        .bind(&handle.name)
        .bind(self.node_id)
        .bind(handle.instance_id)
        .bind(handle.status.as_str())
        .bind(handle.restarts as i32)
        .execute(&self.pool)
        .await
        .map_err(|e| forge_core::ForgeError::Database(e.to_string()))?;

        Ok(())
    }

    async fn record_daemon_stop(&self, handle: &DaemonHandle) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE forge_daemons
            SET status = 'stopped', last_heartbeat = NOW()
            WHERE name = $1 AND instance_id = $2
            "#,
        )
        .bind(&handle.name)
        .bind(handle.instance_id)
        .execute(&self.pool)
        .await
        .map_err(|e| forge_core::ForgeError::Database(e.to_string()))?;

        Ok(())
    }
}

struct DaemonHandle {
    name: String,
    instance_id: Uuid,
    shutdown_tx: watch::Sender<bool>,
    restarts: u32,
    status: DaemonStatus,
}

#[allow(clippy::too_many_arguments)]
async fn run_daemon_loop(
    name: String,
    entry: Arc<super::registry::DaemonEntry>,
    pool: PgPool,
    http_client: reqwest::Client,
    mut shutdown_rx: watch::Receiver<bool>,
    node_id: Uuid,
    startup_delay: Duration,
    restart_on_panic: bool,
    restart_delay: Duration,
    max_restarts: Option<u32>,
    leader_elected: bool,
) {
    let mut restarts = 0u32;

    // Apply startup delay
    if !startup_delay.is_zero() {
        info!(daemon = %name, delay = ?startup_delay, "Waiting startup delay");
        tokio::select! {
            _ = tokio::time::sleep(startup_delay) => {}
            _ = shutdown_rx.changed() => {
                info!(daemon = %name, "Shutdown during startup delay");
                return;
            }
        }
    }

    loop {
        // Check shutdown before attempting to run
        if *shutdown_rx.borrow() {
            info!(daemon = %name, "Daemon shutting down");
            break;
        }

        // Try to acquire leadership if required
        if leader_elected {
            match try_acquire_leadership(&pool, &name, node_id).await {
                Ok(true) => {
                    info!(daemon = %name, "Acquired leadership");
                }
                Ok(false) => {
                    // Another node has leadership, wait and retry
                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_secs(5)) => {}
                        _ = shutdown_rx.changed() => {
                            info!(daemon = %name, "Shutdown while waiting for leadership");
                            return;
                        }
                    }
                    continue;
                }
                Err(e) => {
                    warn!(daemon = %name, error = %e, "Failed to check leadership");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }
            }
        }

        // Update status to running
        if let Err(e) = update_daemon_status(&pool, &name, DaemonStatus::Running).await {
            warn!(daemon = %name, error = %e, "Failed to update status");
        }

        let instance_id = Uuid::new_v4();
        info!(daemon = %name, instance = %instance_id, "Starting daemon");

        // Create context with shutdown receiver
        let (daemon_shutdown_tx, daemon_shutdown_rx) = watch::channel(false);

        // Forward shutdown signal
        let shutdown_rx_clone = shutdown_rx.clone();
        let shutdown_tx_clone = daemon_shutdown_tx.clone();
        tokio::spawn(async move {
            let mut rx = shutdown_rx_clone;
            while rx.changed().await.is_ok() {
                if *rx.borrow() {
                    let _ = shutdown_tx_clone.send(true);
                    break;
                }
            }
        });

        let ctx = DaemonContext::new(
            name.clone(),
            instance_id,
            pool.clone(),
            http_client.clone(),
            daemon_shutdown_rx,
        );

        // Run the daemon
        let result = std::panic::AssertUnwindSafe((entry.handler)(&ctx))
            .catch_unwind()
            .await;

        match result {
            Ok(Ok(())) => {
                info!(daemon = %name, "Daemon completed successfully");
                if let Err(e) = update_daemon_status(&pool, &name, DaemonStatus::Stopped).await {
                    warn!(daemon = %name, error = %e, "Failed to update status");
                }
                break;
            }
            Ok(Err(e)) => {
                error!(daemon = %name, error = %e, "Daemon failed with error");
                if let Err(e) = record_daemon_error(&pool, &name, &e.to_string()).await {
                    warn!(daemon = %name, error = %e, "Failed to record error");
                }
            }
            Err(_) => {
                error!(daemon = %name, "Daemon panicked");
                if let Err(e) = record_daemon_error(&pool, &name, "Daemon panicked").await {
                    warn!(daemon = %name, error = %e, "Failed to record panic");
                }
            }
        }

        // Check shutdown again
        if *shutdown_rx.borrow() {
            info!(daemon = %name, "Daemon shutting down after failure");
            break;
        }

        // Check restart policy
        if !restart_on_panic {
            warn!(daemon = %name, "Restart disabled, daemon stopping");
            if let Err(e) = update_daemon_status(&pool, &name, DaemonStatus::Failed).await {
                warn!(daemon = %name, error = %e, "Failed to update status");
            }
            break;
        }

        restarts += 1;

        // Check max restarts
        if let Some(max) = max_restarts {
            if restarts >= max {
                error!(daemon = %name, restarts = restarts, max = max, "Max restarts exceeded");
                if let Err(e) = update_daemon_status(&pool, &name, DaemonStatus::Failed).await {
                    warn!(daemon = %name, error = %e, "Failed to update status");
                }
                break;
            }
        }

        // Update status to restarting
        if let Err(e) = update_daemon_status(&pool, &name, DaemonStatus::Restarting).await {
            warn!(daemon = %name, error = %e, "Failed to update status");
        }

        info!(daemon = %name, restarts = restarts, delay = ?restart_delay, "Restarting daemon");

        // Wait before restart
        tokio::select! {
            _ = tokio::time::sleep(restart_delay) => {}
            _ = shutdown_rx.changed() => {
                info!(daemon = %name, "Shutdown during restart delay");
                break;
            }
        }
    }

    // Release leadership if we held it
    if leader_elected {
        let _ = release_leadership(&pool, &name, node_id).await;
    }
}

async fn try_acquire_leadership(pool: &PgPool, daemon_name: &str, node_id: Uuid) -> Result<bool> {
    // Use advisory lock for leader election
    // Hash the daemon name to get a consistent lock ID
    let lock_id = daemon_name
        .bytes()
        .fold(0i64, |acc, b| acc.wrapping_add(b as i64).wrapping_mul(31));

    let result: (bool,) = sqlx::query_as(
        "SELECT pg_try_advisory_lock($1)",
    )
    .bind(lock_id)
    .fetch_one(pool)
    .await
    .map_err(|e| forge_core::ForgeError::Database(e.to_string()))?;

    if result.0 {
        // Update daemon record with our node_id
        sqlx::query(
            "UPDATE forge_daemons SET node_id = $1 WHERE name = $2",
        )
        .bind(node_id)
        .bind(daemon_name)
        .execute(pool)
        .await
        .map_err(|e| forge_core::ForgeError::Database(e.to_string()))?;
    }

    Ok(result.0)
}

async fn release_leadership(pool: &PgPool, daemon_name: &str, _node_id: Uuid) -> Result<()> {
    let lock_id = daemon_name
        .bytes()
        .fold(0i64, |acc, b| acc.wrapping_add(b as i64).wrapping_mul(31));

    sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(lock_id)
        .execute(pool)
        .await
        .map_err(|e| forge_core::ForgeError::Database(e.to_string()))?;

    Ok(())
}

async fn update_daemon_status(pool: &PgPool, name: &str, status: DaemonStatus) -> Result<()> {
    sqlx::query(
        "UPDATE forge_daemons SET status = $1, last_heartbeat = NOW() WHERE name = $2",
    )
    .bind(status.as_str())
    .bind(name)
    .execute(pool)
    .await
    .map_err(|e| forge_core::ForgeError::Database(e.to_string()))?;

    Ok(())
}

async fn record_daemon_error(pool: &PgPool, name: &str, error: &str) -> Result<()> {
    sqlx::query(
        "UPDATE forge_daemons SET status = 'failed', last_error = $1, last_heartbeat = NOW() WHERE name = $2",
    )
    .bind(error)
    .bind(name)
    .execute(pool)
    .await
    .map_err(|e| forge_core::ForgeError::Database(e.to_string()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = DaemonRunnerConfig::default();
        assert_eq!(config.health_check_interval, Duration::from_secs(30));
        assert_eq!(config.heartbeat_interval, Duration::from_secs(10));
    }
}
