//! Execution contexts for queries and mutations.
//!
//! Every function receives a context providing access to:
//!
//! - Database connection (pool or transaction)
//! - Authentication state (user ID, roles, claims)
//! - Request metadata (request ID, trace ID, client IP)
//! - Environment variables
//! - Job/workflow dispatch (mutations only)
//!
//! # QueryContext vs MutationContext
//!
//! | Feature | QueryContext | MutationContext |
//! |---------|--------------|-----------------|
//! | Database | Pool (read-only) | Transaction or pool |
//! | Dispatch jobs | No | Yes |
//! | Start workflows | No | Yes |
//! | HTTP client | No | Yes (circuit breaker) |
//!
//! # Transactional Mutations
//!
//! When `transactional = true` (default), mutations run in a transaction.
//! Jobs and workflows dispatched during the mutation are buffered and only
//! inserted after the transaction commits successfully.
//!
//! ```text
//! BEGIN
//!   ├── ctx.db().execute(...)
//!   ├── ctx.dispatch_job("send_email", ...)  // buffered
//!   └── return Ok(result)
//! COMMIT
//!   └── INSERT INTO forge_jobs (buffered jobs)
//! ```

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use futures_core::future::BoxFuture;
use futures_core::stream::BoxStream;
use sqlx::postgres::{PgConnection, PgQueryResult, PgRow};
use sqlx::{Postgres, Transaction};
use tokio::sync::Mutex as AsyncMutex;
use uuid::Uuid;

use tracing::Instrument;

use super::dispatch::{JobDispatch, WorkflowDispatch};
use crate::auth::Claims;
use crate::env::{EnvAccess, EnvProvider, RealEnvProvider};
use crate::http::CircuitBreakerClient;
use crate::job::JobInfo;

/// Token issuer for signing JWTs.
///
/// Implemented by the runtime when HMAC auth is configured.
/// Available via `ctx.issue_token()` in mutation handlers.
pub trait TokenIssuer: Send + Sync {
    /// Sign the given claims into a JWT string.
    fn sign(&self, claims: &Claims) -> crate::error::Result<String>;
}

/// Connection wrapper that implements sqlx's `Executor` trait with automatic
/// `db.query` tracing spans.
///
/// Obtain via `ctx.conn().await?` in mutation handlers.
/// Works with compile-time checked macros via `&mut conn`.
///
/// ```ignore
/// let mut conn = ctx.conn().await?;
/// sqlx::query_as!(User, "SELECT * FROM users WHERE id = $1", id)
///     .fetch_one(&mut *conn)
///     .await?
/// ```
pub enum ForgeConn<'a> {
    Pool(sqlx::pool::PoolConnection<Postgres>),
    Tx(tokio::sync::MutexGuard<'a, Transaction<'static, Postgres>>),
}

impl std::ops::Deref for ForgeConn<'_> {
    type Target = PgConnection;
    fn deref(&self) -> &PgConnection {
        match self {
            ForgeConn::Pool(c) => c,
            ForgeConn::Tx(g) => g,
        }
    }
}

impl std::ops::DerefMut for ForgeConn<'_> {
    fn deref_mut(&mut self) -> &mut PgConnection {
        match self {
            ForgeConn::Pool(c) => c,
            ForgeConn::Tx(g) => g,
        }
    }
}

/// Pool wrapper that adds `db.query` tracing spans to every database operation.
///
/// Returned by [`QueryContext::db()`]. Implements sqlx's [`sqlx::Executor`] trait,
/// so it works as a drop-in replacement for `&PgPool` with compile-time
/// checked macros (`query!`, `query_as!`).
///
/// ```ignore
/// sqlx::query_as!(User, "SELECT * FROM users")
///     .fetch_all(ctx.db())
///     .await?
/// ```
#[derive(Clone)]
pub struct ForgeDb(sqlx::PgPool);

impl std::fmt::Debug for ForgeDb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ForgeDb").finish()
    }
}

impl ForgeDb {
    /// Create a `ForgeDb` from a pool reference. Clones the Arc-backed pool handle.
    pub fn from_pool(pool: &sqlx::PgPool) -> Self {
        Self(pool.clone())
    }
}

fn sql_operation(sql: &str) -> &'static str {
    let bytes = sql.trim_start().as_bytes();
    match bytes.get(..6) {
        Some(prefix) if prefix.eq_ignore_ascii_case(b"select") => "SELECT",
        Some(prefix) if prefix.eq_ignore_ascii_case(b"insert") => "INSERT",
        Some(prefix) if prefix.eq_ignore_ascii_case(b"update") => "UPDATE",
        Some(prefix) if prefix.eq_ignore_ascii_case(b"delete") => "DELETE",
        _ => "OTHER",
    }
}

