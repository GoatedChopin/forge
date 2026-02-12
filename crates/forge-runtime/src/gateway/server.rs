use std::sync::Arc;
use std::time::Duration;

use axum::{
    Json, Router,
    error_handling::HandleErrorLayer,
    extract::DefaultBodyLimit,
    http::StatusCode,
    middleware,
    response::IntoResponse,
    routing::{get, post},
};
use serde::Serialize;
use tower::BoxError;
use tower::ServiceBuilder;
use tower::limit::ConcurrencyLimitLayer;
use tower::timeout::TimeoutLayer;
use tower_http::cors::{Any, CorsLayer};

use forge_core::cluster::NodeId;
use forge_core::function::{JobDispatch, WorkflowDispatch};

use super::auth::{AuthConfig, AuthMiddleware, auth_middleware};
use super::multipart::rpc_multipart_handler;
use super::response::{RpcError, RpcResponse};
use super::rpc::{RpcHandler, rpc_function_handler, rpc_handler};
use super::sse::{
    SseState, sse_handler, sse_job_subscribe_handler, sse_subscribe_handler,
    sse_unsubscribe_handler, sse_workflow_subscribe_handler,
};
use super::tracing::{REQUEST_ID_HEADER, SPAN_ID_HEADER, TRACE_ID_HEADER, TracingState};
use crate::db::Database;
use crate::function::FunctionRegistry;
use crate::realtime::{Reactor, ReactorConfig};

const MAX_JSON_BODY_SIZE: usize = 1024 * 1024;
const MAX_MULTIPART_BODY_SIZE: usize = 20 * 1024 * 1024;
const MAX_MULTIPART_CONCURRENCY: usize = 32;

/// Gateway server configuration.
#[derive(Debug, Clone)]
pub struct GatewayConfig {
    /// Port to listen on.
    pub port: u16,
    /// Maximum number of connections.
    pub max_connections: usize,
    /// Request timeout in seconds.
    pub request_timeout_secs: u64,
    /// Enable CORS.
    pub cors_enabled: bool,
    /// Allowed CORS origins.
    pub cors_origins: Vec<String>,
    /// Authentication configuration.
    pub auth: AuthConfig,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            port: 8080,
            max_connections: 512,
            request_timeout_secs: 30,
            cors_enabled: false,
            cors_origins: Vec::new(),
            auth: AuthConfig::default(),
        }
    }
}

/// Health check response.
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}

/// Readiness check response.
#[derive(Debug, Serialize)]
pub struct ReadinessResponse {
    pub ready: bool,
    pub database: bool,
    pub reactor: bool,
    pub version: String,
}

/// State for readiness check.
#[derive(Clone)]
pub struct ReadinessState {
    db_pool: sqlx::PgPool,
    reactor: Arc<Reactor>,
}

/// Gateway HTTP server.
pub struct GatewayServer {
    config: GatewayConfig,
    registry: FunctionRegistry,
    db: Database,
    reactor: Arc<Reactor>,
    job_dispatcher: Option<Arc<dyn JobDispatch>>,
    workflow_dispatcher: Option<Arc<dyn WorkflowDispatch>>,
}

impl GatewayServer {
    /// Create a new gateway server.
    pub fn new(config: GatewayConfig, registry: FunctionRegistry, db: Database) -> Self {
        let node_id = NodeId::new();
        let reactor = Arc::new(Reactor::new(
            node_id,
            db.read_pool().clone(),
            registry.clone(),
            ReactorConfig::default(),
        ));

        Self {
            config,
            registry,
            db,
            reactor,
            job_dispatcher: None,
            workflow_dispatcher: None,
        }
    }

    /// Set the job dispatcher.
    pub fn with_job_dispatcher(mut self, dispatcher: Arc<dyn JobDispatch>) -> Self {
        self.job_dispatcher = Some(dispatcher);
        self
    }

