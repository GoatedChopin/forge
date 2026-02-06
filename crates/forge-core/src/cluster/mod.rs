//! Cluster coordination and node management.
//!
//! Forge nodes discover each other via PostgreSQL without external service discovery.
//! Nodes register in `forge_nodes`, send heartbeats, and are marked dead if stale.
//!
//! # Node Roles
//!
//! Each node can assume one or more roles:
//!
//! | Role | Responsibility |
//! |------|----------------|
//! | Gateway | HTTP/WebSocket endpoints |
//! | Function | Query/mutation execution |
//! | Worker | Background job processing |
//! | Scheduler | Cron scheduling (singleton via advisory lock) |
//!
//! # Leader Election
//!
//! Singleton processes use PostgreSQL advisory locks. Only one node holds the
//! lock at a time. If the leader crashes, PostgreSQL releases the lock and
//! a standby node acquires it.
//!
//! # Key Types
//!
//! - [`NodeId`] - Unique node identifier (UUID)
//! - [`NodeInfo`] - Node metadata (hostname, roles, capabilities)
//! - [`NodeRole`] - Role enum (Gateway, Function, Worker, Scheduler)
//! - [`LeaderRole`] - Singleton role requiring leader election

mod node;
mod roles;
mod traits;

pub use node::{NodeId, NodeInfo, NodeStatus, ParseNodeStatusError};
pub use roles::{LeaderRole, NodeRole, ParseNodeRoleError, ParseLeaderRoleError};
pub use traits::{ClusterInfo, LeaderInfo};