impl sqlx::Executor<'static> for ForgeDb {
    type Database = Postgres;

    fn fetch_many<'e, 'q: 'e, E>(
        self,
        query: E,
    ) -> BoxStream<'e, Result<sqlx::Either<PgQueryResult, PgRow>, sqlx::Error>>
    where
        E: sqlx::Execute<'q, Postgres> + 'q,
    {
        (&self.0).fetch_many(query)
    }

    fn fetch_optional<'e, 'q: 'e, E>(
        self,
        query: E,
    ) -> BoxFuture<'e, Result<Option<PgRow>, sqlx::Error>>
    where
        E: sqlx::Execute<'q, Postgres> + 'q,
    {
        let op = sql_operation(query.sql());
        let span =
            tracing::info_span!("db.query", db.system = "postgresql", db.operation.name = op,);
        Box::pin(
            async move { sqlx::Executor::fetch_optional(&self.0, query).await }.instrument(span),
        )
    }

    fn execute<'e, 'q: 'e, E>(self, query: E) -> BoxFuture<'e, Result<PgQueryResult, sqlx::Error>>
    where
        E: sqlx::Execute<'q, Postgres> + 'q,
    {
        let op = sql_operation(query.sql());
        let span =
            tracing::info_span!("db.query", db.system = "postgresql", db.operation.name = op,);
        Box::pin(async move { sqlx::Executor::execute(&self.0, query).await }.instrument(span))
    }

    fn fetch_all<'e, 'q: 'e, E>(self, query: E) -> BoxFuture<'e, Result<Vec<PgRow>, sqlx::Error>>
    where
        E: sqlx::Execute<'q, Postgres> + 'q,
    {
        let op = sql_operation(query.sql());
        let span =
            tracing::info_span!("db.query", db.system = "postgresql", db.operation.name = op,);
        Box::pin(async move { sqlx::Executor::fetch_all(&self.0, query).await }.instrument(span))
    }

    fn fetch_one<'e, 'q: 'e, E>(self, query: E) -> BoxFuture<'e, Result<PgRow, sqlx::Error>>
    where
        E: sqlx::Execute<'q, Postgres> + 'q,
    {
        let op = sql_operation(query.sql());
        let span =
            tracing::info_span!("db.query", db.system = "postgresql", db.operation.name = op,);
        Box::pin(async move { sqlx::Executor::fetch_one(&self.0, query).await }.instrument(span))
    }

    fn prepare_with<'e, 'q: 'e>(
        self,
        sql: &'q str,
        parameters: &'e [<Postgres as sqlx::Database>::TypeInfo],
    ) -> BoxFuture<'e, Result<<Postgres as sqlx::Database>::Statement<'q>, sqlx::Error>> {
        Box::pin(async move { sqlx::Executor::prepare_with(&self.0, sql, parameters).await })
    }

    fn describe<'e, 'q: 'e>(
        self,
        sql: &'q str,
    ) -> BoxFuture<'e, Result<sqlx::Describe<Postgres>, sqlx::Error>> {
        Box::pin(async move { sqlx::Executor::describe(&self.0, sql).await })
    }
}

impl std::fmt::Debug for ForgeConn<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ForgeConn::Pool(_) => f.debug_tuple("ForgeConn::Pool").finish(),
            ForgeConn::Tx(_) => f.debug_tuple("ForgeConn::Tx").finish(),
        }
    }
}

impl<'c> sqlx::Executor<'c> for &'c mut ForgeConn<'_> {
    type Database = Postgres;

    fn fetch_many<'e, 'q: 'e, E>(
        self,
        query: E,
    ) -> BoxStream<'e, Result<sqlx::Either<PgQueryResult, PgRow>, sqlx::Error>>
    where
        'c: 'e,
        E: sqlx::Execute<'q, Postgres> + 'q,
    {
        let conn: &'e mut PgConnection = &mut *self;
        conn.fetch_many(query)
    }

    fn fetch_optional<'e, 'q: 'e, E>(
        self,
        query: E,
    ) -> BoxFuture<'e, Result<Option<PgRow>, sqlx::Error>>
    where
        'c: 'e,
        E: sqlx::Execute<'q, Postgres> + 'q,
    {
        let op = sql_operation(query.sql());
        let span =
            tracing::info_span!("db.query", db.system = "postgresql", db.operation.name = op,);
        let conn: &'e mut PgConnection = &mut *self;
        Box::pin(conn.fetch_optional(query).instrument(span))
    }

    fn execute<'e, 'q: 'e, E>(self, query: E) -> BoxFuture<'e, Result<PgQueryResult, sqlx::Error>>
    where
        'c: 'e,
        E: sqlx::Execute<'q, Postgres> + 'q,
    {
        let op = sql_operation(query.sql());
        let span =
            tracing::info_span!("db.query", db.system = "postgresql", db.operation.name = op,);
        let conn: &'e mut PgConnection = &mut *self;
        Box::pin(conn.execute(query).instrument(span))
    }

    fn fetch_all<'e, 'q: 'e, E>(self, query: E) -> BoxFuture<'e, Result<Vec<PgRow>, sqlx::Error>>
    where
        'c: 'e,
        E: sqlx::Execute<'q, Postgres> + 'q,
    {
        let op = sql_operation(query.sql());
        let span =
            tracing::info_span!("db.query", db.system = "postgresql", db.operation.name = op,);
        let conn: &'e mut PgConnection = &mut *self;
        Box::pin(conn.fetch_all(query).instrument(span))
    }

    fn fetch_one<'e, 'q: 'e, E>(self, query: E) -> BoxFuture<'e, Result<PgRow, sqlx::Error>>
    where
        'c: 'e,
        E: sqlx::Execute<'q, Postgres> + 'q,
    {
        let op = sql_operation(query.sql());
        let span =
            tracing::info_span!("db.query", db.system = "postgresql", db.operation.name = op,);
        let conn: &'e mut PgConnection = &mut *self;
        Box::pin(conn.fetch_one(query).instrument(span))
    }

    fn prepare_with<'e, 'q: 'e>(
        self,
        sql: &'q str,
        parameters: &'e [<Postgres as sqlx::Database>::TypeInfo],
    ) -> BoxFuture<'e, Result<<Postgres as sqlx::Database>::Statement<'q>, sqlx::Error>>
    where
        'c: 'e,
    {
        let conn: &'e mut PgConnection = &mut *self;
        conn.prepare_with(sql, parameters)
    }

    fn describe<'e, 'q: 'e>(
        self,
        sql: &'q str,
    ) -> BoxFuture<'e, Result<sqlx::Describe<Postgres>, sqlx::Error>>
    where
        'c: 'e,
    {
        let conn: &'e mut PgConnection = &mut *self;
        conn.describe(sql)
    }
}