    /// Set the workflow dispatcher.
    pub fn with_workflow_dispatcher(mut self, dispatcher: Arc<dyn WorkflowDispatch>) -> Self {
        self.workflow_dispatcher = Some(dispatcher);
        self
    }

    /// Get a reference to the reactor.
    pub fn reactor(&self) -> Arc<Reactor> {
        self.reactor.clone()
    }

    /// Build the Axum router.
    pub fn router(&self) -> Router {
        let rpc_handler_state = Arc::new(RpcHandler::with_dispatch(
            self.registry.clone(),
            self.db.clone(),
            self.job_dispatcher.clone(),
            self.workflow_dispatcher.clone(),
        ));

        let auth_middleware_state = Arc::new(AuthMiddleware::new(self.config.auth.clone()));

        // Build CORS layer
        let cors = if self.config.cors_enabled {
            if self.config.cors_origins.iter().any(|o| o == "*") {
                CorsLayer::new()
                    .allow_origin(Any)
                    .allow_methods(Any)
                    .allow_headers(Any)
            } else {
                let origins: Vec<_> = self
                    .config
                    .cors_origins
                    .iter()
                    .filter_map(|o| o.parse().ok())
                    .collect();
                CorsLayer::new()
                    .allow_origin(origins)
                    .allow_methods(Any)
                    .allow_headers(Any)
            }
        } else {
            CorsLayer::new()
        };

        // SSE state for Server-Sent Events
        let sse_state = Arc::new(SseState::new(
            self.reactor.clone(),
            auth_middleware_state.clone(),
        ));

        // Readiness state for DB + reactor health check
        let readiness_state = Arc::new(ReadinessState {
            db_pool: self.db.primary().clone(),
            reactor: self.reactor.clone(),
        });

        // Build the main router with middleware
        let mut main_router = Router::new()
            // Health check endpoint (liveness)
            .route("/health", get(health_handler))
            // Readiness check endpoint (checks DB)
            .route("/ready", get(readiness_handler).with_state(readiness_state))
            // RPC endpoint
            .route("/rpc", post(rpc_handler))
            // REST-style function endpoint (JSON)
            .route("/rpc/{function}", post(rpc_function_handler))
            // Prevent oversized JSON payloads from exhausting memory.
            .layer(DefaultBodyLimit::max(MAX_JSON_BODY_SIZE))
            // Add state
            .with_state(rpc_handler_state.clone());

        // Multipart RPC router (separate state needed for multipart)
        let multipart_router = Router::new()
            .route("/rpc/{function}/upload", post(rpc_multipart_handler))
            .layer(DefaultBodyLimit::max(MAX_MULTIPART_BODY_SIZE))
            // Cap upload fan-out; each request buffers data in memory.
            .layer(ConcurrencyLimitLayer::new(MAX_MULTIPART_CONCURRENCY))
            .with_state(rpc_handler_state);

        // SSE router
        let sse_router = Router::new()
            .route("/events", get(sse_handler))
            .route("/subscribe", post(sse_subscribe_handler))
            .route("/unsubscribe", post(sse_unsubscribe_handler))
            .route("/subscribe-job", post(sse_job_subscribe_handler))
            .route("/subscribe-workflow", post(sse_workflow_subscribe_handler))
            .with_state(sse_state);

        main_router = main_router.merge(multipart_router).merge(sse_router);

        // Build middleware stack
        let service_builder = ServiceBuilder::new()
            .layer(HandleErrorLayer::new(handle_middleware_error))
            .layer(ConcurrencyLimitLayer::new(self.config.max_connections))
            .layer(TimeoutLayer::new(Duration::from_secs(
                self.config.request_timeout_secs,
            )))
            .layer(cors.clone())
            .layer(middleware::from_fn_with_state(
                auth_middleware_state,
                auth_middleware,
            ))
            .layer(middleware::from_fn(tracing_middleware));

        // Apply the remaining middleware layers
        main_router.layer(service_builder)
    }

