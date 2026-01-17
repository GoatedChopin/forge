use std::collections::HashMap;
use std::sync::Arc;

use uuid::Uuid;

use super::dispatch::{JobDispatch, WorkflowDispatch};
use crate::env::{EnvAccess, EnvProvider, RealEnvProvider};

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

    /// Get a reference to the database pool.
    pub fn db(&self) -> &sqlx::PgPool {
        &self.db_pool
    }

    /// Get the authenticated user ID or return an error.
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

/// Context for mutation functions (transactional database access).
pub struct MutationContext {
    /// Authentication context.
    pub auth: AuthContext,
    /// Request metadata.
    pub request: RequestMetadata,
    /// Database pool for transactional operations.
    db_pool: sqlx::PgPool,
    /// Optional job dispatcher for dispatching background jobs.
    job_dispatch: Option<Arc<dyn JobDispatch>>,
    /// Optional workflow dispatcher for starting workflows.
    workflow_dispatch: Option<Arc<dyn WorkflowDispatch>>,
    /// Environment variable provider.
    env_provider: Arc<dyn EnvProvider>,
}

impl MutationContext {
    /// Create a new mutation context.
    pub fn new(db_pool: sqlx::PgPool, auth: AuthContext, request: RequestMetadata) -> Self {
        Self {
            auth,
            request,
            db_pool,
            job_dispatch: None,
            workflow_dispatch: None,
            env_provider: Arc::new(RealEnvProvider::new()),
        }
    }

    /// Create a mutation context with dispatch capabilities.
    pub fn with_dispatch(
        db_pool: sqlx::PgPool,
        auth: AuthContext,
        request: RequestMetadata,
        job_dispatch: Option<Arc<dyn JobDispatch>>,
        workflow_dispatch: Option<Arc<dyn WorkflowDispatch>>,
    ) -> Self {
        Self {
            auth,
            request,
            db_pool,
            job_dispatch,
            workflow_dispatch,
            env_provider: Arc::new(RealEnvProvider::new()),
        }
    }

    /// Create a mutation context with a custom environment provider.
    pub fn with_env(
        db_pool: sqlx::PgPool,
        auth: AuthContext,
        request: RequestMetadata,
        job_dispatch: Option<Arc<dyn JobDispatch>>,
        workflow_dispatch: Option<Arc<dyn WorkflowDispatch>>,
        env_provider: Arc<dyn EnvProvider>,
    ) -> Self {
        Self {
            auth,
            request,
            db_pool,
            job_dispatch,
            workflow_dispatch,
            env_provider,
        }
    }

    /// Get a reference to the database pool.
    pub fn db(&self) -> &sqlx::PgPool {
        &self.db_pool
    }

    /// Get the authenticated user ID or return an error.
    pub fn require_user_id(&self) -> crate::error::Result<Uuid> {
        self.auth.require_user_id()
    }

    /// Like `require_user_id()` but for non-UUID auth providers.
    pub fn require_subject(&self) -> crate::error::Result<&str> {
        self.auth.require_subject()
    }

    /// Dispatch a background job.
    ///
    /// # Arguments
    /// * `job_type` - The registered name of the job type
    /// * `args` - The arguments for the job (will be serialized to JSON)
    ///
    /// # Returns
    /// The UUID of the dispatched job, or an error if dispatch is not available.
    pub async fn dispatch_job<T: serde::Serialize>(
        &self,
        job_type: &str,
        args: T,
    ) -> crate::error::Result<Uuid> {
        let dispatcher = self.job_dispatch.as_ref().ok_or_else(|| {
            crate::error::ForgeError::Internal("Job dispatch not available".into())
        })?;
        let args_json = serde_json::to_value(args)?;
        dispatcher.dispatch_by_name(job_type, args_json).await
    }

    /// Start a workflow.
    ///
    /// # Arguments
    /// * `workflow_name` - The registered name of the workflow
    /// * `input` - The input for the workflow (will be serialized to JSON)
    ///
    /// # Returns
    /// The UUID of the started workflow run, or an error if dispatch is not available.
    pub async fn start_workflow<T: serde::Serialize>(
        &self,
        workflow_name: &str,
        input: T,
    ) -> crate::error::Result<Uuid> {
        let dispatcher = self.workflow_dispatch.as_ref().ok_or_else(|| {
            crate::error::ForgeError::Internal("Workflow dispatch not available".into())
        })?;
        let input_json = serde_json::to_value(input)?;
        dispatcher.start_by_name(workflow_name, input_json).await
    }
}