#[derive(Debug, Clone)]
pub struct PendingJob {
    pub id: Uuid,
    pub job_type: String,
    pub args: serde_json::Value,
    pub context: serde_json::Value,
    pub owner_subject: Option<String>,
    pub priority: i32,
    pub max_attempts: i32,
    pub worker_capability: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PendingWorkflow {
    pub id: Uuid,
    pub workflow_name: String,
    pub input: serde_json::Value,
    pub owner_subject: Option<String>,
}

#[derive(Default)]
pub struct OutboxBuffer {
    pub jobs: Vec<PendingJob>,
    pub workflows: Vec<PendingWorkflow>,
}

/// Authentication context available to all functions.
#[derive(Debug, Clone)]
pub struct AuthContext {
    /// The authenticated user ID (if any).
    user_id: Option<Uuid>,
    /// User roles.
    roles: Vec<String>,
    /// Custom claims from JWT.
    claims: HashMap<String, serde_json::Value>,
    /// Whether the request is authenticated.
    authenticated: bool,
}

impl AuthContext {
    /// Create an unauthenticated context.
    pub fn unauthenticated() -> Self {
        Self {
            user_id: None,
            roles: Vec::new(),
            claims: HashMap::new(),
            authenticated: false,
        }
    }

    /// Create an authenticated context with a UUID user ID.
    pub fn authenticated(
        user_id: Uuid,
        roles: Vec<String>,
        claims: HashMap<String, serde_json::Value>,
    ) -> Self {
        Self {
            user_id: Some(user_id),
            roles,
            claims,
            authenticated: true,
        }
    }

    /// Create an authenticated context without requiring a UUID user ID.
    ///
    /// Use this for auth providers that don't use UUID subjects (e.g., Firebase,
    /// Clerk). The raw subject string is available via `subject()` method
    /// from the "sub" claim.
    pub fn authenticated_without_uuid(
        roles: Vec<String>,
        claims: HashMap<String, serde_json::Value>,
    ) -> Self {
        Self {
            user_id: None,
            roles,
            claims,
            authenticated: true,
        }
    }

    /// Check if the user is authenticated.
    pub fn is_authenticated(&self) -> bool {
        self.authenticated
    }

    /// Get the user ID if authenticated.
    pub fn user_id(&self) -> Option<Uuid> {
        self.user_id
    }

    /// Get the user ID, returning an error if not authenticated.
    pub fn require_user_id(&self) -> crate::error::Result<Uuid> {
        self.user_id
            .ok_or_else(|| crate::error::ForgeError::Unauthorized("Authentication required".into()))
    }

    /// Check if the user has a specific role.
    pub fn has_role(&self, role: &str) -> bool {
        self.roles.iter().any(|r| r == role)
    }

    /// Require a specific role, returning an error if not present.
    pub fn require_role(&self, role: &str) -> crate::error::Result<()> {
        if self.has_role(role) {
            Ok(())
        } else {
            Err(crate::error::ForgeError::Forbidden(format!(
                "Required role '{}' not present",
                role
            )))
        }
    }

    /// Get a custom claim value.
    pub fn claim(&self, key: &str) -> Option<&serde_json::Value> {
        self.claims.get(key)
    }

    /// Get all custom claims.
    pub fn claims(&self) -> &HashMap<String, serde_json::Value> {
        &self.claims
    }

    /// Get all roles.
    pub fn roles(&self) -> &[String] {
        &self.roles
    }

