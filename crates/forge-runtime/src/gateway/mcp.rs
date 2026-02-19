use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::{Extension, State};
use axum::http::header::{HeaderName, HeaderValue};
use axum::http::{HeaderMap, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::{Json, body::Body};
use forge_core::config::McpConfig;
use forge_core::function::{AuthContext, JobDispatch, RequestMetadata, WorkflowDispatch};
use forge_core::mcp::McpToolContext;
use forge_core::rate_limit::RateLimitKey;
use serde_json::Value;
use tokio::sync::RwLock;

use crate::mcp::McpToolRegistry;
use crate::rate_limit::RateLimiter;

const MCP_PROTOCOL_VERSION: &str = "2025-11-25";
const MCP_SESSION_HEADER: &str = "mcp-session-id";
const MCP_PROTOCOL_HEADER: &str = "mcp-protocol-version";
const DEFAULT_PAGE_SIZE: usize = 50;
type ResponseError = Box<Response>;

#[derive(Debug, Clone)]
struct McpSession {
    initialized: bool,
    protocol_version: String,
    expires_at: Instant,
}

#[derive(Clone)]
pub struct McpState {
    config: McpConfig,
    registry: McpToolRegistry,
    pool: sqlx::PgPool,
    sessions: Arc<RwLock<HashMap<String, McpSession>>>,
    job_dispatcher: Option<Arc<dyn JobDispatch>>,
    workflow_dispatcher: Option<Arc<dyn WorkflowDispatch>>,
    rate_limiter: Arc<RateLimiter>,
}

impl McpState {
    pub fn new(
        config: McpConfig,
        registry: McpToolRegistry,
        pool: sqlx::PgPool,
        job_dispatcher: Option<Arc<dyn JobDispatch>>,
        workflow_dispatcher: Option<Arc<dyn WorkflowDispatch>>,
    ) -> Self {
        Self {
            config,
            registry,
            pool: pool.clone(),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            job_dispatcher,
            workflow_dispatcher,
            rate_limiter: Arc::new(RateLimiter::new(pool)),
        }
    }

    async fn cleanup_expired_sessions(&self) {
        let mut sessions = self.sessions.write().await;
        let now = Instant::now();
        sessions.retain(|_, session| session.expires_at > now);
    }

    async fn touch_session(&self, session_id: &str) {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(session_id) {
            session.expires_at = Instant::now() + Duration::from_secs(self.config.session_ttl_secs);
        }
    }

    fn session_ttl(&self) -> Duration {
        Duration::from_secs(self.config.session_ttl_secs)
    }
}

pub async fn mcp_get_handler(State(state): State<Arc<McpState>>, headers: HeaderMap) -> Response {
    if let Err(resp) = validate_origin(&headers, &state.config) {
        return *resp;
    }

    (
        StatusCode::METHOD_NOT_ALLOWED,
        Json(json_rpc_error(
            None,
            -32601,
            "GET stream is not supported in Forge MCP v1",
            None,
        )),
    )
        .into_response()
}

pub async fn mcp_post_handler(
    State(state): State<Arc<McpState>>,
    Extension(auth): Extension<AuthContext>,
    Extension(tracing): Extension<super::tracing::TracingState>,
    method: Method,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    if method != Method::POST {
        return (
            StatusCode::METHOD_NOT_ALLOWED,
            Json(json_rpc_error(None, -32601, "Only POST is supported", None)),
        )
            .into_response();
    }

    if let Err(resp) = validate_origin(&headers, &state.config) {
        return *resp;
    }

    state.cleanup_expired_sessions().await;

    let Some(method_name) = payload.get("method").and_then(Value::as_str) else {
        // Notifications / responses sent by client should get 202 when accepted.
        if payload.get("id").is_some()
            && (payload.get("result").is_some() || payload.get("error").is_some())
        {
            return StatusCode::ACCEPTED.into_response();
        }
        return (
            StatusCode::BAD_REQUEST,
            Json(json_rpc_error(
                None,
                -32600,
                "Invalid JSON-RPC payload",
                None,
            )),
        )
            .into_response();
    };

    let id = payload.get("id").cloned();
    let params = payload
        .get("params")
        .cloned()
        .unwrap_or(Value::Object(Default::default()));

    // Notification flow
    if id.is_none() {
        return handle_notification(&state, method_name, params, &headers).await;
    }

    // Request flow
    if method_name != "initialize"
        && let Err(resp) = enforce_protocol_header(&state.config, &headers)
    {
        return *resp;
    }

    match method_name {
        "initialize" => handle_initialize(&state, id, &params).await,
        "tools/list" => {
            let session_id = match required_session_id(&state, &headers, true).await {
                Ok(v) => v,
                Err(resp) => return resp,
            };
            state.touch_session(&session_id).await;
            handle_tools_list(&state, id, &params)
        }
        "tools/call" => {
            let session_id = match required_session_id(&state, &headers, true).await {
                Ok(v) => v,
                Err(resp) => return resp,
            };
            state.touch_session(&session_id).await;

            let metadata = build_request_metadata(&tracing, &headers);
            handle_tools_call(&state, id, &params, &auth, metadata).await
        }
        _ => (
            StatusCode::OK,
            Json(json_rpc_error(id, -32601, "Method not found", None)),
        )
            .into_response(),
    }
}

async fn handle_notification(
    state: &Arc<McpState>,
    method_name: &str,
    _params: Value,
    headers: &HeaderMap,
) -> Response {
    if let Err(resp) = enforce_protocol_header(&state.config, headers) {
        return *resp;
    }

    match method_name {
        "notifications/initialized" => {
            let session_id = match required_session_id(state, headers, false).await {
                Ok(v) => v,
                Err(resp) => return resp,
            };

            let mut sessions = state.sessions.write().await;
            if let Some(session) = sessions.get_mut(&session_id) {
                session.initialized = true;
                session.expires_at = Instant::now() + state.session_ttl();
                return StatusCode::ACCEPTED.into_response();
            }

            (
                StatusCode::BAD_REQUEST,
                Json(json_rpc_error(
                    None,
                    -32600,
                    "Unknown MCP session. Re-initialize the connection.",
                    None,
                )),
            )
                .into_response()
        }
        _ => StatusCode::ACCEPTED.into_response(),
    }
}

async fn handle_initialize(state: &Arc<McpState>, id: Option<Value>, params: &Value) -> Response {
    let Some(requested_version) = params.get("protocolVersion").and_then(Value::as_str) else {
        return (
            StatusCode::OK,
            Json(json_rpc_error(
                id,
                -32602,
                "Missing protocolVersion in initialize params",
                None,
            )),
        )
            .into_response();
    };

    if requested_version != MCP_PROTOCOL_VERSION {
        return (
            StatusCode::OK,
            Json(json_rpc_error(
                id,
                -32602,
                "Unsupported protocolVersion",
                Some(serde_json::json!({
                    "supported": MCP_PROTOCOL_VERSION
                })),
            )),
        )
            .into_response();
    }

    let session_id = uuid::Uuid::new_v4().to_string();
    {
        let mut sessions = state.sessions.write().await;
        sessions.insert(
            session_id.clone(),
            McpSession {
                initialized: false,
                protocol_version: MCP_PROTOCOL_VERSION.to_string(),
                expires_at: Instant::now() + state.session_ttl(),
            },
        );
    }

    let mut response = (
        StatusCode::OK,
        Json(json_rpc_success(
            id,
            serde_json::json!({
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": "forge",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
        )),
    )
        .into_response();

    set_header(&mut response, MCP_SESSION_HEADER, &session_id);
    set_header(&mut response, MCP_PROTOCOL_HEADER, MCP_PROTOCOL_VERSION);
    response
}

fn handle_tools_list(state: &Arc<McpState>, id: Option<Value>, params: &Value) -> Response {
    let cursor = params.get("cursor").and_then(Value::as_str);
    let start = match cursor {
        Some(c) => match c.parse::<usize>() {
            Ok(v) => v,
            Err(_) => {
                return (
                    StatusCode::OK,
                    Json(json_rpc_error(
                        id,
                        -32602,
                        "Invalid cursor in tools/list request",
                        None,
                    )),
                )
                    .into_response();
            }
        },
        None => 0,
    };

    let mut tools: Vec<_> = state.registry.list().collect();
    tools.sort_by(|a, b| a.info.name.cmp(b.info.name));

    let page: Vec<_> = tools
        .iter()
        .skip(start)
        .take(DEFAULT_PAGE_SIZE)
        .map(|entry| {
            let annotations = serde_json::json!({
                "title": entry.info.annotations.title,
                "readOnlyHint": entry.info.annotations.read_only_hint,
                "destructiveHint": entry.info.annotations.destructive_hint,
                "idempotentHint": entry.info.annotations.idempotent_hint,
                "openWorldHint": entry.info.annotations.open_world_hint
            });

            let icons: Vec<_> = entry
                .info
                .icons
                .iter()
                .map(|icon| {
                    serde_json::json!({
                        "src": icon.src,
                        "mimeType": icon.mime_type,
                        "sizes": icon.sizes,
                        "theme": icon.theme
                    })
                })
                .collect();

            let mut value = serde_json::json!({
                "name": entry.info.name,
                "title": entry.info.title,
                "description": entry.info.description,
                "inputSchema": entry.input_schema,
                "annotations": annotations,
                "icons": icons
            });
            if let Some(output_schema) = &entry.output_schema
                && let Some(obj) = value.as_object_mut()
            {
                obj.insert("outputSchema".to_string(), output_schema.clone());
            }
            value
        })
        .collect();

    let end = start.saturating_add(page.len());
    let next_cursor = if end < tools.len() {
        Some(end.to_string())
    } else {
        None
    };

    (
        StatusCode::OK,
        Json(json_rpc_success(
            id,
            serde_json::json!({
                "tools": page,
                "nextCursor": next_cursor
            }),
        )),
    )
        .into_response()
}

async fn handle_tools_call(
    state: &Arc<McpState>,
    id: Option<Value>,
    params: &Value,
    auth: &AuthContext,
    request_metadata: RequestMetadata,
) -> Response {
    let Some(tool_name) = params.get("name").and_then(Value::as_str) else {
        return (
            StatusCode::OK,
            Json(json_rpc_error(id, -32602, "Missing tool name", None)),
        )
            .into_response();
    };

    let Some(entry) = state.registry.get(tool_name) else {
        return (
            StatusCode::OK,
            Json(json_rpc_error(id, -32602, "Unknown tool", None)),
        )
            .into_response();
    };

    if !entry.info.is_public && !auth.is_authenticated() {
        return (
            StatusCode::OK,
            Json(json_rpc_error(id, -32001, "Authentication required", None)),
        )
            .into_response();
    }
    if let Some(role) = entry.info.required_role
        && !auth.has_role(role)
    {
        return (
            StatusCode::OK,
            Json(json_rpc_error(
                id,
                -32003,
                format!("Role '{}' required", role),
                None,
            )),
        )
            .into_response();
    }

    if let (Some(requests), Some(per_secs)) = (
        entry.info.rate_limit_requests,
        entry.info.rate_limit_per_secs,
    ) {
        let key_type = entry
            .info
            .rate_limit_key
            .and_then(|k| k.parse::<RateLimitKey>().ok())
            .unwrap_or_default();

        let config = forge_core::RateLimitConfig::new(requests, Duration::from_secs(per_secs))
            .with_key(key_type);
        let bucket_key = state
            .rate_limiter
            .build_key(key_type, tool_name, auth, &request_metadata);

        if let Err(e) = state.rate_limiter.enforce(&bucket_key, &config).await {
            return (
                StatusCode::OK,
                Json(json_rpc_error(id, -32029, e.to_string(), None)),
            )
                .into_response();
        }
    }

    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or(Value::Object(Default::default()));

    let ctx = McpToolContext::with_dispatch(
        state.pool.clone(),
        auth.clone(),
        request_metadata,
        state.job_dispatcher.clone(),
        state.workflow_dispatcher.clone(),
    );

    let result = if let Some(timeout_secs) = entry.info.timeout {
        match tokio::time::timeout(
            Duration::from_secs(timeout_secs),
            (entry.handler)(&ctx, args),
        )
        .await
        {
            Ok(inner) => inner,
            Err(_) => {
                return (
                    StatusCode::OK,
                    Json(json_rpc_error(id, -32000, "Tool timed out", None)),
                )
                    .into_response();
            }
        }
    } else {
        (entry.handler)(&ctx, args).await
    };

    match result {
        Ok(output) => {
            let result = tool_success_result(output);
            (
                StatusCode::OK,
                Json(json_rpc_success(id, serde_json::json!(result))),
            )
                .into_response()
        }
        Err(e) => match e {
            forge_core::ForgeError::Validation(msg)
            | forge_core::ForgeError::InvalidArgument(msg) => (
                StatusCode::OK,
                Json(json_rpc_success(
                    id,
                    serde_json::json!({
                        "content": [{ "type": "text", "text": msg }],
                        "isError": true
                    }),
                )),
            )
                .into_response(),
            forge_core::ForgeError::Unauthorized(msg) => {
                (StatusCode::OK, Json(json_rpc_error(id, -32001, msg, None))).into_response()
            }
            forge_core::ForgeError::Forbidden(msg) => {
                (StatusCode::OK, Json(json_rpc_error(id, -32003, msg, None))).into_response()
            }
            _ => (
                StatusCode::OK,
                Json(json_rpc_error(id, -32603, "Internal server error", None)),
            )
                .into_response(),
        },
    }
}

fn tool_success_result(output: Value) -> Value {
    match output {
        Value::Object(_) => serde_json::json!({
            "content": [{
                "type": "text",
                "text": serde_json::to_string(&output).unwrap_or_else(|_| "{}".to_string())
            }],
            "structuredContent": output
        }),
        Value::String(text) => serde_json::json!({
            "content": [{ "type": "text", "text": text }]
        }),
        other => serde_json::json!({
            "content": [{
                "type": "text",
                "text": serde_json::to_string(&other).unwrap_or_else(|_| "null".to_string())
            }]
        }),
    }
}

async fn required_session_id(
    state: &Arc<McpState>,
    headers: &HeaderMap,
    require_initialized: bool,
) -> std::result::Result<String, Response> {
    let Some(session_id) = headers
        .get(MCP_SESSION_HEADER)
        .and_then(|v| v.to_str().ok())
    else {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json_rpc_error(
                None,
                -32600,
                "Missing MCP-Session-Id header",
                None,
            )),
        )
            .into_response());
    };

    let sessions = state.sessions.read().await;
    match sessions.get(session_id) {
        Some(session) => {
            if session.protocol_version != MCP_PROTOCOL_VERSION {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(json_rpc_error(
                        None,
                        -32600,
                        "Session protocol version mismatch",
                        None,
                    )),
                )
                    .into_response());
            }
            if require_initialized && !session.initialized {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(json_rpc_error(
                        None,
                        -32600,
                        "MCP session is not initialized",
                        None,
                    )),
                )
                    .into_response());
            }
            Ok(session_id.to_string())
        }
        None => Err((
            StatusCode::BAD_REQUEST,
            Json(json_rpc_error(
                None,
                -32600,
                "Unknown MCP session. Re-initialize.",
                None,
            )),
        )
            .into_response()),
    }
}

