use std::sync::Arc;

use crate::Result;
use crate::env::{EnvAccess, EnvProvider, RealEnvProvider};
use crate::function::{AuthContext, JobDispatch, RequestMetadata, WorkflowDispatch};
use uuid::Uuid;

/// Context for MCP tool execution.
pub struct McpToolContext {
    /// Authentication context.
    pub auth: AuthContext,
    /// Request metadata.
    pub request: RequestMetadata,
    db_pool: sqlx::PgPool,
    job_dispatch: Option<Arc<dyn JobDispatch>>,
    workflow_dispatch: Option<Arc<dyn WorkflowDispatch>>,
    env_provider: Arc<dyn EnvProvider>,
}

impl McpToolContext {
    /// Create a new MCP tool context.
    pub fn new(db_pool: sqlx::PgPool, auth: AuthContext, request: RequestMetadata) -> Self {
        Self::with_dispatch(db_pool, auth, request, None, None)
    }

    /// Create a context with dispatch capabilities.
    pub fn with_dispatch(
        db_pool: sqlx::PgPool,
        auth: AuthContext,
        request: RequestMetadata,
        job_dispatch: Option<Arc<dyn JobDispatch>>,
        workflow_dispatch: Option<Arc<dyn WorkflowDispatch>>,
    ) -> Self {
        Self::with_env(
            db_pool,
            auth,
            request,
            job_dispatch,
            workflow_dispatch,
            Arc::new(RealEnvProvider::new()),
        )
    }

    /// Create a context with a custom environment provider.
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

    pub fn db(&self) -> &sqlx::PgPool {
        &self.db_pool
    }

    /// Returns a `DbConn` wrapping the pool, allowing shared helper functions
    /// that accept `DbConn` to work across all context types.
    pub fn db_conn(&self) -> crate::function::DbConn<'_> {
        crate::function::DbConn::Pool(&self.db_pool)
    }

    /// Acquire a connection compatible with sqlx compile-time checked macros.
    pub async fn conn(&self) -> sqlx::Result<crate::function::ForgeConn<'static>> {
        Ok(crate::function::ForgeConn::Pool(
            self.db_pool.acquire().await?,
        ))
    }

    pub fn require_user_id(&self) -> Result<Uuid> {
        self.auth.require_user_id()
    }

    pub fn require_subject(&self) -> Result<&str> {
        self.auth.require_subject()
    }

    /// Dispatch a background job.
    pub async fn dispatch_job<T: serde::Serialize>(&self, job_type: &str, args: T) -> Result<Uuid> {
        let dispatcher = self.job_dispatch.as_ref().ok_or_else(|| {
            crate::error::ForgeError::Internal("Job dispatch not available".to_string())
        })?;

        let args_json = serde_json::to_value(args)?;
        dispatcher
            .dispatch_by_name(job_type, args_json, self.auth.principal_id())
            .await
    }

    /// Start a workflow.
    pub async fn start_workflow<T: serde::Serialize>(
        &self,
        workflow_name: &str,
        input: T,
    ) -> Result<Uuid> {
        let dispatcher = self.workflow_dispatch.as_ref().ok_or_else(|| {
            crate::error::ForgeError::Internal("Workflow dispatch not available".to_string())
        })?;

        let input_json = serde_json::to_value(input)?;
        dispatcher
            .start_by_name(workflow_name, input_json, self.auth.principal_id())
            .await
    }
}

impl EnvAccess for McpToolContext {
    fn env_provider(&self) -> &dyn EnvProvider {
        self.env_provider.as_ref()
    }
}