    /// Get the raw subject claim.
    ///
    /// This works with any provider's subject format (UUID, email, custom ID).
    /// For providers like Firebase or Clerk that don't use UUIDs, use this
    /// instead of `user_id()`.
    pub fn subject(&self) -> Option<&str> {
        self.claims.get("sub").and_then(|v| v.as_str())
    }

    /// Like `require_user_id()` but returns the raw subject string for non-UUID providers.
    pub fn require_subject(&self) -> crate::error::Result<&str> {
        if !self.authenticated {
            return Err(crate::error::ForgeError::Unauthorized(
                "Authentication required".to_string(),
            ));
        }
        self.subject().ok_or_else(|| {
            crate::error::ForgeError::Unauthorized("No subject claim in token".to_string())
        })
    }

    /// Get a stable principal identifier for access control and cache scoping.
    ///
    /// Prefers the raw JWT `sub` claim and falls back to UUID user_id.
    pub fn principal_id(&self) -> Option<String> {
        self.subject()
            .map(ToString::to_string)
            .or_else(|| self.user_id.map(|id| id.to_string()))
    }

    /// Check whether this principal should be treated as privileged admin.
    pub fn is_admin(&self) -> bool {
        self.roles.iter().any(|r| r == "admin")
    }

    /// Validate that identity/tenant-scoped arguments in a function call match
    /// the authenticated principal.
    ///
    /// When `enforce_scope` is true (private functions), at least one scope key
    /// must be present and match. When false, existing scope keys are still
    /// validated but their absence is tolerated.
    pub fn check_identity_args(
        &self,
        function_name: &str,
        args: &serde_json::Value,
        enforce_scope: bool,
    ) -> crate::error::Result<()> {
        use crate::error::ForgeError;

        if self.is_admin() {
            return Ok(());
        }

        if !self.is_authenticated() && !enforce_scope {
            return Ok(());
        }

        let Some(obj) = args.as_object() else {
            if enforce_scope && self.is_authenticated() {
                return Err(ForgeError::Forbidden(format!(
                    "Function '{function_name}' must include identity or tenant scope arguments"
                )));
            }
            return Ok(());
        };

        let mut principal_values: Vec<String> = Vec::new();
        if let Some(user_id) = self.user_id().map(|id| id.to_string()) {
            principal_values.push(user_id);
        }
        if let Some(subject) = self.principal_id()
            && !principal_values.iter().any(|v| v == &subject)
        {
            principal_values.push(subject);
        }

        let mut has_scope_key = false;

        for key in [
            "user_id",
            "userId",
            "owner_id",
            "ownerId",
            "owner_subject",
            "ownerSubject",
            "subject",
            "sub",
            "principal_id",
            "principalId",
        ] {
            let Some(value) = obj.get(key) else {
                continue;
            };
            has_scope_key = true;

            if !self.is_authenticated() {
                return Err(ForgeError::Unauthorized(format!(
                    "Function '{function_name}' requires authentication for identity-scoped argument '{key}'"
                )));
            }

            let serde_json::Value::String(actual) = value else {
                return Err(ForgeError::InvalidArgument(format!(
                    "Function '{function_name}' argument '{key}' must be a non-empty string"
                )));
            };

            if actual.trim().is_empty() || !principal_values.iter().any(|v| v == actual) {
                return Err(ForgeError::Forbidden(format!(
                    "Function '{function_name}' argument '{key}' does not match authenticated principal"
                )));
            }
        }

        for key in ["tenant_id", "tenantId"] {
            let Some(value) = obj.get(key) else {
                continue;
            };
            has_scope_key = true;

            if !self.is_authenticated() {
                return Err(ForgeError::Unauthorized(format!(
                    "Function '{function_name}' requires authentication for tenant-scoped argument '{key}'"
                )));
            }

            let expected = self
                .claim("tenant_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    ForgeError::Forbidden(format!(
                        "Function '{function_name}' argument '{key}' is not allowed for this principal"
                    ))
                })?;

            let serde_json::Value::String(actual) = value else {
                return Err(ForgeError::InvalidArgument(format!(
                    "Function '{function_name}' argument '{key}' must be a non-empty string"
                )));
            };

            if actual.trim().is_empty() || actual != expected {
                return Err(ForgeError::Forbidden(format!(
                    "Function '{function_name}' argument '{key}' does not match authenticated tenant"
                )));
            }
        }

        if enforce_scope && self.is_authenticated() && !has_scope_key {
            return Err(ForgeError::Forbidden(format!(
                "Function '{function_name}' must include identity or tenant scope arguments"
            )));
        }

        Ok(())
    }
}

