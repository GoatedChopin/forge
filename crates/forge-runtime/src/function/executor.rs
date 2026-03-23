use std::sync::Arc;
use std::time::Duration;

use forge_core::{AuthContext, ForgeError, JobDispatch, RequestMetadata, Result, WorkflowDispatch};
use serde_json::Value;
use tokio::time::timeout;
use tracing::{Instrument, debug, error, info, trace, warn};

use super::registry::FunctionRegistry;
use super::router::{FunctionRouter, RouteResult};
use crate::db::Database;

/// Executes functions with timeout and error handling.
pub struct FunctionExecutor {
    router: FunctionRouter,
    registry: Arc<FunctionRegistry>,
    default_timeout: Duration,
}

impl FunctionExecutor {
    /// Create a new function executor.
    pub fn new(registry: Arc<FunctionRegistry>, db: Database) -> Self {
        Self {
            router: FunctionRouter::new(Arc::clone(&registry), db),
            registry,
            default_timeout: Duration::from_secs(30),
        }
    }

    /// Create a new function executor with custom timeout.
    pub fn with_timeout(
        registry: Arc<FunctionRegistry>,
        db: Database,
        default_timeout: Duration,
    ) -> Self {
        Self {
            router: FunctionRouter::new(Arc::clone(&registry), db),
            registry,
            default_timeout,
        }
    }

    /// Create a new function executor with dispatch capabilities.
    pub fn with_dispatch(
        registry: Arc<FunctionRegistry>,
        db: Database,
        job_dispatcher: Option<Arc<dyn JobDispatch>>,
        workflow_dispatcher: Option<Arc<dyn WorkflowDispatch>>,
    ) -> Self {
        Self::with_dispatch_and_issuer(registry, db, job_dispatcher, workflow_dispatcher, None)
    }

    /// Create a function executor with dispatch and token issuer.
    pub fn with_dispatch_and_issuer(
        registry: Arc<FunctionRegistry>,
        db: Database,
        job_dispatcher: Option<Arc<dyn JobDispatch>>,
        workflow_dispatcher: Option<Arc<dyn WorkflowDispatch>>,
        token_issuer: Option<Arc<dyn forge_core::TokenIssuer>>,
    ) -> Self {
        let mut router = FunctionRouter::new(Arc::clone(&registry), db);
        if let Some(jd) = job_dispatcher {
            router = router.with_job_dispatcher(jd);
        }
        if let Some(wd) = workflow_dispatcher {
            router = router.with_workflow_dispatcher(wd);
        }
        if let Some(issuer) = token_issuer {
            router = router.with_token_issuer(issuer);
        }
        Self {
            router,
            registry,
            default_timeout: Duration::from_secs(30),
        }
    }

    /// Set the token TTL config on the underlying router.
    pub fn set_token_ttl(&mut self, ttl: forge_core::AuthTokenTtl) {
        self.router.set_token_ttl(ttl);
    }

