//! Database pool wrapper that hides the underlying sqlx version.
//!
//! Forge uses sqlx internally but does not expose `sqlx::PgPool` on its public
//! API. The [`ForgePool`] newtype is the stable handle handed to user code
//! (most prominently `ForgeBuilder::custom_routes`), so a future sqlx major
//! upgrade — or a switch to a different driver — does not require a
//! breaking change in user code.
//!
//! Drop down to the underlying pool with [`ForgePool::as_sqlx_pool`] when
//! you need to call sqlx directly. That escape hatch is explicitly
//! "unstable across forge versions": if you use it, plan to update with
//! the framework.

use std::sync::Arc;

/// Stable handle to the framework-managed database pool.
///
/// `Clone` is cheap (it clones an `Arc` internally), so cloning to hand a
/// pool to a route state is fine.
#[derive(Debug, Clone)]
pub struct ForgePool {
    pool: Arc<sqlx::PgPool>,
}

impl ForgePool {
    /// Wrap a `sqlx::PgPool`.
    ///
    /// Users typically receive a `ForgePool` from the framework rather than
    /// constructing one. This constructor is exposed for tests and for
    /// integrating with custom database setup logic.
    pub fn from_sqlx(pool: sqlx::PgPool) -> Self {
        Self {
            pool: Arc::new(pool),
        }
    }

    /// Borrow the underlying `sqlx::PgPool`.
    ///
    /// Use this when you need to call sqlx APIs directly. This is an
    /// **escape hatch**: the sqlx major version exposed here may change in
    /// future Forge releases. Code that depends on a specific sqlx surface
    /// should pin both Forge and sqlx versions explicitly.
    pub fn as_sqlx_pool(&self) -> &sqlx::PgPool {
        &self.pool
    }
}