/// Request metadata available to all functions.
#[derive(Debug, Clone)]
pub struct RequestMetadata {
    /// Unique request ID for tracing.
    pub request_id: Uuid,
    /// Trace ID for distributed tracing.
    pub trace_id: String,
    /// Client IP address.
    pub client_ip: Option<String>,
    /// User agent string.
    pub user_agent: Option<String>,
    /// Request timestamp.
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl RequestMetadata {
    /// Create new request metadata.
    pub fn new() -> Self {
        Self {
            request_id: Uuid::new_v4(),
            trace_id: Uuid::new_v4().to_string(),
            client_ip: None,
            user_agent: None,
            timestamp: chrono::Utc::now(),
        }
    }

    /// Create with a specific trace ID.
    pub fn with_trace_id(trace_id: String) -> Self {
        Self {
            request_id: Uuid::new_v4(),
            trace_id,
            client_ip: None,
            user_agent: None,
            timestamp: chrono::Utc::now(),
        }
    }
}

impl Default for RequestMetadata {
    fn default() -> Self {
        Self::new()
    }
}

/// Context for query functions (read-only database access).
pub struct QueryContext {
    /// Authentication context.
    pub auth: AuthContext,
    /// Request metadata.
    pub request: RequestMetadata,
    /// Database pool for read operations.
    db_pool: sqlx::PgPool,
    /// Environment variable provider.
    env_provider: Arc<dyn EnvProvider>,
}

impl QueryContext {
    /// Create a new query context.
    pub fn new(db_pool: sqlx::PgPool, auth: AuthContext, request: RequestMetadata) -> Self {
        Self {
            auth,
            request,
            db_pool,
            env_provider: Arc::new(RealEnvProvider::new()),
        }
    }

    /// Create a query context with a custom environment provider.
    pub fn with_env(
        db_pool: sqlx::PgPool,
        auth: AuthContext,
        request: RequestMetadata,
        env_provider: Arc<dyn EnvProvider>,
    ) -> Self {
        Self {
            auth,
            request,
            db_pool,
            env_provider,
        }
    }

    /// Database handle with automatic `db.query` tracing spans.
    ///
    /// Works directly with sqlx compile-time checked macros:
    /// ```ignore
    /// sqlx::query_as!(User, "SELECT * FROM users")
    ///     .fetch_all(ctx.db())
    ///     .await?
    /// ```
    pub fn db(&self) -> ForgeDb {
        ForgeDb(self.db_pool.clone())
    }

    pub fn require_user_id(&self) -> crate::error::Result<Uuid> {
        self.auth.require_user_id()
    }

    /// Like `require_user_id()` but for non-UUID auth providers.
    pub fn require_subject(&self) -> crate::error::Result<&str> {
        self.auth.require_subject()
    }
}

impl EnvAccess for QueryContext {
    fn env_provider(&self) -> &dyn EnvProvider {
        self.env_provider.as_ref()
    }
}

/// Callback type for looking up job info by name.
pub type JobInfoLookup = Arc<dyn Fn(&str) -> Option<JobInfo> + Send + Sync>;

/// Token TTL configuration resolved from `[auth]` in forge.toml.
#[derive(Debug, Clone)]
pub struct AuthTokenTtl {
    /// Access token lifetime in seconds (default 3600).
    pub access_token_secs: i64,
    /// Refresh token lifetime in days (default 30).
    pub refresh_token_days: i64,
}

impl Default for AuthTokenTtl {
    fn default() -> Self {
        Self {
            access_token_secs: 3600,
            refresh_token_days: 30,
        }
    }
}

/// Context for mutation functions (transactional database access).
pub struct MutationContext {
    /// Authentication context.
    pub auth: AuthContext,
    /// Request metadata.
    pub request: RequestMetadata,
    /// Database pool for transactional operations.
    db_pool: sqlx::PgPool,
    /// HTTP client with circuit breaker for external requests.
    http_client: CircuitBreakerClient,
    /// Optional job dispatcher for dispatching background jobs.
    job_dispatch: Option<Arc<dyn JobDispatch>>,
    /// Optional workflow dispatcher for starting workflows.
    workflow_dispatch: Option<Arc<dyn WorkflowDispatch>>,
    /// Environment variable provider.
    env_provider: Arc<dyn EnvProvider>,
    /// Transaction handle for transactional mutations.
    tx: Option<Arc<AsyncMutex<Transaction<'static, Postgres>>>>,
    /// Outbox buffer for jobs/workflows dispatched during transaction.
    outbox: Option<Arc<Mutex<OutboxBuffer>>>,
    /// Job info lookup for transactional dispatch.
    job_info_lookup: Option<JobInfoLookup>,
    /// Optional token issuer for signing JWTs (available when HMAC auth is configured).
    token_issuer: Option<Arc<dyn TokenIssuer>>,
    /// Token TTL config from forge.toml.
    token_ttl: AuthTokenTtl,
}

impl MutationContext {
    /// Create a new mutation context.
    pub fn new(db_pool: sqlx::PgPool, auth: AuthContext, request: RequestMetadata) -> Self {
        Self {
            auth,
            request,
            db_pool,
            http_client: CircuitBreakerClient::with_defaults(reqwest::Client::new()),
            job_dispatch: None,
            workflow_dispatch: None,
            env_provider: Arc::new(RealEnvProvider::new()),
            tx: None,
            outbox: None,
            job_info_lookup: None,
            token_issuer: None,
            token_ttl: AuthTokenTtl::default(),
        }
    }

