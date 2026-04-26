use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use std::time::Duration;

use chrono::Utc;
use sqlx::PgPool;

use super::registry::{DrainEntry, WorkflowRegistry};
use forge_core::Result;

/// Cache TTL for [`WorkflowReadiness::refresh`]. Re-running the drain query on
/// every readiness probe (typical 1Hz LB poll) hammers PG; 5s is short enough
/// that operators clearing stuck runs see traffic recover quickly.
pub const DRAIN_CACHE_TTL: Duration = Duration::from_secs(5);

/// Shared, lock-free readiness state for the workflow drain check.
///
/// Populated at boot via [`WorkflowReadiness::refresh`] and cached. The gateway
/// readiness handler refreshes the cache lazily when it ages out so the
/// `/_api/ready` endpoint flips back to 200 within ~5s of the operator
/// clearing the last stranded run in PG.
#[derive(Debug, Default)]
pub struct WorkflowReadiness {
    drain_pending: AtomicUsize,
    last_check_at_unix: AtomicI64,
}

impl WorkflowReadiness {
    /// Construct an empty readiness handle. Call [`refresh`](Self::refresh)
    /// to populate the count.
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Current drain-pending count.
    pub fn drain_pending(&self) -> usize {
        self.drain_pending.load(Ordering::Relaxed)
    }

    /// True when the cached count is still within [`DRAIN_CACHE_TTL`].
    pub fn is_fresh(&self) -> bool {
        let last = self.last_check_at_unix.load(Ordering::Relaxed);
        if last == 0 {
            return false;
        }
        let age = Utc::now().timestamp() - last;
        age >= 0 && (age as u64) < DRAIN_CACHE_TTL.as_secs()
    }

    /// Re-run the drain check, replace the cached count, and return the entries.
    pub async fn refresh(
        &self,
        registry: &WorkflowRegistry,
        pool: &PgPool,
    ) -> Result<Vec<DrainEntry>> {
        let entries = registry.drain_check(pool).await?;
        let total: u64 = entries.iter().map(|e| e.in_flight_count).sum();
        self.drain_pending
            .store(total.min(usize::MAX as u64) as usize, Ordering::Relaxed);
        self.last_check_at_unix
            .store(Utc::now().timestamp(), Ordering::Relaxed);
        Ok(entries)
    }

    /// Refresh only when [`is_fresh`](Self::is_fresh) is false. Used by the
    /// readiness handler to avoid hammering PG on hot probe paths.
    pub async fn refresh_if_stale(&self, registry: &WorkflowRegistry, pool: &PgPool) -> Result<()> {
        if self.is_fresh() {
            return Ok(());
        }
        self.refresh(registry, pool).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn readiness_starts_stale() {
        let r = WorkflowReadiness::new();
        assert!(!r.is_fresh());
        assert_eq!(r.drain_pending(), 0);
    }
}
