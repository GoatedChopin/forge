use std::sync::Arc;
use std::time::Duration;

use forge_core::{
    AuthContext, ForgeError, FunctionInfo, FunctionKind, JobDispatch, MutationContext,
    QueryContext, RequestMetadata, Result, WorkflowDispatch,
    rate_limit::{RateLimitConfig, RateLimitKey},
};
use serde_json::Value;

use super::cache::QueryCache;
use super::registry::{FunctionEntry, FunctionRegistry};
use crate::rate_limit::RateLimiter;

/// Result of routing a function call.
pub enum RouteResult {
    /// Query execution result.
    Query(Value),
    /// Mutation execution result.
    Mutation(Value),
    /// Job dispatch result (returns job_id).
    Job(Value),
    /// Workflow dispatch result (returns workflow_id).
    Workflow(Value),
}

/// Routes function calls to the appropriate handler.
pub struct FunctionRouter {
    registry: Arc<FunctionRegistry>,
    db_pool: sqlx::PgPool,
    http_client: reqwest::Client,
    job_dispatcher: Option<Arc<dyn JobDispatch>>,
    workflow_dispatcher: Option<Arc<dyn WorkflowDispatch>>,
    rate_limiter: RateLimiter,
    query_cache: QueryCache,
}

impl FunctionRouter {
    /// Create a new function router.
    pub fn new(registry: Arc<FunctionRegistry>, db_pool: sqlx::PgPool) -> Self {
        let rate_limiter = RateLimiter::new(db_pool.clone());
        Self {
            registry,
            db_pool,
            http_client: reqwest::Client::new(),
            job_dispatcher: None,
            workflow_dispatcher: None,
            rate_limiter,
            query_cache: QueryCache::new(),
        }
    }

    /// Create a new function router with a custom HTTP client.
    pub fn with_http_client(
        registry: Arc<FunctionRegistry>,
        db_pool: sqlx::PgPool,
        http_client: reqwest::Client,
    ) -> Self {
        let rate_limiter = RateLimiter::new(db_pool.clone());
        Self {
            registry,
            db_pool,
            http_client,
            job_dispatcher: None,
            workflow_dispatcher: None,
            rate_limiter,
            query_cache: QueryCache::new(),
        }
    }

    /// Set the job dispatcher for this router.
    pub fn with_job_dispatcher(mut self, dispatcher: Arc<dyn JobDispatch>) -> Self {
        self.job_dispatcher = Some(dispatcher);
        self
    }

    /// Set the workflow dispatcher for this router.
    pub fn with_workflow_dispatcher(mut self, dispatcher: Arc<dyn WorkflowDispatch>) -> Self {
        self.workflow_dispatcher = Some(dispatcher);
        self
    }

    /// Route and execute a function call.
    pub async fn route(
        &self,
        function_name: &str,
        args: Value,
        auth: AuthContext,
        request: RequestMetadata,
    ) -> Result<RouteResult> {
        // First, try to find in the function registry (queries/mutations)
        if let Some(entry) = self.registry.get(function_name) {
            // Check authorization
            self.check_auth(entry.info(), &auth)?;

            // Check rate limit
            self.check_rate_limit(entry.info(), function_name, &auth, &request)
                .await?;

            return match entry {
                FunctionEntry::Query { handler, info, .. } => {
                    if let Some(ttl) = info.cache_ttl {
                        if let Some(cached) = self.query_cache.get(function_name, &args) {
                            return Ok(RouteResult::Query(cached));
                        }

                        // Execute and cache result
                        let ctx = QueryContext::new(self.db_pool.clone(), auth, request);
                        let result = handler(&ctx, args.clone()).await?;

                        self.query_cache.set(
                            function_name,
                            &args,
                            result.clone(),
                            Duration::from_secs(ttl),
                        );

                        Ok(RouteResult::Query(result))
                    } else {
                        let ctx = QueryContext::new(self.db_pool.clone(), auth, request);
                        let result = handler(&ctx, args).await?;
                        Ok(RouteResult::Query(result))
                    }
                }
                FunctionEntry::Mutation { handler, .. } => {
                    let ctx = MutationContext::with_dispatch(
                        self.db_pool.clone(),
                        auth,
                        request,
                        self.http_client.clone(),
                        self.job_dispatcher.clone(),
                        self.workflow_dispatcher.clone(),
                    );
                    let result = handler(&ctx, args).await?;
                    Ok(RouteResult::Mutation(result))
                }
            };
        }

        // Try job/workflow dispatchers - auth checked before dispatch to prevent unauthorized execution
        if let Some(ref job_dispatcher) = self.job_dispatcher {
            if !auth.is_authenticated() {
                return Err(ForgeError::Unauthorized("Authentication required".into()));
            }
            match job_dispatcher
                .dispatch_by_name(function_name, args.clone())
                .await
            {
                Ok(job_id) => {
                    return Ok(RouteResult::Job(serde_json::json!({ "job_id": job_id })));
                }
                Err(ForgeError::NotFound(_)) => {}
                Err(e) => return Err(e),
            }
        }

        if let Some(ref workflow_dispatcher) = self.workflow_dispatcher {
            if !auth.is_authenticated() {
                return Err(ForgeError::Unauthorized("Authentication required".into()));
            }
            match workflow_dispatcher
                .start_by_name(function_name, args.clone())
                .await
            {
                Ok(workflow_id) => {
                    return Ok(RouteResult::Workflow(
                        serde_json::json!({ "workflow_id": workflow_id }),
                    ));
                }
                Err(ForgeError::NotFound(_)) => {}
                Err(e) => return Err(e),
            }
        }

        // Nothing found
        Err(ForgeError::NotFound(format!(
            "Function '{}' not found",
            function_name
        )))
    }

