//! Test context for MCP tool functions.

#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use std::collections::HashMap;
use std::sync::Arc;

use sqlx::PgPool;
use uuid::Uuid;

use super::super::mock_dispatch::{MockJobDispatch, MockWorkflowDispatch};
use super::super::mock_http::{MockHttp, MockRequest, MockResponse};
use super::build_test_auth;
use crate::Result;
use crate::env::{EnvAccess, EnvProvider, MockEnvProvider};
use crate::function::{AuthContext, RequestMetadata};

/// Test context for MCP tool functions.
///
/// Provides an isolated testing environment for MCP tools with configurable
/// authentication, mock HTTP, and mock job/workflow dispatch.
///
/// # Example
///
/// ```ignore
/// let ctx = TestMcpToolContext::builder()
///     .as_user(Uuid::new_v4())
///     .with_role("admin")
///     .build();
///
/// // Dispatch a job
/// ctx.dispatch_job("process_event", payload).await?;
///
/// // Verify job was dispatched
/// ctx.job_dispatch().assert_dispatched("process_event");
/// ```
pub struct TestMcpToolContext {
    /// Authentication context.
    pub auth: AuthContext,
    /// Request metadata.
    pub request: RequestMetadata,
    /// Optional database pool for integration tests.
    pool: Option<PgPool>,
    /// Tenant ID for multi-tenant testing.
    tenant_id: Option<Uuid>,
    /// Mock HTTP client.
    http: Arc<MockHttp>,
    /// Mock job dispatch for verification.
    job_dispatch: Arc<MockJobDispatch>,
    /// Mock workflow dispatch for verification.
    workflow_dispatch: Arc<MockWorkflowDispatch>,
    /// Mock environment provider.
    env_provider: Arc<MockEnvProvider>,
}

impl TestMcpToolContext {
    /// Create a new builder.
    pub fn builder() -> TestMcpToolContextBuilder {
        TestMcpToolContextBuilder::default()
    }

    /// Create a minimal unauthenticated context (no database).
    pub fn minimal() -> Self {
        Self::builder().build()
    }

    /// Create an authenticated context with the given user ID (no database).
    pub fn authenticated(user_id: Uuid) -> Self {
        Self::builder().as_user(user_id).build()
    }

    /// Get the database pool (if available).
    pub fn db(&self) -> Option<&PgPool> {
        self.pool.as_ref()
    }

    /// Get the mock HTTP client.
    pub fn http(&self) -> &MockHttp {
        &self.http
    }

    /// Get the authenticated user's UUID. Returns 401 if not authenticated.
    pub fn user_id(&self) -> Result<Uuid> {
        self.auth.require_user_id()
    }

    /// Check if a specific role is present.
    pub fn has_role(&self, role: &str) -> bool {
        self.auth.has_role(role)
    }

    /// Get a claim value.
    pub fn claim(&self, key: &str) -> Option<&serde_json::Value> {
        self.auth.claim(key)
    }

    /// Get the tenant ID (if set).
    pub fn tenant_id(&self) -> Option<Uuid> {
        self.tenant_id
    }

    /// Get the mock job dispatch for verification.
    pub fn job_dispatch(&self) -> &MockJobDispatch {
        &self.job_dispatch
    }

    /// Get the mock workflow dispatch for verification.
    pub fn workflow_dispatch(&self) -> &MockWorkflowDispatch {
        &self.workflow_dispatch
    }

    /// Dispatch a job (records for later verification).
    pub async fn dispatch_job<T: serde::Serialize>(&self, job_type: &str, args: T) -> Result<Uuid> {
        self.job_dispatch.dispatch(job_type, args).await
    }

    /// Start a workflow (records for later verification).
    pub async fn start_workflow<T: serde::Serialize>(
        &self,
        workflow_name: &str,
        input: T,
    ) -> Result<Uuid> {
        self.workflow_dispatch.start(workflow_name, input).await
    }

    /// Get the mock env provider for verification.
    pub fn env_mock(&self) -> &MockEnvProvider {
        &self.env_provider
    }
}

impl EnvAccess for TestMcpToolContext {
    fn env_provider(&self) -> &dyn EnvProvider {
        self.env_provider.as_ref()
    }
}

/// Builder for TestMcpToolContext.
#[derive(Default)]
pub struct TestMcpToolContextBuilder {
    user_id: Option<Uuid>,
    roles: Vec<String>,
    claims: HashMap<String, serde_json::Value>,
    tenant_id: Option<Uuid>,
    pool: Option<PgPool>,
    http: MockHttp,
    job_dispatch: Option<Arc<MockJobDispatch>>,
    workflow_dispatch: Option<Arc<MockWorkflowDispatch>>,
    env_vars: HashMap<String, String>,
}

impl TestMcpToolContextBuilder {
    /// Set the authenticated user with a UUID.
    pub fn as_user(mut self, id: Uuid) -> Self {
        self.user_id = Some(id);
        self
    }

    /// For non-UUID auth providers (Firebase, Clerk, etc.).
    pub fn as_subject(mut self, subject: impl Into<String>) -> Self {
        self.claims
            .insert("sub".to_string(), serde_json::json!(subject.into()));
        self
    }

