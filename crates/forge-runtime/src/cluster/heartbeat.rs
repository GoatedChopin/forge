use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::Duration;

use forge_core::cluster::NodeId;
use tokio::sync::watch;

/// Heartbeat loop configuration.
#[derive(Debug, Clone)]
pub struct HeartbeatConfig {
    /// Interval between heartbeats.
    pub interval: Duration,
    /// Threshold for marking nodes as dead.
    pub dead_threshold: Duration,
    /// Whether to mark dead nodes.
    pub mark_dead_nodes: bool,
    /// Maximum heartbeat interval when cluster is stable.
    pub max_interval: Duration,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(5),
            dead_threshold: Duration::from_secs(15),
            mark_dead_nodes: true,
            max_interval: Duration::from_secs(60),
        }
    }
}

/// Heartbeat loop for cluster health.
pub struct HeartbeatLoop {
    pool: sqlx::PgPool,
    node_id: NodeId,
    config: HeartbeatConfig,
    running: Arc<AtomicBool>,
    shutdown_tx: watch::Sender<bool>,
    shutdown_rx: watch::Receiver<bool>,
    current_interval_ms: AtomicU64,
    stable_count: AtomicU32,
    last_active_count: AtomicU32,
}

impl HeartbeatLoop {
    /// Create a new heartbeat loop.
    pub fn new(pool: sqlx::PgPool, node_id: NodeId, config: HeartbeatConfig) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let interval_ms = config.interval.as_millis() as u64;
        Self {
            pool,
            node_id,
            config,
            running: Arc::new(AtomicBool::new(false)),
            shutdown_tx,
            shutdown_rx,
            current_interval_ms: AtomicU64::new(interval_ms),
            stable_count: AtomicU32::new(0),
            last_active_count: AtomicU32::new(0),
        }
    }

    /// Check if the loop is running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Get a shutdown receiver.
    pub fn shutdown_receiver(&self) -> watch::Receiver<bool> {
        self.shutdown_rx.clone()
    }

    /// Stop the heartbeat loop.
    pub fn stop(&self) {
        let _ = self.shutdown_tx.send(true);
        self.running.store(false, Ordering::SeqCst);
    }

    /// Run the heartbeat loop.
    pub async fn run(&self) {
        self.running.store(true, Ordering::SeqCst);
        let mut shutdown_rx = self.shutdown_rx.clone();

        loop {
            let interval = self.current_interval();
            tokio::select! {
                _ = tokio::time::sleep(interval) => {
                    // Update our heartbeat
                    if let Err(e) = self.send_heartbeat().await {
                        tracing::debug!(error = %e, "Failed to send heartbeat");
                    }

                    // Adjust interval based on cluster stability
                    self.adjust_interval().await;

                    // Mark dead nodes if enabled
                    if self.config.mark_dead_nodes
                        && let Err(e) = self.mark_dead_nodes().await
                    {
                        tracing::debug!(error = %e, "Failed to mark dead nodes");
                    }
                }
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        tracing::debug!("Heartbeat loop shutting down");
                        break;
                    }
                }
            }
        }

        self.running.store(false, Ordering::SeqCst);
    }

    /// Current adaptive interval.
    fn current_interval(&self) -> Duration {
        Duration::from_millis(self.current_interval_ms.load(Ordering::Relaxed))
    }

    /// Query the number of active nodes in the cluster.
    async fn active_node_count(&self) -> forge_core::Result<u32> {
        let row: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM forge_nodes WHERE status = 'active'")
                .fetch_one(&self.pool)
                .await
                .map_err(|e| forge_core::ForgeError::Database(e.to_string()))?;

        Ok(row.0 as u32)
    }

    /// Adjust heartbeat interval based on cluster stability.
    async fn adjust_interval(&self) {
        let count = match self.active_node_count().await {
            Ok(c) => c,
            Err(e) => {
                tracing::debug!(error = %e, "Failed to query active node count");
                return;
            }
        };

        let last = self.last_active_count.load(Ordering::Relaxed);
        if last != 0 && count == last {
            let stable = self.stable_count.fetch_add(1, Ordering::Relaxed) + 1;
            if stable >= 3 {
                let base_ms = self.config.interval.as_millis() as u64;
                let max_ms = self.config.max_interval.as_millis() as u64;
                let cur = self.current_interval_ms.load(Ordering::Relaxed);
                let next = (cur * 2).min(max_ms).max(base_ms);
                self.current_interval_ms.store(next, Ordering::Relaxed);
            }
        } else {
            // Membership changed, reset to base interval
            self.stable_count.store(0, Ordering::Relaxed);
            let base_ms = self.config.interval.as_millis() as u64;
            self.current_interval_ms.store(base_ms, Ordering::Relaxed);
        }
        self.last_active_count.store(count, Ordering::Relaxed);
    }

    /// Send a heartbeat update.
    async fn send_heartbeat(&self) -> forge_core::Result<()> {
        sqlx::query(
            r#"
            UPDATE forge_nodes
            SET last_heartbeat = NOW()
            WHERE id = $1
            "#,
        )
        .bind(self.node_id.as_uuid())
        .execute(&self.pool)
        .await
        .map_err(|e| forge_core::ForgeError::Database(e.to_string()))?;

        Ok(())
    }

    /// Mark stale nodes as dead. Threshold is the greater of the configured
    /// dead_threshold and 3x the current adaptive interval.
    async fn mark_dead_nodes(&self) -> forge_core::Result<u64> {
        let adaptive = self.current_interval().as_secs_f64() * 3.0;
        let configured = self.config.dead_threshold.as_secs_f64();
        let threshold_secs = adaptive.max(configured);

        let result = sqlx::query(
            r#"
            UPDATE forge_nodes
            SET status = 'dead'
            WHERE status = 'active'
              AND last_heartbeat < NOW() - make_interval(secs => $1)
            "#,
        )
        .bind(threshold_secs)
        .execute(&self.pool)
        .await
        .map_err(|e| forge_core::ForgeError::Database(e.to_string()))?;

        let count = result.rows_affected();
        if count > 0 {
            tracing::warn!(count, "Marked nodes as dead");
        }

        Ok(count)
    }

    /// Update load metrics.
    pub async fn update_load(
        &self,
        current_connections: u32,
        current_jobs: u32,
        cpu_usage: f32,
        memory_usage: f32,
    ) -> forge_core::Result<()> {
        sqlx::query(
            r#"
            UPDATE forge_nodes
            SET current_connections = $2,
                current_jobs = $3,
                cpu_usage = $4,
                memory_usage = $5,
                last_heartbeat = NOW()
            WHERE id = $1
            "#,
        )
        .bind(self.node_id.as_uuid())
        .bind(current_connections as i32)
        .bind(current_jobs as i32)
        .bind(cpu_usage)
        .bind(memory_usage)
        .execute(&self.pool)
        .await
        .map_err(|e| forge_core::ForgeError::Database(e.to_string()))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heartbeat_config_default() {
        let config = HeartbeatConfig::default();
        assert_eq!(config.interval, Duration::from_secs(5));
        assert_eq!(config.dead_threshold, Duration::from_secs(15));
        assert!(config.mark_dead_nodes);
        assert_eq!(config.max_interval, Duration::from_secs(60));
    }
}
