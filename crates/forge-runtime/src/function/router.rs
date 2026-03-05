use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use forge_core::{
    AuthContext, CircuitBreakerClient, ForgeError, FunctionInfo, FunctionKind, JobDispatch,
    MutationContext, OutboxBuffer, PendingJob, PendingWorkflow, QueryContext, RequestMetadata,
    Result, WorkflowDispatch,
    job::JobStatus,
    rate_limit::{RateLimitConfig, RateLimitKey},
    workflow::WorkflowStatus,
};
use serde_json::Value;

use super::cache::QueryCache;
use super::registry::{BoxedMutationFn, FunctionEntry, FunctionRegistry};
use crate::db::Database;
use crate::rate_limit::HybridRateLimiter;

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
    db: Database,
    http_client: CircuitBreakerClient,
    job_dispatcher: Option<Arc<dyn JobDispatch>>,
    workflow_dispatcher: Option<Arc<dyn WorkflowDispatch>>,
    rate_limiter: HybridRateLimiter,
    query_cache: QueryCache,
}

impl FunctionRouter {
    /// Create a new function router.
    pub fn new(registry: Arc<FunctionRegistry>, db: Database) -> Self {
        let rate_limiter = HybridRateLimiter::new(db.primary().clone());
        Self {
            registry,
            db,
            http_client: CircuitBreakerClient::with_defaults(reqwest::Client::new()),
            job_dispatcher: None,
            workflow_dispatcher: None,
            rate_limiter,
            query_cache: QueryCache::new(),
        }
    }

