use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{RwLock, mpsc};
use tokio::time::Instant;

use forge_core::realtime::{Change, SubscriptionId};

use super::manager::SubscriptionManager;

/// Configuration for the invalidation engine.
///
/// The invalidation engine uses a debounce algorithm to batch rapid changes
/// into single re-executions. This prevents "thundering herd" scenarios where
/// a batch insert of 1000 rows would trigger 1000 subscription refreshes.
///
/// The algorithm works as follows:
/// 1. When a change arrives, record the subscription as pending
/// 2. Wait for `debounce_ms` of silence (no new changes to that subscription)
/// 3. If `max_debounce_ms` passes since the first change, flush anyway
/// 4. If buffer exceeds `max_buffer_size`, flush immediately (memory protection)
///
/// This balances latency (users want updates fast) against efficiency (batching
/// reduces database load). Default values target 50ms debounce with 200ms max
/// wait, meaning updates arrive within 200ms worst-case.
#[derive(Debug, Clone)]
pub struct InvalidationConfig {
    /// Debounce window in milliseconds.
    /// After a change, wait this long for more changes before invalidating.
    pub debounce_ms: u64,
    /// Maximum debounce wait in milliseconds.
    /// Even if changes keep arriving, invalidate after this duration.
    pub max_debounce_ms: u64,
    /// Whether to coalesce changes by table.
    /// When true, multiple changes to the same table become a single invalidation.
    pub coalesce_by_table: bool,
    /// Maximum changes to buffer before forcing flush.
    /// Prevents unbounded memory growth during high-throughput periods.
    pub max_buffer_size: usize,
}

impl Default for InvalidationConfig {
    fn default() -> Self {
        Self {
            debounce_ms: 50,
            max_debounce_ms: 200,
            coalesce_by_table: true,
            max_buffer_size: 1000,
        }
    }
}

/// Pending invalidation for a subscription.
#[derive(Debug)]
struct PendingInvalidation {
    /// Subscription ID.
    #[allow(dead_code)]
    subscription_id: SubscriptionId,
    /// Tables that changed.
    changed_tables: HashSet<String>,
    /// When this invalidation was first queued.
    first_change: Instant,
    /// When the last change was received.
    last_change: Instant,
}

/// Engine for determining which subscriptions need re-execution.
pub struct InvalidationEngine {
    subscription_manager: Arc<SubscriptionManager>,
    #[allow(dead_code)]
    config: InvalidationConfig,
    /// Pending invalidations per subscription.
    pending: Arc<RwLock<HashMap<SubscriptionId, PendingInvalidation>>>,
    /// Channel for signaling invalidations.
    #[allow(dead_code)]
    invalidation_tx: mpsc::Sender<Vec<SubscriptionId>>,
    #[allow(dead_code)]
    invalidation_rx: Arc<RwLock<mpsc::Receiver<Vec<SubscriptionId>>>>,
}

impl InvalidationEngine {
    /// Create a new invalidation engine.
    pub fn new(subscription_manager: Arc<SubscriptionManager>, config: InvalidationConfig) -> Self {
        let (invalidation_tx, invalidation_rx) = mpsc::channel(1024);

        Self {
            subscription_manager,
            config,
            pending: Arc::new(RwLock::new(HashMap::new())),
            invalidation_tx,
            invalidation_rx: Arc::new(RwLock::new(invalidation_rx)),
        }
    }

    /// Process a database change.
    pub async fn process_change(&self, change: Change) {
        // Find affected subscriptions
        let affected = self
            .subscription_manager
            .find_affected_subscriptions(&change)
            .await;

        if affected.is_empty() {
            return;
        }

        tracing::debug!(
            table = %change.table,
            affected_count = affected.len(),
            "Found affected subscriptions for change"
        );

        let now = Instant::now();
        let mut pending = self.pending.write().await;

        for sub_id in affected {
            let entry = pending
                .entry(sub_id)
                .or_insert_with(|| PendingInvalidation {
                    subscription_id: sub_id,
                    changed_tables: HashSet::new(),
                    first_change: now,
                    last_change: now,
                });

            entry.changed_tables.insert(change.table.clone());
            entry.last_change = now;
        }

        // Check if we should flush due to buffer size
        if pending.len() >= self.config.max_buffer_size {
            drop(pending);
            self.flush_all().await;
        }
    }