    /// Execute a function call.
    pub async fn execute(
        &self,
        function_name: &str,
        args: Value,
        auth: AuthContext,
        request: RequestMetadata,
    ) -> Result<ExecutionResult> {
        let start = std::time::Instant::now();
        let fn_timeout = self.get_function_timeout(function_name);
        let log_level = self.get_function_log_level(function_name);

        let kind = self
            .router
            .get_function_kind(function_name)
            .map(|k| k.to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let span = tracing::info_span!(
            "fn.execute",
            function = function_name,
            fn.kind = %kind,
        );

        let result = match timeout(
            fn_timeout,
            self.router
                .route(function_name, args.clone(), auth, request)
                .instrument(span),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => {
                let duration = start.elapsed();
                self.log_execution(
                    log_level,
                    function_name,
                    "unknown",
                    &args,
                    duration,
                    false,
                    Some(&format!("Timeout after {:?}", fn_timeout)),
                );
                crate::observability::record_fn_execution(
                    function_name,
                    &kind,
                    false,
                    duration.as_secs_f64(),
                );
                return Err(ForgeError::Timeout(format!(
                    "Function '{}' timed out after {:?}",
                    function_name, fn_timeout
                )));
            }
        };

        let duration = start.elapsed();

        match result {
            Ok(route_result) => {
                let (result_kind, value) = match route_result {
                    RouteResult::Query(v) => ("query", v),
                    RouteResult::Mutation(v) => ("mutation", v),
                    RouteResult::Job(v) => ("job", v),
                    RouteResult::Workflow(v) => ("workflow", v),
                };

                self.log_execution(
                    log_level,
                    function_name,
                    result_kind,
                    &args,
                    duration,
                    true,
                    None,
                );
                crate::observability::record_fn_execution(
                    function_name,
                    result_kind,
                    true,
                    duration.as_secs_f64(),
                );

                Ok(ExecutionResult {
                    function_name: function_name.to_string(),
                    function_kind: result_kind.to_string(),
                    result: value,
                    duration,
                    success: true,
                    error: None,
                })
            }
            Err(e) => {
                self.log_execution(
                    log_level,
                    function_name,
                    &kind,
                    &args,
                    duration,
                    false,
                    Some(&e.to_string()),
                );
                crate::observability::record_fn_execution(
                    function_name,
                    &kind,
                    false,
                    duration.as_secs_f64(),
                );

                Err(e)
            }
        }
    }

    /// Log function execution at the configured level.
    #[allow(clippy::too_many_arguments)]
    fn log_execution(
        &self,
        log_level: &str,
        function_name: &str,
        kind: &str,
        input: &Value,
        duration: Duration,
        success: bool,
        error: Option<&str>,
    ) {
        // Failures are always logged at error regardless of the function's
        // configured log level. Successes use the configured level.
        if !success {
            error!(
                function = function_name,
                kind = kind,
                duration_ms = duration.as_millis() as u64,
                error = error,
                "Function failed"
            );
            debug!(
                function = function_name,
                input = %input,
                "Function input"
            );
            return;
        }

        macro_rules! log_fn {
            ($level:ident) => {{
                $level!(
                    function = function_name,
                    kind = kind,
                    duration_ms = duration.as_millis() as u64,
                    "Function executed"
                );
                debug!(
                    function = function_name,
                    input = %input,
                    "Function input"
                );
            }};
        }

        match log_level {
            "off" => {}
            "error" => log_fn!(error),
            "warn" => log_fn!(warn),
            "info" => log_fn!(info),
            "debug" => log_fn!(debug),
            _ => log_fn!(trace),
        }
    }

    /// Mutations default to "info" because writes are worth tracking.
    /// Queries default to "debug" since they're high-volume.
    fn get_function_log_level(&self, function_name: &str) -> &'static str {
        self.registry
            .get(function_name)
            .map(|entry| {
                entry.info().log_level.unwrap_or(match entry.kind() {
                    forge_core::FunctionKind::Mutation => "info",
                    forge_core::FunctionKind::Query => "debug",
                })
            })
            .unwrap_or("info")
    }

    /// Get the timeout for a specific function.
    fn get_function_timeout(&self, function_name: &str) -> Duration {
        self.registry
            .get(function_name)
            .and_then(|entry| entry.info().timeout)
            .map(Duration::from_secs)
            .unwrap_or(self.default_timeout)
    }

    /// Check if a function exists.
    pub fn has_function(&self, function_name: &str) -> bool {
        self.router.has_function(function_name)
    }
}

/// Result of executing a function.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecutionResult {
    /// Function name that was executed.
    pub function_name: String,
    /// Kind of function (query, mutation).
    pub function_kind: String,
    /// The result value (or null on error).
    pub result: Value,
    /// Execution duration.
    #[serde(with = "duration_millis")]
    pub duration: Duration,
    /// Whether execution succeeded.
    pub success: bool,
    /// Error message if failed.
    pub error: Option<String>,
}

mod duration_millis {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(duration.as_millis() as u64)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let millis = u64::deserialize(deserializer)?;
        Ok(Duration::from_millis(millis))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_execution_result_serialization() {
        let result = ExecutionResult {
            function_name: "get_user".to_string(),
            function_kind: "query".to_string(),
            result: serde_json::json!({"id": "123"}),
            duration: Duration::from_millis(42),
            success: true,
            error: None,
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"duration\":42"));
        assert!(json.contains("\"success\":true"));
    }

    #[test]
    fn test_execution_result_round_trip() {
        let original = ExecutionResult {
            function_name: "create_user".to_string(),
            function_kind: "mutation".to_string(),
            result: serde_json::json!({"id": "456"}),
            duration: Duration::from_millis(100),
            success: true,
            error: None,
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: ExecutionResult = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.function_name, "create_user");
        assert_eq!(deserialized.duration, Duration::from_millis(100));
        assert!(deserialized.success);
    }
}