    /// Get the socket address to bind to.
    pub fn addr(&self) -> std::net::SocketAddr {
        std::net::SocketAddr::from(([0, 0, 0, 0], self.config.port))
    }

    /// Run the server (blocking).
    pub async fn run(self) -> Result<(), std::io::Error> {
        let addr = self.addr();
        let router = self.router();

        // Start the reactor for real-time updates
        self.reactor
            .start()
            .await
            .map_err(|e| std::io::Error::other(format!("Failed to start reactor: {}", e)))?;
        tracing::info!("Reactor started for real-time updates");

        tracing::info!("Gateway server listening on {}", addr);

        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, router.into_make_service()).await
    }
}

/// Health check handler (liveness probe).
async fn health_handler() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

/// Readiness check handler (readiness probe).
async fn readiness_handler(
    axum::extract::State(state): axum::extract::State<Arc<ReadinessState>>,
) -> (axum::http::StatusCode, Json<ReadinessResponse>) {
    // Check database connectivity
    let db_ok = sqlx::query("SELECT 1")
        .fetch_one(&state.db_pool)
        .await
        .is_ok();

    // Check reactor health (change listener must be running for real-time updates)
    let reactor_stats = state.reactor.stats().await;
    let reactor_ok = reactor_stats.listener_running;

    let ready = db_ok && reactor_ok;
    let status_code = if ready {
        axum::http::StatusCode::OK
    } else {
        axum::http::StatusCode::SERVICE_UNAVAILABLE
    };

    (
        status_code,
        Json(ReadinessResponse {
            ready,
            database: db_ok,
            reactor: reactor_ok,
            version: env!("CARGO_PKG_VERSION").to_string(),
        }),
    )
}

async fn handle_middleware_error(err: BoxError) -> axum::response::Response {
    let (status, code, message) = if err.is::<tower::timeout::error::Elapsed>() {
        (StatusCode::REQUEST_TIMEOUT, "TIMEOUT", "Request timed out")
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "SERVICE_UNAVAILABLE",
            "Server overloaded",
        )
    };
    (
        status,
        Json(RpcResponse::error(RpcError::new(code, message))),
    )
        .into_response()
}

/// Simple tracing middleware that adds TracingState to extensions.
async fn tracing_middleware(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let headers = req.headers();

    let trace_id = headers
        .get(TRACE_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(String::from)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let parent_span_id = headers
        .get(SPAN_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    let mut tracing_state = TracingState::with_trace_id(trace_id.clone());
    if let Some(span_id) = parent_span_id {
        tracing_state = tracing_state.with_parent_span(span_id);
    }

    let mut req = req;
    req.extensions_mut().insert(tracing_state.clone());

    if req
        .extensions()
        .get::<forge_core::function::AuthContext>()
        .is_none()
    {
        req.extensions_mut()
            .insert(forge_core::function::AuthContext::unauthenticated());
    }

    let mut response = next.run(req).await;

    let elapsed = tracing_state.elapsed();
    tracing::debug!(
        trace_id = %trace_id,
        request_id = %tracing_state.request_id,
        status = %response.status().as_u16(),
        duration_ms = %elapsed.as_millis(),
        "Request completed"
    );

    if let Ok(val) = trace_id.parse() {
        response.headers_mut().insert(TRACE_ID_HEADER, val);
    }
    if let Ok(val) = tracing_state.request_id.parse() {
        response.headers_mut().insert(REQUEST_ID_HEADER, val);
    }

    response
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_gateway_config_default() {
        let config = GatewayConfig::default();
        assert_eq!(config.port, 8080);
        assert_eq!(config.max_connections, 512);
        assert!(!config.cors_enabled);
    }

    #[test]
    fn test_health_response_serialization() {
        let resp = HealthResponse {
            status: "healthy".to_string(),
            version: "0.1.0".to_string(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("healthy"));
    }
}
