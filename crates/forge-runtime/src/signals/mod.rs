//! Signals: built-in product analytics and frontend diagnostics.
//!
//! Zero-config, GDPR-compliant (no cookies, no persistent client IDs).
//! Auto-captures RPC calls, sessions, page views, and frontend errors.
//! Visualization via Grafana dashboards over PostgreSQL.

pub mod bot;
pub mod collector;
pub mod device;
pub mod endpoints;
pub mod partition;
pub mod session;
pub mod views;
pub mod visitor;

pub use collector::SignalsCollector;

#[cfg(all(test, feature = "testcontainers"))]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests;