impl EnvAccess for MutationContext {
    fn env_provider(&self) -> &dyn EnvProvider {
        self.env_provider.as_ref()
    }
}

/// Context for action functions (can call external APIs).
pub struct ActionContext {
    /// Authentication context.
    pub auth: AuthContext,
    /// Request metadata.
    pub request: RequestMetadata,
    /// Database pool for database operations.
    db_pool: sqlx::PgPool,
    /// HTTP client for external requests.
    http_client: reqwest::Client,
    /// Optional job dispatcher for dispatching background jobs.
    job_dispatch: Option<Arc<dyn JobDispatch>>,
    /// Optional workflow dispatcher for starting workflows.
    workflow_dispatch: Option<Arc<dyn WorkflowDispatch>>,
    /// Environment variable provider.
    env_provider: Arc<dyn EnvProvider>,
}

impl ActionContext {
    /// Create a new action context.
    pub fn new(
        db_pool: sqlx::PgPool,
        auth: AuthContext,
        request: RequestMetadata,
        http_client: reqwest::Client,
    ) -> Self {
        Self {
            auth,
            request,
            db_pool,
            http_client,
            job_dispatch: None,
            workflow_dispatch: None,
            env_provider: Arc::new(RealEnvProvider::new()),
        }
    }

    /// Create an action context with dispatch capabilities.
    pub fn with_dispatch(
        db_pool: sqlx::PgPool,
        auth: AuthContext,
        request: RequestMetadata,
        http_client: reqwest::Client,
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
        }
    }

    /// Create an action context with a custom environment provider.
    pub fn with_env(
        db_pool: sqlx::PgPool,
        auth: AuthContext,
        request: RequestMetadata,
        http_client: reqwest::Client,
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
        }
    }

    /// Get a reference to the database pool.
    pub fn db(&self) -> &sqlx::PgPool {
        &self.db_pool
    }

    /// Get a reference to the HTTP client.
    pub fn http(&self) -> &reqwest::Client {
        &self.http_client
    }

    /// Get the authenticated user ID or return an error.
    pub fn require_user_id(&self) -> crate::error::Result<Uuid> {
        self.auth.require_user_id()
    }

    /// Like `require_user_id()` but for non-UUID auth providers.
    pub fn require_subject(&self) -> crate::error::Result<&str> {
        self.auth.require_subject()
    }

    /// Dispatch a background job.
    ///
    /// # Arguments
    /// * `job_type` - The registered name of the job type
    /// * `args` - The arguments for the job (will be serialized to JSON)
    ///
    /// # Returns
    /// The UUID of the dispatched job, or an error if dispatch is not available.
    pub async fn dispatch_job<T: serde::Serialize>(
        &self,
        job_type: &str,
        args: T,
    ) -> crate::error::Result<Uuid> {
        let dispatcher = self.job_dispatch.as_ref().ok_or_else(|| {
            crate::error::ForgeError::Internal("Job dispatch not available".into())
        })?;
        let args_json = serde_json::to_value(args)?;
        dispatcher.dispatch_by_name(job_type, args_json).await
    }

    /// Start a workflow.
    ///
    /// # Arguments
    /// * `workflow_name` - The registered name of the workflow
    /// * `input` - The input for the workflow (will be serialized to JSON)
    ///
    /// # Returns
    /// The UUID of the started workflow run, or an error if dispatch is not available.
    pub async fn start_workflow<T: serde::Serialize>(
        &self,
        workflow_name: &str,
        input: T,
    ) -> crate::error::Result<Uuid> {
        let dispatcher = self.workflow_dispatch.as_ref().ok_or_else(|| {
            crate::error::ForgeError::Internal("Workflow dispatch not available".into())
        })?;
        let input_json = serde_json::to_value(input)?;
        dispatcher.start_by_name(workflow_name, input_json).await
    }
}

impl EnvAccess for ActionContext {
    fn env_provider(&self) -> &dyn EnvProvider {
        self.env_provider.as_ref()
    }
}

#[cfg(test)]
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