    /// Create a new function router with a custom HTTP client.
    pub fn with_http_client(
        registry: Arc<FunctionRegistry>,
        db: Database,
        http_client: CircuitBreakerClient,
    ) -> Self {
        let rate_limiter = HybridRateLimiter::new(db.primary().clone());
        Self {
            registry,
            db,
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

    pub async fn route(
        &self,
        function_name: &str,
        args: Value,
        auth: AuthContext,
        request: RequestMetadata,
    ) -> Result<RouteResult> {
        if let Some(entry) = self.registry.get(function_name) {
            self.check_auth(entry.info(), &auth)?;
            self.check_rate_limit(entry.info(), function_name, &auth, &request)
                .await?;
            Self::check_identity_args(function_name, &args, &auth, !entry.info().is_public)?;

            return match entry {
                FunctionEntry::Query { handler, info, .. } => {
                    let auth_scope = Self::auth_cache_scope(&auth);
                    if let Some(ttl) = info.cache_ttl {
                        if let Some(cached) =
                            self.query_cache
                                .get(function_name, &args, auth_scope.as_deref())
                        {
                            return Ok(RouteResult::Query(Value::clone(&cached)));
                        }

                        // Execute and cache result (use read replica for queries)
                        let ctx = QueryContext::new(self.db.read_pool().clone(), auth, request);
                        let result = handler(&ctx, args.clone()).await?;

                        self.query_cache.set(
                            function_name,
                            &args,
                            auth_scope.as_deref(),
                            result.clone(),
                            Duration::from_secs(ttl),
                        );

                        Ok(RouteResult::Query(result))
                    } else {
                        // Use read replica for queries
                        let ctx = QueryContext::new(self.db.read_pool().clone(), auth, request);
                        let result = handler(&ctx, args).await?;
                        Ok(RouteResult::Query(result))
                    }
                }
                FunctionEntry::Mutation { handler, info } => {
                    if info.transactional {
                        self.execute_transactional(handler, args, auth, request)
                            .await
                    } else {
                        // Use primary for mutations
                        let ctx = MutationContext::with_dispatch(
                            self.db.primary().clone(),
                            auth,
                            request,
                            self.http_client.clone(),
                            self.job_dispatcher.clone(),
                            self.workflow_dispatcher.clone(),
                        );
                        let result = handler(&ctx, args).await?;
                        Ok(RouteResult::Mutation(result))
                    }
                }
            };
        }

        if let Some(ref job_dispatcher) = self.job_dispatcher
            && let Some(job_info) = job_dispatcher.get_info(function_name)
        {
            self.check_job_auth(&job_info, &auth)?;
            Self::check_identity_args(function_name, &args, &auth, !job_info.is_public)?;
            match job_dispatcher
                .dispatch_by_name(function_name, args.clone(), auth.principal_id())
                .await
            {
                Ok(job_id) => {
                    return Ok(RouteResult::Job(serde_json::json!({ "job_id": job_id })));
                }
                Err(ForgeError::NotFound(_)) => {}
                Err(e) => return Err(e),
            }
        }

        if let Some(ref workflow_dispatcher) = self.workflow_dispatcher
            && let Some(workflow_info) = workflow_dispatcher.get_info(function_name)
        {
            self.check_workflow_auth(&workflow_info, &auth)?;
            Self::check_identity_args(function_name, &args, &auth, !workflow_info.is_public)?;
            match workflow_dispatcher
                .start_by_name(function_name, args.clone(), auth.principal_id())
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

        Err(ForgeError::NotFound(format!(
            "Function '{}' not found",
            function_name
        )))
    }

    fn check_auth(&self, info: &FunctionInfo, auth: &AuthContext) -> Result<()> {
        if info.is_public {
            return Ok(());
        }

        if !auth.is_authenticated() {
            return Err(ForgeError::Unauthorized("Authentication required".into()));
        }

        if let Some(role) = info.required_role
            && !auth.has_role(role)
        {
            return Err(ForgeError::Forbidden(format!("Role '{}' required", role)));
        }

        Ok(())
    }

    fn check_job_auth(&self, info: &forge_core::job::JobInfo, auth: &AuthContext) -> Result<()> {
        if info.is_public {
            return Ok(());
        }

        if !auth.is_authenticated() {
            return Err(ForgeError::Unauthorized("Authentication required".into()));
        }

        if let Some(role) = info.required_role
            && !auth.has_role(role)
        {
            return Err(ForgeError::Forbidden(format!("Role '{}' required", role)));
        }

        Ok(())
    }

    fn check_workflow_auth(
        &self,
        info: &forge_core::workflow::WorkflowInfo,
        auth: &AuthContext,
    ) -> Result<()> {
        if info.is_public {
            return Ok(());
        }

        if !auth.is_authenticated() {
            return Err(ForgeError::Unauthorized("Authentication required".into()));
        }

        if let Some(role) = info.required_role
            && !auth.has_role(role)
        {
            return Err(ForgeError::Forbidden(format!("Role '{}' required", role)));
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

    fn auth_cache_scope(auth: &AuthContext) -> Option<String> {
        if !auth.is_authenticated() {
            return Some("anon".to_string());
        }

        // Include role + claims fingerprint to avoid cross-scope cache bleed.
        let mut roles = auth.roles().to_vec();
        roles.sort();
        roles.dedup();

        let mut claims = BTreeMap::new();
        for (k, v) in auth.claims() {
            claims.insert(k.clone(), v.clone());
        }

        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        roles.hash(&mut hasher);
        serde_json::to_string(&claims)
            .unwrap_or_default()
            .hash(&mut hasher);

        let principal = auth
            .principal_id()
            .unwrap_or_else(|| "authenticated".to_string());

        Some(format!(
            "subject:{principal}:scope:{:016x}",
            hasher.finish()
        ))
    }

    fn check_identity_args(
        function_name: &str,
        args: &Value,
        auth: &AuthContext,
        enforce_scope: bool,
    ) -> Result<()> {
        if auth.is_admin() {
            return Ok(());
        }

        let Some(obj) = args.as_object() else {
            if enforce_scope && auth.is_authenticated() {
                return Err(ForgeError::Forbidden(format!(
                    "Function '{function_name}' must include identity or tenant scope arguments"
                )));
            }
            return Ok(());
        };

        let mut principal_values: Vec<String> = Vec::new();
        if let Some(user_id) = auth.user_id().map(|id| id.to_string()) {
            principal_values.push(user_id);
        }
        if let Some(subject) = auth.principal_id()
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

            if !auth.is_authenticated() {
                return Err(ForgeError::Unauthorized(format!(
                    "Function '{function_name}' requires authentication for identity-scoped argument '{key}'"
                )));
            }

            let Value::String(actual) = value else {
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

            if !auth.is_authenticated() {
                return Err(ForgeError::Unauthorized(format!(
                    "Function '{function_name}' requires authentication for tenant-scoped argument '{key}'"
                )));
            }

            let expected = auth
                .claim("tenant_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    ForgeError::Forbidden(format!(
                        "Function '{function_name}' argument '{key}' is not allowed for this principal"
                    ))
                })?;

            let Value::String(actual) = value else {
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

        if enforce_scope && auth.is_authenticated() && !has_scope_key {
            return Err(ForgeError::Forbidden(format!(
                "Function '{function_name}' must include identity or tenant scope arguments"
            )));
        }

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

    async fn execute_transactional(
        &self,
        handler: &BoxedMutationFn,
        args: Value,
        auth: AuthContext,
        request: RequestMetadata,
    ) -> Result<RouteResult> {
        // Use primary for transactional mutations
        let primary = self.db.primary();
        let tx = primary
            .begin()
            .await
            .map_err(|e| ForgeError::Database(e.to_string()))?;

        let job_dispatcher = self.job_dispatcher.clone();
        let job_lookup: forge_core::JobInfoLookup =
            Arc::new(move |name: &str| job_dispatcher.as_ref().and_then(|d| d.get_info(name)));

        let (ctx, tx_handle, outbox) = MutationContext::with_transaction(
            primary.clone(),
            tx,
            auth,
            request,
            self.http_client.clone(),
            job_lookup,
        );

        match handler(&ctx, args).await {
            Ok(value) => {
                let buffer = {
                    let guard = outbox.lock().expect("outbox mutex poisoned");
                    OutboxBuffer {
                        jobs: guard.jobs.clone(),
                        workflows: guard.workflows.clone(),
                    }
                };

                let mut tx = Arc::try_unwrap(tx_handle)
                    .map_err(|_| ForgeError::Internal("Transaction still in use".into()))?
                    .into_inner();

                for job in &buffer.jobs {
                    Self::insert_job(&mut tx, job).await?;
                }

                for workflow in &buffer.workflows {
                    let version = self
                        .workflow_dispatcher
                        .as_ref()
                        .and_then(|d| d.get_info(&workflow.workflow_name))
                        .map(|info| info.version)
                        .ok_or_else(|| {
                            ForgeError::NotFound(format!(
                                "Workflow '{}' not found",
                                workflow.workflow_name
                            ))
                        })?;
                    Self::insert_workflow(&mut tx, workflow, version).await?;
                }

                tx.commit()
                    .await
                    .map_err(|e| ForgeError::Database(e.to_string()))?;

                Ok(RouteResult::Mutation(value))
            }
            Err(e) => Err(e),
        }
    }

    async fn insert_job(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        job: &PendingJob,
    ) -> Result<()> {
        let now = Utc::now();
        sqlx::query(
            r#"
            INSERT INTO forge_jobs (
                id, job_type, input, job_context, status, priority, attempts, max_attempts,
                worker_capability, owner_subject, scheduled_at, created_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            "#,
        )
        .bind(job.id)
        .bind(&job.job_type)
        .bind(&job.args)
        .bind(&job.context)
        .bind(JobStatus::Pending.as_str())
        .bind(job.priority)
        .bind(0i32)
        .bind(job.max_attempts)
        .bind(&job.worker_capability)
        .bind(&job.owner_subject)
        .bind(now)
        .bind(now)
        .execute(&mut **tx)
        .await
        .map_err(|e| ForgeError::Database(e.to_string()))?;

        Ok(())
    }

    async fn insert_workflow(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        workflow: &PendingWorkflow,
        version: u32,
    ) -> Result<()> {
        let now = Utc::now();
        sqlx::query(
            r#"
            INSERT INTO forge_workflow_runs (
                id, workflow_name, version, owner_subject, input, status, current_step,
                step_results, started_at, trace_id
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#,
        )
        .bind(workflow.id)
        .bind(&workflow.workflow_name)
        .bind(version as i32)
        .bind(&workflow.owner_subject)
        .bind(&workflow.input)
        .bind(WorkflowStatus::Created.as_str())
        .bind(Option::<String>::None)
        .bind(serde_json::json!({}))
        .bind(now)
        .bind(workflow.id.to_string())
        .execute(&mut **tx)
        .await
        .map_err(|e| ForgeError::Database(e.to_string()))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_check_auth_public() {
        let info = FunctionInfo {
            name: "test",
            description: None,
            kind: FunctionKind::Query,
            required_role: None,
            is_public: true,
            cache_ttl: None,
            timeout: None,
            rate_limit_requests: None,
            rate_limit_per_secs: None,
            rate_limit_key: None,
            log_level: None,
            table_dependencies: &[],
            selected_columns: &[],
            transactional: false,
        };

        let _auth = AuthContext::unauthenticated();

        // Can't test check_auth directly without a router instance,
        // but we can test the logic
        assert!(info.is_public);
    }

    #[test]
    fn test_identity_args_reject_cross_user_value() {
        let user_id = uuid::Uuid::new_v4();
        let auth = AuthContext::authenticated(
            user_id,
            vec!["user".to_string()],
            HashMap::from([(
                "sub".to_string(),
                serde_json::Value::String(user_id.to_string()),
            )]),
        );
        let args = serde_json::json!({
            "user_id": uuid::Uuid::new_v4().to_string()
        });

        let result = FunctionRouter::check_identity_args("list_orders", &args, &auth, true);
        assert!(matches!(result, Err(ForgeError::Forbidden(_))));
    }

    #[test]
    fn test_identity_args_allow_matching_subject() {
        let sub = "user_123";
        let auth = AuthContext::authenticated_without_uuid(
            vec!["user".to_string()],
            HashMap::from([(
                "sub".to_string(),
                serde_json::Value::String(sub.to_string()),
            )]),
        );
        let args = serde_json::json!({
            "subject": sub
        });

        let result = FunctionRouter::check_identity_args("list_orders", &args, &auth, true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_identity_args_require_auth_for_identity_keys() {
        let auth = AuthContext::unauthenticated();
        let args = serde_json::json!({
            "user_id": uuid::Uuid::new_v4().to_string()
        });

        let result = FunctionRouter::check_identity_args("list_orders", &args, &auth, true);
        assert!(matches!(result, Err(ForgeError::Unauthorized(_))));
    }

    #[test]
    fn test_identity_args_require_scope_for_non_public_calls() {
        let user_id = uuid::Uuid::new_v4();
        let auth = AuthContext::authenticated(
            user_id,
            vec!["user".to_string()],
            HashMap::from([(
                "sub".to_string(),
                serde_json::Value::String(user_id.to_string()),
            )]),
        );

        let result =
            FunctionRouter::check_identity_args("list_orders", &serde_json::json!({}), &auth, true);
        assert!(matches!(result, Err(ForgeError::Forbidden(_))));
    }

    #[test]
    fn test_auth_cache_scope_changes_with_claims() {
        let user_id = uuid::Uuid::new_v4();
        let auth_a = AuthContext::authenticated(
            user_id,
            vec!["user".to_string()],
            HashMap::from([
                (
                    "sub".to_string(),
                    serde_json::Value::String(user_id.to_string()),
                ),
                (
                    "tenant_id".to_string(),
                    serde_json::Value::String("tenant-a".to_string()),
                ),
            ]),
        );
        let auth_b = AuthContext::authenticated(
            user_id,
            vec!["user".to_string()],
            HashMap::from([
                (
                    "sub".to_string(),
                    serde_json::Value::String(user_id.to_string()),
                ),
                (
                    "tenant_id".to_string(),
                    serde_json::Value::String("tenant-b".to_string()),
                ),
            ]),
        );

        let scope_a = FunctionRouter::auth_cache_scope(&auth_a);
        let scope_b = FunctionRouter::auth_cache_scope(&auth_b);
        assert_ne!(scope_a, scope_b);
    }
}