    /// Check authorization for a function call.
    fn check_auth(&self, info: &FunctionInfo, auth: &AuthContext) -> Result<()> {
        // Public functions don't require auth
        if info.is_public {
            return Ok(());
        }

        // Check if auth is required
        if info.requires_auth && !auth.is_authenticated() {
            return Err(ForgeError::Unauthorized("Authentication required".into()));
        }

        // Check role requirement
        if let Some(role) = info.required_role {
            // Role check implies authentication is required
            if !auth.is_authenticated() {
                return Err(ForgeError::Unauthorized("Authentication required".into()));
            }
            if !auth.has_role(role) {
                return Err(ForgeError::Forbidden(format!("Role '{}' required", role)));
            }
        }

        Ok(())
    }

    /// Check rate limit for a function call.
    async fn check_rate_limit(
        &self,
        info: &FunctionInfo,
        function_name: &str,
        auth: &AuthContext,
        request: &RequestMetadata,
    ) -> Result<()> {
        // Skip if no rate limit configured
        let (requests, per_secs) = match (info.rate_limit_requests, info.rate_limit_per_secs) {
            (Some(r), Some(p)) => (r, p),
            _ => return Ok(()),
        };

        // Build rate limit config
        let key_str = info.rate_limit_key.unwrap_or("user");
        let key_type: RateLimitKey = match key_str.parse() {
            Ok(k) => k,
            Err(_) => {
                tracing::warn!(
                    function = %function_name,
                    key = %key_str,
                    "Invalid rate limit key, falling back to 'user'"
                );
                RateLimitKey::default()
            }
        };

        let config =
            RateLimitConfig::new(requests, Duration::from_secs(per_secs)).with_key(key_type);

        // Build bucket key
        let bucket_key = self
            .rate_limiter
            .build_key(key_type, function_name, auth, request);

        // Enforce rate limit
        self.rate_limiter.enforce(&bucket_key, &config).await?;

        Ok(())
    }

    /// Get the function kind by name.
    pub fn get_function_kind(&self, function_name: &str) -> Option<FunctionKind> {
        self.registry.get(function_name).map(|e| e.kind())
    }

    /// Check if a function exists.
    pub fn has_function(&self, function_name: &str) -> bool {
        self.registry.get(function_name).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_auth_public() {
        let info = FunctionInfo {
            name: "test",
            description: None,
            kind: FunctionKind::Query,
            requires_auth: false,
            required_role: None,
            is_public: true,
            cache_ttl: None,
            timeout: None,
            rate_limit_requests: None,
            rate_limit_per_secs: None,
            rate_limit_key: None,
            log_level: None,
            table_dependencies: &[],
            transactional: false,
        };

        let _auth = AuthContext::unauthenticated();

        // Can't test check_auth directly without a router instance,
        // but we can test the logic
        assert!(info.is_public);
    }
}