    /// Create a mutation context with dispatch capabilities.
    pub fn with_dispatch(
        db_pool: sqlx::PgPool,
        auth: AuthContext,
        request: RequestMetadata,
        http_client: CircuitBreakerClient,
        job_dispatch: Option<Arc<dyn JobDispatch>>,
        workflow_dispatch: Option<Arc<dyn WorkflowDispatch>>,
    ) -> Self {
        Self {
            auth,
            request,
            db_pool,
            http_client,
            job_dispatch,
            workflow_dispatch,
            env_provider: Arc::new(RealEnvProvider::new()),
            tx: None,
            outbox: None,
            job_info_lookup: None,
            token_issuer: None,
            token_ttl: AuthTokenTtl::default(),
        }
    }

    /// Create a mutation context with a custom environment provider.
    pub fn with_env(
        db_pool: sqlx::PgPool,
        auth: AuthContext,
        request: RequestMetadata,
        http_client: CircuitBreakerClient,
        job_dispatch: Option<Arc<dyn JobDispatch>>,
        workflow_dispatch: Option<Arc<dyn WorkflowDispatch>>,
        env_provider: Arc<dyn EnvProvider>,
    ) -> Self {
        Self {
            auth,
            request,
            db_pool,
            http_client,
            job_dispatch,
            workflow_dispatch,
            env_provider,
            tx: None,
            outbox: None,
            job_info_lookup: None,
            token_issuer: None,
            token_ttl: AuthTokenTtl::default(),
        }
    }

    /// Returns handles to transaction and outbox for the caller to commit/flush.
    #[allow(clippy::type_complexity)]
    pub fn with_transaction(
        db_pool: sqlx::PgPool,
        tx: Transaction<'static, Postgres>,
        auth: AuthContext,
        request: RequestMetadata,
        http_client: CircuitBreakerClient,
        job_info_lookup: JobInfoLookup,
    ) -> (
        Self,
        Arc<AsyncMutex<Transaction<'static, Postgres>>>,
        Arc<Mutex<OutboxBuffer>>,
    ) {
        let tx_handle = Arc::new(AsyncMutex::new(tx));
        let outbox = Arc::new(Mutex::new(OutboxBuffer::default()));

        let ctx = Self {
            auth,
            request,
            db_pool,
            http_client,
            job_dispatch: None,
            workflow_dispatch: None,
            env_provider: Arc::new(RealEnvProvider::new()),
            tx: Some(tx_handle.clone()),
            outbox: Some(outbox.clone()),
            job_info_lookup: Some(job_info_lookup),
            token_issuer: None,
            token_ttl: AuthTokenTtl::default(),
        };

        (ctx, tx_handle, outbox)
    }

    pub fn is_transactional(&self) -> bool {
        self.tx.is_some()
    }

    /// Acquire a connection compatible with sqlx compile-time checked macros.
    ///
    /// In transactional mode, returns a guard over the active transaction.
    /// Otherwise acquires a fresh connection from the pool.
    ///
    /// ```ignore
    /// let mut conn = ctx.conn().await?;
    /// sqlx::query_as!(User, "INSERT INTO users ... RETURNING *", ...)
    ///     .fetch_one(&mut *conn)
    ///     .await?
    /// ```
    pub async fn conn(&self) -> sqlx::Result<ForgeConn<'_>> {
        match &self.tx {
            Some(tx) => Ok(ForgeConn::Tx(tx.lock().await)),
            None => Ok(ForgeConn::Pool(self.db_pool.acquire().await?)),
        }
    }

    /// Direct pool access for operations that cannot run inside a transaction.
    pub fn pool(&self) -> &sqlx::PgPool {
        &self.db_pool
    }

    /// Get the HTTP client for external requests.
    ///
    /// The client includes circuit breaker protection that tracks failure rates
    /// per host. After repeated failures, requests fail fast to prevent cascade
    /// failures when downstream services are unhealthy.
    pub fn http(&self) -> &reqwest::Client {
        self.http_client.inner()
    }

    /// Get the circuit breaker client directly for advanced usage.
    pub fn http_with_circuit_breaker(&self) -> &CircuitBreakerClient {
        &self.http_client
    }

    pub fn require_user_id(&self) -> crate::error::Result<Uuid> {
        self.auth.require_user_id()
    }

    pub fn require_subject(&self) -> crate::error::Result<&str> {
        self.auth.require_subject()
    }

    /// Set the token issuer for this context.
    pub fn set_token_issuer(&mut self, issuer: Arc<dyn TokenIssuer>) {
        self.token_issuer = Some(issuer);
    }

    /// Set the token TTL configuration (from forge.toml `[auth]`).
    pub fn set_token_ttl(&mut self, ttl: AuthTokenTtl) {
        self.token_ttl = ttl;
    }