    /// Check for subscriptions that need to be invalidated.
    pub async fn check_pending(&self) -> Vec<SubscriptionId> {
        let now = Instant::now();
        let debounce = Duration::from_millis(self.config.debounce_ms);
        let max_debounce = Duration::from_millis(self.config.max_debounce_ms);

        let mut pending = self.pending.write().await;
        let mut ready = Vec::new();

        pending.retain(|_, inv| {
            let since_last = now.duration_since(inv.last_change);
            let since_first = now.duration_since(inv.first_change);

            // Ready if debounce window passed or max wait exceeded
            if since_last >= debounce || since_first >= max_debounce {
                ready.push(inv.subscription_id);
                false // Remove from pending
            } else {
                true // Keep in pending
            }
        });

        ready
    }

    /// Flush all pending invalidations immediately.
    pub async fn flush_all(&self) -> Vec<SubscriptionId> {
        let mut pending = self.pending.write().await;
        let ready: Vec<SubscriptionId> = pending.keys().copied().collect();
        pending.clear();
        ready
    }

    /// Get the invalidation receiver for consuming invalidation events.
    pub async fn take_receiver(&self) -> Option<mpsc::Receiver<Vec<SubscriptionId>>> {
        let _rx_guard = self.invalidation_rx.write().await;
        // We can only take once, so this is a simple swap
        // In practice, you'd use a different pattern
        None // Simplified - receiver is accessed via run loop
    }

    /// Run the invalidation check loop.
    pub async fn run(&self) {
        let check_interval = Duration::from_millis(self.config.debounce_ms / 2);

        loop {
            tokio::time::sleep(check_interval).await;

            let ready = self.check_pending().await;
            if !ready.is_empty() && self.invalidation_tx.send(ready).await.is_err() {
                // Receiver dropped, stop the loop
                break;
            }
        }
    }

    /// Get pending count for monitoring.
    pub async fn pending_count(&self) -> usize {
        self.pending.read().await.len()
    }

    /// Get statistics about the invalidation engine.
    pub async fn stats(&self) -> InvalidationStats {
        let pending = self.pending.read().await;

        let mut tables_pending = HashSet::new();
        for inv in pending.values() {
            tables_pending.extend(inv.changed_tables.iter().cloned());
        }

        InvalidationStats {
            pending_subscriptions: pending.len(),
            pending_tables: tables_pending.len(),
        }
    }
}

/// Statistics about the invalidation engine.
#[derive(Debug, Clone, Default)]
pub struct InvalidationStats {
    /// Number of subscriptions pending invalidation.
    pub pending_subscriptions: usize,
    /// Number of unique tables with pending changes.
    pub pending_tables: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invalidation_config_default() {
        let config = InvalidationConfig::default();
        assert_eq!(config.debounce_ms, 50);
        assert_eq!(config.max_debounce_ms, 200);
        assert!(config.coalesce_by_table);
    }

    #[tokio::test]
    async fn test_invalidation_engine_creation() {
        let subscription_manager = Arc::new(SubscriptionManager::new(50));
        let engine = InvalidationEngine::new(subscription_manager, InvalidationConfig::default());

        assert_eq!(engine.pending_count().await, 0);

        let stats = engine.stats().await;
        assert_eq!(stats.pending_subscriptions, 0);
        assert_eq!(stats.pending_tables, 0);
    }

    #[tokio::test]
    async fn test_invalidation_flush_all() {
        let subscription_manager = Arc::new(SubscriptionManager::new(50));
        let engine = InvalidationEngine::new(subscription_manager, InvalidationConfig::default());

        // Flush on empty should return empty
        let flushed = engine.flush_all().await;
        assert!(flushed.is_empty());
    }
}
