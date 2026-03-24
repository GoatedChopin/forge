use std::future::Future;
use std::pin::Pin;

use serde::{Serialize, de::DeserializeOwned};

use super::context::{MutationContext, QueryContext};
use crate::error::Result;

/// Information about a registered function.
#[derive(Debug, Clone)]
pub struct FunctionInfo {
    /// Function name (used for routing).
    pub name: &'static str,
    /// Human-readable description.
    pub description: Option<&'static str>,
    /// Kind of function.
    pub kind: FunctionKind,
    /// Required role (if any, implies auth required).
    pub required_role: Option<&'static str>,
    /// Whether this function is public (no auth).
    pub is_public: bool,
    /// Cache TTL in seconds (for queries).
    pub cache_ttl: Option<u64>,
    /// Timeout in seconds.
    pub timeout: Option<u64>,
    /// Default timeout in seconds for outbound HTTP requests made via the
    /// circuit-breaker client. `None` means no request timeout is applied.
    pub http_timeout: Option<u64>,
    /// Rate limit: requests per time window.
    pub rate_limit_requests: Option<u32>,
    /// Rate limit: time window in seconds.
    pub rate_limit_per_secs: Option<u64>,
    /// Rate limit: bucket key type (user, ip, tenant, global).
    pub rate_limit_key: Option<&'static str>,
    /// Log level for access logging: "trace", "debug", "info", "warn", "error", "off".
    /// Defaults to "trace" if not specified.
    pub log_level: Option<&'static str>,
    /// Table dependencies extracted at compile time for reactive subscriptions.
    /// Empty slice means tables could not be determined (dynamic SQL).
    pub table_dependencies: &'static [&'static str],
    /// Columns referenced in SELECT clauses, extracted at compile time.
    /// Used for fine-grained invalidation: skip re-execution when changed columns
    /// don't intersect with selected columns. Empty means unknown (invalidate always).
    pub selected_columns: &'static [&'static str],
    /// Whether this mutation should be wrapped in a database transaction.
    /// Only applies to mutations. When true, jobs are buffered and inserted
    /// atomically with the mutation via the outbox pattern.
    pub transactional: bool,
    /// Force this query to read from the primary database instead of replicas.
    /// Use for read-after-write consistency (e.g., post-mutation confirmation,
    /// permission checks depending on just-written state).
    pub consistent: bool,
    /// Whether the function signature has input arguments beyond the context.
    /// When false, identity scope enforcement is skipped since there are no
    /// args to carry scope fields. Auth is still enforced via the JWT.
    pub has_input_args: bool,
}

/// The kind of function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionKind {
    Query,
    Mutation,
}

impl std::fmt::Display for FunctionKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FunctionKind::Query => write!(f, "query"),
            FunctionKind::Mutation => write!(f, "mutation"),
        }
    }
}

/// A query function (read-only, cacheable, subscribable).
///
/// Queries:
/// - Can only read from the database
/// - Are automatically cached based on arguments
/// - Can be subscribed to for real-time updates
/// - Should be deterministic (same inputs → same outputs)
/// - Should not have side effects
pub trait ForgeQuery: Send + Sync + 'static {
    /// The input arguments type.
    type Args: DeserializeOwned + Serialize + Send + Sync;
    /// The output type.
    type Output: Serialize + Send;

    /// Function metadata.
    fn info() -> FunctionInfo;

    /// Execute the query.
    fn execute(
        ctx: &QueryContext,
        args: Self::Args,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Output>> + Send + '_>>;
}

/// A mutation function (transactional write).
///
/// Mutations:
/// - Run within a database transaction
/// - Can read and write to the database
/// - Should NOT call external APIs (use Actions)
/// - Are atomic: all changes commit or none do
pub trait ForgeMutation: Send + Sync + 'static {
    /// The input arguments type.
    type Args: DeserializeOwned + Serialize + Send + Sync;
    /// The output type.
    type Output: Serialize + Send;

    /// Function metadata.
    fn info() -> FunctionInfo;

    /// Execute the mutation within a transaction.
    fn execute(
        ctx: &MutationContext,
        args: Self::Args,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Output>> + Send + '_>>;
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn test_function_kind_display() {
        assert_eq!(format!("{}", FunctionKind::Query), "query");
        assert_eq!(format!("{}", FunctionKind::Mutation), "mutation");
    }

    #[test]
    fn test_function_info() {
        let info = FunctionInfo {
            name: "get_user",
            description: Some("Get a user by ID"),
            kind: FunctionKind::Query,
            required_role: None,
            is_public: false,
            cache_ttl: Some(300),
            timeout: Some(30),
            http_timeout: Some(5),
            rate_limit_requests: Some(100),
            rate_limit_per_secs: Some(60),
            rate_limit_key: Some("user"),
            log_level: Some("debug"),
            table_dependencies: &["users"],
            selected_columns: &["id", "name", "email"],
            transactional: false,
            consistent: false,
            has_input_args: true,
        };

        assert_eq!(info.name, "get_user");
        assert_eq!(info.kind, FunctionKind::Query);
        assert_eq!(info.cache_ttl, Some(300));
        assert_eq!(info.http_timeout, Some(5));
        assert_eq!(info.rate_limit_requests, Some(100));
        assert_eq!(info.log_level, Some("debug"));
        assert_eq!(info.table_dependencies, &["users"]);
    }
}
