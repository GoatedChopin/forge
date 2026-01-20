use std::sync::Arc;

use axum::{
    Json,
    extract::{Extension, State},
    http::{HeaderMap, header::USER_AGENT},
};
use forge_core::function::{AuthContext, JobDispatch, RequestMetadata, WorkflowDispatch};

use super::request::RpcRequest;
use super::response::{RpcError, RpcResponse};
use super::tracing::TracingState;
use crate::function::{FunctionExecutor, FunctionRegistry};

/// RPC handler for function invocations.
#[derive(Clone)]
pub struct RpcHandler {
    /// Function executor.
    executor: Arc<FunctionExecutor>,
}

impl RpcHandler {
    /// Create a new RPC handler.
    pub fn new(registry: FunctionRegistry, db_pool: sqlx::PgPool) -> Self {
        let executor = FunctionExecutor::new(Arc::new(registry), db_pool);
        Self {
            executor: Arc::new(executor),
        }
    }

    /// Create a new RPC handler with dispatch capabilities.
    pub fn with_dispatch(
        registry: FunctionRegistry,
        db_pool: sqlx::PgPool,
        job_dispatcher: Option<Arc<dyn JobDispatch>>,
        workflow_dispatcher: Option<Arc<dyn WorkflowDispatch>>,
    ) -> Self {
        let executor = FunctionExecutor::with_dispatch(
            Arc::new(registry),
            db_pool,
            job_dispatcher,
            workflow_dispatcher,
        );
        Self {
            executor: Arc::new(executor),
        }
    }

    /// Handle an RPC request.
    pub async fn handle(
        &self,
        request: RpcRequest,
        auth: AuthContext,
        metadata: RequestMetadata,
    ) -> RpcResponse {
        // Don't check has_function early - let executor try jobs/workflows too
        match self
            .executor
            .execute(&request.function, request.args, auth, metadata.clone())
            .await
        {
            Ok(exec_result) => RpcResponse::success(exec_result.result)
                .with_request_id(metadata.request_id.to_string()),
            Err(e) => RpcResponse::error(RpcError::from(e))
                .with_request_id(metadata.request_id.to_string()),
        }
    }
}

/// Extract client IP from X-Forwarded-For or X-Real-IP headers.
fn extract_client_ip(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or("").trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            headers
                .get("x-real-ip")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        })
}

/// Extract user agent from headers.
fn extract_user_agent(headers: &HeaderMap) -> Option<String> {
    headers
        .get(USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(String::from)
}

/// Axum handler for POST /rpc.
pub async fn rpc_handler(
    State(handler): State<Arc<RpcHandler>>,
    Extension(auth): Extension<AuthContext>,
    Extension(tracing): Extension<TracingState>,
    headers: HeaderMap,
    Json(request): Json<RpcRequest>,
) -> RpcResponse {
    let metadata = RequestMetadata {
        request_id: uuid::Uuid::parse_str(&tracing.request_id)
            .unwrap_or_else(|_| uuid::Uuid::new_v4()),
        trace_id: tracing.trace_id,
        client_ip: extract_client_ip(&headers),
        user_agent: extract_user_agent(&headers),
        timestamp: chrono::Utc::now(),
    };

    handler.handle(request, auth, metadata).await
}

/// Request body wrapper for REST-style RPC calls.
#[derive(Debug, serde::Deserialize)]
pub struct RpcFunctionBody {
    /// Function arguments.
    #[serde(default)]
    pub args: serde_json::Value,
}

/// Axum handler for POST /rpc/:function (REST-style).
pub async fn rpc_function_handler(
    State(handler): State<Arc<RpcHandler>>,
    Extension(auth): Extension<AuthContext>,
    Extension(tracing): Extension<TracingState>,
    headers: HeaderMap,
    axum::extract::Path(function): axum::extract::Path<String>,
    Json(body): Json<RpcFunctionBody>,
) -> RpcResponse {
    let request = RpcRequest::new(function, body.args);

    let metadata = RequestMetadata {
        request_id: uuid::Uuid::parse_str(&tracing.request_id)
            .unwrap_or_else(|_| uuid::Uuid::new_v4()),
        trace_id: tracing.trace_id,
        client_ip: extract_client_ip(&headers),
        user_agent: extract_user_agent(&headers),
        timestamp: chrono::Utc::now(),
    };

    handler.handle(request, auth, metadata).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_mock_pool() -> sqlx::PgPool {
        sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect_lazy("postgres://localhost/nonexistent")
            .expect("Failed to create mock pool")
    }

    fn create_test_handler() -> RpcHandler {
        let registry = FunctionRegistry::new();
        let db_pool = create_mock_pool();
        RpcHandler::new(registry, db_pool)
    }

    #[tokio::test]
    async fn test_handle_unknown_function() {
        let handler = create_test_handler();
        let request = RpcRequest::new("unknown_function", serde_json::json!({}));
        let auth = AuthContext::unauthenticated();
        let metadata = RequestMetadata::new();

        let response = handler.handle(request, auth, metadata).await;

        assert!(!response.success);
        assert!(response.error.is_some());
        assert_eq!(response.error.as_ref().unwrap().code, "NOT_FOUND");
    }
}