    /// Issue a signed JWT from the given claims.
    ///
    /// Only available when HMAC auth is configured in `forge.toml`.
    /// Returns an error if auth is not configured or uses an external provider (RSA/JWKS).
    ///
    /// ```ignore
    /// let claims = Claims::builder()
    ///     .user_id(user.id)
    ///     .duration_secs(7 * 24 * 3600)
    ///     .build()
    ///     .map_err(|e| ForgeError::Internal(e))?;
    ///
    /// let token = ctx.issue_token(&claims)?;
    /// ```
    pub fn issue_token(&self, claims: &Claims) -> crate::error::Result<String> {
        let issuer = self.token_issuer.as_ref().ok_or_else(|| {
            crate::error::ForgeError::Internal(
                "Token issuer not available. Configure [auth] with an HMAC algorithm in forge.toml"
                    .into(),
            )
        })?;
        issuer.sign(claims)
    }

    /// Issue an access + refresh token pair for the given user.
    ///
    /// Stores the refresh token hash in `forge_refresh_tokens` and returns
    /// both tokens. Use `rotate_refresh_token()` to exchange a refresh token
    /// for a new pair, and `revoke_refresh_token()` to invalidate one.
    ///
    /// TTLs come from `[auth]` in forge.toml:
    /// - `access_token_ttl` (default "1h")
    /// - `refresh_token_ttl` (default "30d")
    pub async fn issue_token_pair(
        &self,
        user_id: Uuid,
        roles: &[&str],
    ) -> crate::error::Result<crate::auth::TokenPair> {
        let issuer = self.token_issuer.clone().ok_or_else(|| {
            crate::error::ForgeError::Internal(
                "Token issuer not available. Configure [auth] in forge.toml".into(),
            )
        })?;
        let access_ttl = self.token_ttl.access_token_secs;
        let refresh_ttl = self.token_ttl.refresh_token_days;
        crate::auth::tokens::issue_token_pair(
            &self.db_pool,
            user_id,
            roles,
            access_ttl,
            refresh_ttl,
            move |uid, r, ttl| {
                let claims = Claims::builder()
                    .subject(uid)
                    .roles(r.iter().map(|s| s.to_string()).collect())
                    .duration_secs(ttl)
                    .build()
                    .map_err(crate::error::ForgeError::Internal)?;
                issuer.sign(&claims)
            },
        )
        .await
    }

    /// Rotate a refresh token: validate the old one, issue a new pair.
    ///
    /// The old token is atomically deleted and a new access + refresh pair
    /// is returned. Fails if the token is invalid or expired.
    pub async fn rotate_refresh_token(
        &self,
        old_refresh_token: &str,
    ) -> crate::error::Result<crate::auth::TokenPair> {
        let issuer = self.token_issuer.clone().ok_or_else(|| {
            crate::error::ForgeError::Internal(
                "Token issuer not available. Configure [auth] in forge.toml".into(),
            )
        })?;
        let access_ttl = self.token_ttl.access_token_secs;
        let refresh_ttl = self.token_ttl.refresh_token_days;
        crate::auth::tokens::rotate_refresh_token(
            &self.db_pool,
            old_refresh_token,
            &["user"],
            access_ttl,
            refresh_ttl,
            move |uid, r, ttl| {
                let claims = Claims::builder()
                    .subject(uid)
                    .roles(r.iter().map(|s| s.to_string()).collect())
                    .duration_secs(ttl)
                    .build()
                    .map_err(crate::error::ForgeError::Internal)?;
                issuer.sign(&claims)
            },
        )
        .await
    }

    /// Revoke a specific refresh token (e.g., on logout).
    pub async fn revoke_refresh_token(
        &self,
        refresh_token: &str,
    ) -> crate::error::Result<()> {
        crate::auth::tokens::revoke_refresh_token(&self.db_pool, refresh_token).await
    }

    /// Revoke all refresh tokens for a user (e.g., on password change or account deletion).
    pub async fn revoke_all_refresh_tokens(
        &self,
        user_id: Uuid,
    ) -> crate::error::Result<()> {
        crate::auth::tokens::revoke_all_refresh_tokens(&self.db_pool, user_id).await
    }

    /// In transactional mode, buffers for atomic commit; otherwise dispatches immediately.
    pub async fn dispatch_job<T: serde::Serialize>(
        &self,
        job_type: &str,
        args: T,
    ) -> crate::error::Result<Uuid> {
        let args_json = serde_json::to_value(args)?;

        // Transactional mode: buffer the job for atomic commit
        if let (Some(outbox), Some(job_info_lookup)) = (&self.outbox, &self.job_info_lookup) {
            let job_info = job_info_lookup(job_type).ok_or_else(|| {
                crate::error::ForgeError::NotFound(format!("Job type '{}' not found", job_type))
            })?;

            let pending = PendingJob {
                id: Uuid::new_v4(),
                job_type: job_type.to_string(),
                args: args_json,
                context: serde_json::json!({}),
                owner_subject: self.auth.principal_id(),
                priority: job_info.priority.as_i32(),
                max_attempts: job_info.retry.max_attempts as i32,
                worker_capability: job_info.worker_capability.map(|s| s.to_string()),
            };

            let job_id = pending.id;
            outbox
                .lock()
                .expect("outbox lock poisoned")
                .jobs
                .push(pending);
            return Ok(job_id);
        }

        // Non-transactional mode: dispatch immediately
        let dispatcher = self.job_dispatch.as_ref().ok_or_else(|| {
            crate::error::ForgeError::Internal("Job dispatch not available".into())
        })?;
        dispatcher
            .dispatch_by_name(job_type, args_json, self.auth.principal_id())
            .await
    }