fn validate_origin(
    headers: &HeaderMap,
    config: &McpConfig,
) -> std::result::Result<(), ResponseError> {
    let Some(origin) = headers.get("origin").and_then(|v| v.to_str().ok()) else {
        return Ok(());
    };

    if config.allowed_origins.is_empty() {
        return Ok(());
    }

    let allowed = config
        .allowed_origins
        .iter()
        .any(|candidate| candidate == "*" || candidate.eq_ignore_ascii_case(origin));
    if allowed {
        return Ok(());
    }

    Err(Box::new(
        (
            StatusCode::FORBIDDEN,
            Json(json_rpc_error(None, -32600, "Invalid Origin header", None)),
        )
            .into_response(),
    ))
}

fn enforce_protocol_header(
    config: &McpConfig,
    headers: &HeaderMap,
) -> std::result::Result<(), ResponseError> {
    if !config.require_protocol_version_header {
        return Ok(());
    }

    let Some(version) = headers
        .get(MCP_PROTOCOL_HEADER)
        .and_then(|v| v.to_str().ok())
    else {
        return Err(Box::new(
            (
                StatusCode::BAD_REQUEST,
                Json(json_rpc_error(
                    None,
                    -32600,
                    "Missing MCP-Protocol-Version header",
                    None,
                )),
            )
                .into_response(),
        ));
    };

    if version != MCP_PROTOCOL_VERSION {
        return Err(Box::new(
            (
                StatusCode::BAD_REQUEST,
                Json(json_rpc_error(
                    None,
                    -32600,
                    "Unsupported MCP-Protocol-Version",
                    Some(serde_json::json!({ "supported": MCP_PROTOCOL_VERSION })),
                )),
            )
                .into_response(),
        ));
    }

    Ok(())
}

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