    /// Add a role.
    pub fn with_role(mut self, role: impl Into<String>) -> Self {
        self.roles.push(role.into());
        self
    }

    /// Add multiple roles.
    pub fn with_roles(mut self, roles: Vec<String>) -> Self {
        self.roles.extend(roles);
        self
    }

    /// Add a custom claim.
    pub fn with_claim(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.claims.insert(key.into(), value);
        self
    }

    /// Set the tenant ID for multi-tenant testing.
    pub fn with_tenant(mut self, tenant_id: Uuid) -> Self {
        self.tenant_id = Some(tenant_id);
        self
    }

    /// Set the database pool.
    pub fn with_pool(mut self, pool: PgPool) -> Self {
        self.pool = Some(pool);
        self
    }

    /// Add an HTTP mock with a custom handler.
    pub fn mock_http<F>(self, pattern: &str, handler: F) -> Self
    where
        F: Fn(&MockRequest) -> MockResponse + Send + Sync + 'static,
    {
        self.http.add_mock_sync(pattern, handler);
        self
    }

    /// Add an HTTP mock that returns a JSON response.
    pub fn mock_http_json<T: serde::Serialize>(self, pattern: &str, response: T) -> Self {
        let json = serde_json::to_value(response).unwrap_or(serde_json::Value::Null);
        self.mock_http(pattern, move |_| MockResponse::json(json.clone()))
    }

    /// Use a specific mock job dispatch.
    pub fn with_job_dispatch(mut self, dispatch: Arc<MockJobDispatch>) -> Self {
        self.job_dispatch = Some(dispatch);
        self
    }

    /// Use a specific mock workflow dispatch.
    pub fn with_workflow_dispatch(mut self, dispatch: Arc<MockWorkflowDispatch>) -> Self {
        self.workflow_dispatch = Some(dispatch);
        self
    }

    /// Set a single environment variable.
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env_vars.insert(key.into(), value.into());
        self
    }

    /// Set multiple environment variables.
    pub fn with_envs(mut self, vars: HashMap<String, String>) -> Self {
        self.env_vars.extend(vars);
        self
    }

    /// Build the test context.
    pub fn build(self) -> TestMcpToolContext {
        TestMcpToolContext {
            auth: build_test_auth(self.user_id, self.roles, self.claims),
            request: RequestMetadata::default(),
            pool: self.pool,
            tenant_id: self.tenant_id,
            http: Arc::new(self.http),
            job_dispatch: self
                .job_dispatch
                .unwrap_or_else(|| Arc::new(MockJobDispatch::new())),
            workflow_dispatch: self
                .workflow_dispatch
                .unwrap_or_else(|| Arc::new(MockWorkflowDispatch::new())),
            env_provider: Arc::new(MockEnvProvider::with_vars(self.env_vars)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_minimal_context() {
        let ctx = TestMcpToolContext::minimal();
        assert!(!ctx.auth.is_authenticated());
        assert!(ctx.db().is_none());
    }

    #[test]
    fn test_authenticated_context() {
        let user_id = Uuid::new_v4();
        let ctx = TestMcpToolContext::authenticated(user_id);
        assert!(ctx.auth.is_authenticated());
        assert_eq!(ctx.user_id().unwrap(), user_id);
    }

    #[test]
    fn test_context_with_roles() {
        let ctx = TestMcpToolContext::builder()
            .as_user(Uuid::new_v4())
            .with_role("admin")
            .with_role("user")
            .build();

        assert!(ctx.has_role("admin"));
        assert!(ctx.has_role("user"));
        assert!(!ctx.has_role("superuser"));
    }

    #[tokio::test]
    async fn test_dispatch_job() {
        let ctx = TestMcpToolContext::builder()
            .as_user(Uuid::new_v4())
            .build();

        let job_id = ctx
            .dispatch_job("process_event", serde_json::json!({"action": "push"}))
            .await
            .unwrap();

        assert!(!job_id.is_nil());
        ctx.job_dispatch().assert_dispatched("process_event");
    }

    #[tokio::test]
    async fn test_start_workflow() {
        let ctx = TestMcpToolContext::builder()
            .as_user(Uuid::new_v4())
            .build();

        let run_id = ctx
            .start_workflow("onboarding", serde_json::json!({"user_id": "u123"}))
            .await
            .unwrap();

        assert!(!run_id.is_nil());
        ctx.workflow_dispatch().assert_started("onboarding");
    }

    #[test]
    fn test_context_with_env() {
        let ctx = TestMcpToolContext::builder()
            .with_env("API_KEY", "test_key_123")
            .with_env("TIMEOUT", "30")
            .build();

        assert_eq!(ctx.env("API_KEY"), Some("test_key_123".to_string()));
        assert_eq!(ctx.env_or("TIMEOUT", "10"), "30");
        assert_eq!(ctx.env_or("MISSING", "default"), "default");

        ctx.env_mock().assert_accessed("API_KEY");
        ctx.env_mock().assert_accessed("TIMEOUT");
    }
}