    /// Dispatch a job with initial context.
    pub async fn dispatch_job_with_context<T: serde::Serialize>(
        &self,
        job_type: &str,
        args: T,
        context: serde_json::Value,
    ) -> crate::error::Result<Uuid> {
        let args_json = serde_json::to_value(args)?;

        if let (Some(outbox), Some(job_info_lookup)) = (&self.outbox, &self.job_info_lookup) {
            let job_info = job_info_lookup(job_type).ok_or_else(|| {
                crate::error::ForgeError::NotFound(format!("Job type '{}' not found", job_type))
            })?;

            let pending = PendingJob {
                id: Uuid::new_v4(),
                job_type: job_type.to_string(),
                args: args_json,
                context,
                owner_subject: self.auth.principal_id(),
                priority: job_info.priority.as_i32(),
                max_attempts: job_info.retry.max_attempts as i32,
                worker_capability: job_info.worker_capability.map(|s| s.to_string()),
            };

            let job_id = pending.id;
            outbox
                .lock()
                .expect("outbox lock poisoned")
                .jobs
                .push(pending);
            return Ok(job_id);
        }

        let dispatcher = self.job_dispatch.as_ref().ok_or_else(|| {
            crate::error::ForgeError::Internal("Job dispatch not available".into())
        })?;
        dispatcher
            .dispatch_by_name(job_type, args_json, self.auth.principal_id())
            .await
    }

    /// Request cancellation for a job.
    pub async fn cancel_job(
        &self,
        job_id: Uuid,
        reason: Option<String>,
    ) -> crate::error::Result<bool> {
        let dispatcher = self.job_dispatch.as_ref().ok_or_else(|| {
            crate::error::ForgeError::Internal("Job dispatch not available".into())
        })?;
        dispatcher.cancel(job_id, reason).await
    }

    /// In transactional mode, buffers for atomic commit; otherwise starts immediately.
    pub async fn start_workflow<T: serde::Serialize>(
        &self,
        workflow_name: &str,
        input: T,
    ) -> crate::error::Result<Uuid> {
        let input_json = serde_json::to_value(input)?;

        // Transactional mode: buffer the workflow for atomic commit
        if let Some(outbox) = &self.outbox {
            let pending = PendingWorkflow {
                id: Uuid::new_v4(),
                workflow_name: workflow_name.to_string(),
                input: input_json,
                owner_subject: self.auth.principal_id(),
            };

            let workflow_id = pending.id;
            outbox
                .lock()
                .expect("outbox lock poisoned")
                .workflows
                .push(pending);
            return Ok(workflow_id);
        }

        // Non-transactional mode: start immediately
        let dispatcher = self.workflow_dispatch.as_ref().ok_or_else(|| {
            crate::error::ForgeError::Internal("Workflow dispatch not available".into())
        })?;
        dispatcher
            .start_by_name(workflow_name, input_json, self.auth.principal_id())
            .await
    }
}

impl EnvAccess for MutationContext {
    fn env_provider(&self) -> &dyn EnvProvider {
        self.env_provider.as_ref()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_context_unauthenticated() {
        let ctx = AuthContext::unauthenticated();
        assert!(!ctx.is_authenticated());
        assert!(ctx.user_id().is_none());
        assert!(ctx.require_user_id().is_err());
    }

    #[test]
    fn test_auth_context_authenticated() {
        let user_id = Uuid::new_v4();
        let ctx = AuthContext::authenticated(
            user_id,
            vec!["admin".to_string(), "user".to_string()],
            HashMap::new(),
        );

        assert!(ctx.is_authenticated());
        assert_eq!(ctx.user_id(), Some(user_id));
        assert!(ctx.require_user_id().is_ok());
        assert!(ctx.has_role("admin"));
        assert!(ctx.has_role("user"));
        assert!(!ctx.has_role("superadmin"));
        assert!(ctx.require_role("admin").is_ok());
        assert!(ctx.require_role("superadmin").is_err());
    }

    #[test]
    fn test_auth_context_with_claims() {
        let mut claims = HashMap::new();
        claims.insert("org_id".to_string(), serde_json::json!("org-123"));

        let ctx = AuthContext::authenticated(Uuid::new_v4(), vec![], claims);

        assert_eq!(ctx.claim("org_id"), Some(&serde_json::json!("org-123")));
        assert!(ctx.claim("nonexistent").is_none());
    }

    #[test]
    fn test_request_metadata() {
        let meta = RequestMetadata::new();
        assert!(!meta.trace_id.is_empty());
        assert!(meta.client_ip.is_none());

        let meta2 = RequestMetadata::with_trace_id("trace-123".to_string());
        assert_eq!(meta2.trace_id, "trace-123");
    }
}