fn extract_user_agent(headers: &HeaderMap) -> Option<String> {
    headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(String::from)
}

fn build_request_metadata(
    tracing: &super::tracing::TracingState,
    headers: &HeaderMap,
) -> RequestMetadata {
    RequestMetadata {
        request_id: uuid::Uuid::parse_str(&tracing.request_id)
            .unwrap_or_else(|_| uuid::Uuid::new_v4()),
        trace_id: tracing.trace_id.clone(),
        client_ip: extract_client_ip(headers),
        user_agent: extract_user_agent(headers),
        timestamp: chrono::Utc::now(),
    }
}

fn json_rpc_success(id: Option<Value>, result: Value) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(Value::Null),
        "result": result
    })
}

fn json_rpc_error(
    id: Option<Value>,
    code: i32,
    message: impl Into<String>,
    data: Option<Value>,
) -> Value {
    let mut error = serde_json::json!({
        "code": code,
        "message": message.into()
    });
    if let Some(data) = data
        && let Some(obj) = error.as_object_mut()
    {
        obj.insert("data".to_string(), data);
    }

    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(Value::Null),
        "error": error
    })
}

fn set_header(response: &mut Response<Body>, name: &str, value: &str) {
    if let (Ok(name), Ok(value)) = (HeaderName::try_from(name), HeaderValue::from_str(value)) {
        response.headers_mut().insert(name, value);
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::indexing_slicing, clippy::unwrap_used)]
mod tests {
    use super::super::tracing::TracingState;
    use super::*;
    use axum::body::to_bytes;
    use forge_core::function::AuthContext;
    use forge_core::mcp::{ForgeMcpTool, McpToolAnnotations, McpToolInfo};
    use forge_core::schemars::{self, JsonSchema};
    use serde::{Deserialize, Serialize};
    use std::collections::HashMap;
    use std::future::Future;
    use std::pin::Pin;

    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    struct EchoArgs {
        message: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    struct EchoOutput {
        echoed: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    #[serde(rename_all = "snake_case")]
    enum ExportFormat {
        Json,
        Csv,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    struct MetadataArgs {
        #[schemars(description = "Project UUID to export")]
        project_id: String,
        format: ExportFormat,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    struct MetadataOutput {
        accepted: bool,
    }

    struct EchoTool;

    impl ForgeMcpTool for EchoTool {
        type Args = EchoArgs;
        type Output = EchoOutput;

        fn info() -> McpToolInfo {
            McpToolInfo {
                name: "echo",
                title: Some("Echo"),
                description: Some("Echo back the message"),
                required_role: None,
                is_public: false,
                timeout: None,
                rate_limit_requests: None,
                rate_limit_per_secs: None,
                rate_limit_key: None,
                annotations: McpToolAnnotations::default(),
                icons: &[],
            }
        }

        fn execute(
            _ctx: &McpToolContext,
            args: Self::Args,
        ) -> Pin<Box<dyn Future<Output = forge_core::Result<Self::Output>> + Send + '_>> {
            Box::pin(async move {
                Ok(EchoOutput {
                    echoed: args.message,
                })
            })
        }
    }

    struct AdminTool;

    impl ForgeMcpTool for AdminTool {
        type Args = EchoArgs;
        type Output = EchoOutput;

        fn info() -> McpToolInfo {
            McpToolInfo {
                name: "admin.echo",
                title: Some("Admin Echo"),
                description: Some("Admin only echo"),
                required_role: Some("admin"),
                is_public: false,
                timeout: None,
                rate_limit_requests: None,
                rate_limit_per_secs: None,
                rate_limit_key: None,
                annotations: McpToolAnnotations::default(),
                icons: &[],
            }
        }

        fn execute(
            _ctx: &McpToolContext,
            args: Self::Args,
        ) -> Pin<Box<dyn Future<Output = forge_core::Result<Self::Output>> + Send + '_>> {
            Box::pin(async move {
                Ok(EchoOutput {
                    echoed: args.message,
                })
            })
        }
    }

    struct MetadataTool;

    impl ForgeMcpTool for MetadataTool {
        type Args = MetadataArgs;
        type Output = MetadataOutput;

        fn info() -> McpToolInfo {
            McpToolInfo {
                name: "export.project",
                title: Some("Export Project"),
                description: Some("Export project data"),
                required_role: None,
                is_public: false,
                timeout: None,
                rate_limit_requests: None,
                rate_limit_per_secs: None,
                rate_limit_key: None,
                annotations: McpToolAnnotations::default(),
                icons: &[],
            }
        }

        fn execute(
            _ctx: &McpToolContext,
            _args: Self::Args,
        ) -> Pin<Box<dyn Future<Output = forge_core::Result<Self::Output>> + Send + '_>> {
            Box::pin(async move { Ok(MetadataOutput { accepted: true }) })
        }
    }

    #[test]
    fn test_json_rpc_helpers() {
        let success = json_rpc_success(
            Some(serde_json::json!(1)),
            serde_json::json!({ "ok": true }),
        );
        assert_eq!(success["jsonrpc"], "2.0");
        assert!(success.get("result").is_some());

        let err = json_rpc_error(Some(serde_json::json!(1)), -32601, "not found", None);
        assert_eq!(err["error"]["code"], -32601);
    }

    fn test_state(config: McpConfig) -> Arc<McpState> {
        test_state_with_registry(config, McpToolRegistry::new())
    }

    fn test_state_with_registry(config: McpConfig, registry: McpToolRegistry) -> Arc<McpState> {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect_lazy("postgres://localhost/nonexistent")
            .expect("lazy pool must build");
        Arc::new(McpState::new(config, registry, pool, None, None))
    }

    async fn response_json(response: Response) -> Value {
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body bytes");
        if bytes.is_empty() {
            return serde_json::json!({});
        }
        serde_json::from_slice(&bytes).expect("valid json")
    }

    async fn initialize_session(state: Arc<McpState>) -> String {
        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": { "name": "test", "version": "1.0.0" }
            }
        });
        let response = mcp_post_handler(
            State(state),
            Extension(AuthContext::unauthenticated()),
            Extension(TracingState::new()),
            Method::POST,
            HeaderMap::new(),
            Json(payload),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        response
            .headers()
            .get(MCP_SESSION_HEADER)
            .and_then(|v| v.to_str().ok())
            .expect("session id must exist")
            .to_string()
    }

    async fn mark_initialized(state: Arc<McpState>, headers: HeaderMap) {
        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {}
        });
        let response = mcp_post_handler(
            State(state),
            Extension(AuthContext::unauthenticated()),
            Extension(TracingState::new()),
            Method::POST,
            headers,
            Json(payload),
        )
        .await;
        assert_eq!(response.status(), StatusCode::ACCEPTED);
    }

    async fn initialized_headers(state: Arc<McpState>) -> HeaderMap {
        let session_id = initialize_session(state.clone()).await;
        let mut headers = HeaderMap::new();
        headers.insert(
            MCP_SESSION_HEADER,
            HeaderValue::from_str(&session_id).expect("valid session id header"),
        );
        headers.insert(
            MCP_PROTOCOL_HEADER,
            HeaderValue::from_static(MCP_PROTOCOL_VERSION),
        );
        mark_initialized(state, headers.clone()).await;
        headers
    }

    #[tokio::test]
    async fn test_initialize_sets_session_header() {
        let state = test_state(McpConfig {
            enabled: true,
            ..Default::default()
        });
        let session = initialize_session(state).await;
        assert!(!session.is_empty());
    }

    #[tokio::test]
    async fn test_tools_list_requires_initialized_session() {
        let state = test_state(McpConfig {
            enabled: true,
            ..Default::default()
        });

        let session_id = initialize_session(state.clone()).await;

        let mut headers = HeaderMap::new();
        headers.insert(
            MCP_SESSION_HEADER,
            HeaderValue::from_str(&session_id).expect("valid"),
        );
        headers.insert(
            MCP_PROTOCOL_HEADER,
            HeaderValue::from_static(MCP_PROTOCOL_VERSION),
        );

        let list_payload = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {}
        });
        let response = mcp_post_handler(
            State(state),
            Extension(AuthContext::unauthenticated()),
            Extension(TracingState::new()),
            Method::POST,
            headers,
            Json(list_payload),
        )
        .await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_tools_list_returns_registered_tools() {
        let mut registry = McpToolRegistry::new();
        registry.register::<EchoTool>();

        let state = test_state_with_registry(
            McpConfig {
                enabled: true,
                ..Default::default()
            },
            registry,
        );
        let headers = initialized_headers(state.clone()).await;
        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {}
        });

        let response = mcp_post_handler(
            State(state),
            Extension(AuthContext::unauthenticated()),
            Extension(TracingState::new()),
            Method::POST,
            headers,
            Json(payload),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json(response).await;
        let tools = body["result"]["tools"]
            .as_array()
            .expect("tools list should be array");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"], "echo");
        assert!(tools[0].get("inputSchema").is_some());
        assert!(tools[0].get("outputSchema").is_some());
    }

    #[tokio::test]
    async fn test_tools_list_exposes_parameter_metadata() {
        let mut registry = McpToolRegistry::new();
        registry.register::<MetadataTool>();

        let state = test_state_with_registry(
            McpConfig {
                enabled: true,
                ..Default::default()
            },
            registry,
        );
        let headers = initialized_headers(state.clone()).await;
        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 9,
            "method": "tools/list",
            "params": {}
        });

        let response = mcp_post_handler(
            State(state),
            Extension(AuthContext::unauthenticated()),
            Extension(TracingState::new()),
            Method::POST,
            headers,
            Json(payload),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json(response).await;
        let tools = body["result"]["tools"]
            .as_array()
            .expect("tools list should be array");
        assert_eq!(tools.len(), 1);

        let input_schema = &tools[0]["inputSchema"];
        assert_eq!(
            input_schema["properties"]["project_id"]["description"],
            "Project UUID to export"
        );

        let schema_text = input_schema.to_string();
        assert!(schema_text.contains("\"json\""));
        assert!(schema_text.contains("\"csv\""));
    }

    #[tokio::test]
    async fn test_tools_call_success_returns_structured_content() {
        let mut registry = McpToolRegistry::new();
        registry.register::<EchoTool>();

        let state = test_state_with_registry(
            McpConfig {
                enabled: true,
                ..Default::default()
            },
            registry,
        );
        let headers = initialized_headers(state.clone()).await;
        let auth = AuthContext::authenticated(
            uuid::Uuid::new_v4(),
            vec!["member".to_string()],
            HashMap::new(),
        );
        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "echo",
                "arguments": { "message": "hello" }
            }
        });

        let response = mcp_post_handler(
            State(state),
            Extension(auth),
            Extension(TracingState::new()),
            Method::POST,
            headers,
            Json(payload),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json(response).await;
        assert_eq!(body["result"]["structuredContent"]["echoed"], "hello");
        assert_eq!(body["result"]["content"][0]["type"], "text");
    }

    #[tokio::test]
    async fn test_tools_call_validation_failure_returns_is_error() {
        let mut registry = McpToolRegistry::new();
        registry.register::<EchoTool>();

        let state = test_state_with_registry(
            McpConfig {
                enabled: true,
                ..Default::default()
            },
            registry,
        );
        let headers = initialized_headers(state.clone()).await;
        let auth = AuthContext::authenticated(
            uuid::Uuid::new_v4(),
            vec!["member".to_string()],
            HashMap::new(),
        );
        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": {
                "name": "echo",
                "arguments": {}
            }
        });

        let response = mcp_post_handler(
            State(state),
            Extension(auth),
            Extension(TracingState::new()),
            Method::POST,
            headers,
            Json(payload),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json(response).await;
        assert_eq!(body["result"]["isError"], true);
    }

    #[tokio::test]
    async fn test_tools_call_requires_authentication() {
        let mut registry = McpToolRegistry::new();
        registry.register::<EchoTool>();

        let state = test_state_with_registry(
            McpConfig {
                enabled: true,
                ..Default::default()
            },
            registry,
        );
        let headers = initialized_headers(state.clone()).await;
        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "tools/call",
            "params": {
                "name": "echo",
                "arguments": { "message": "hello" }
            }
        });

        let response = mcp_post_handler(
            State(state),
            Extension(AuthContext::unauthenticated()),
            Extension(TracingState::new()),
            Method::POST,
            headers,
            Json(payload),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json(response).await;
        assert_eq!(body["error"]["code"], -32001);
    }

    #[tokio::test]
    async fn test_tools_call_requires_role() {
        let mut registry = McpToolRegistry::new();
        registry.register::<AdminTool>();

        let state = test_state_with_registry(
            McpConfig {
                enabled: true,
                ..Default::default()
            },
            registry,
        );
        let headers = initialized_headers(state.clone()).await;
        let auth = AuthContext::authenticated(
            uuid::Uuid::new_v4(),
            vec!["member".to_string()],
            HashMap::new(),
        );
        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 6,
            "method": "tools/call",
            "params": {
                "name": "admin.echo",
                "arguments": { "message": "hello" }
            }
        });

        let response = mcp_post_handler(
            State(state),
            Extension(auth),
            Extension(TracingState::new()),
            Method::POST,
            headers,
            Json(payload),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json(response).await;
        assert_eq!(body["error"]["code"], -32003);
    }

    #[tokio::test]
    async fn test_invalid_protocol_header_returns_400() {
        let state = test_state(McpConfig {
            enabled: true,
            ..Default::default()
        });
        let session_id = initialize_session(state.clone()).await;
        let mut headers = HeaderMap::new();
        headers.insert(
            MCP_SESSION_HEADER,
            HeaderValue::from_str(&session_id).expect("valid"),
        );
        headers.insert(
            MCP_PROTOCOL_HEADER,
            HeaderValue::from_static("invalid-version"),
        );

        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "tools/list",
            "params": {}
        });

        let response = mcp_post_handler(
            State(state),
            Extension(AuthContext::unauthenticated()),
            Extension(TracingState::new()),
            Method::POST,
            headers,
            Json(payload),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_missing_protocol_header_returns_400() {
        let state = test_state(McpConfig {
            enabled: true,
            ..Default::default()
        });
        let session_id = initialize_session(state.clone()).await;
        let mut headers = HeaderMap::new();
        headers.insert(
            MCP_SESSION_HEADER,
            HeaderValue::from_str(&session_id).expect("valid"),
        );

        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 8,
            "method": "tools/list",
            "params": {}
        });

        let response = mcp_post_handler(
            State(state),
            Extension(AuthContext::unauthenticated()),
            Extension(TracingState::new()),
            Method::POST,
            headers,
            Json(payload),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_notifications_return_202() {
        let state = test_state(McpConfig {
            enabled: true,
            ..Default::default()
        });
        let mut headers = HeaderMap::new();
        headers.insert(
            MCP_PROTOCOL_HEADER,
            HeaderValue::from_static(MCP_PROTOCOL_VERSION),
        );
        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/tools/list_changed",
            "params": {}
        });
        let response = mcp_post_handler(
            State(state),
            Extension(AuthContext::unauthenticated()),
            Extension(TracingState::new()),
            Method::POST,
            headers,
            Json(payload),
        )
        .await;
        assert_eq!(response.status(), StatusCode::ACCEPTED);
    }

    #[tokio::test]
    async fn test_invalid_origin_rejected() {
        let state = test_state(McpConfig {
            enabled: true,
            allowed_origins: vec!["https://allowed.example".to_string()],
            ..Default::default()
        });
        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": { "name": "test", "version": "1.0.0" }
            }
        });

        let mut headers = HeaderMap::new();
        headers.insert("origin", HeaderValue::from_static("https://evil.example"));

        let response = mcp_post_handler(
            State(state),
            Extension(AuthContext::unauthenticated()),
            Extension(TracingState::new()),
            Method::POST,
            headers,
            Json(payload),
        )
        .await;

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
}
