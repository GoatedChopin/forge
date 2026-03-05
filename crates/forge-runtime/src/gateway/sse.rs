//! Server-Sent Events (SSE) handler for real-time updates.

use std::collections::HashMap;
use std::convert::Infallible;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use axum::Json;
use axum::extract::{Extension, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_util::Stream;
use serde::{Deserialize, Serialize};
use tokio::sync::{RwLock, mpsc};
use tokio_util::sync::CancellationToken;

/// Wraps an mpsc::Receiver as a Stream for SSE.
struct ReceiverStream<T> {
    rx: mpsc::Receiver<T>,
}

impl<T> Stream for ReceiverStream<T> {
    type Item = T;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<T>> {
        self.rx.poll_recv(cx)
    }
}

use forge_core::function::AuthContext;
use forge_core::realtime::{SessionId, SubscriptionId};

use super::auth::AuthMiddleware;
use crate::realtime::Reactor;
use crate::realtime::RealtimeMessage;

/// Maximum length for client subscription IDs to prevent memory bloat
const MAX_CLIENT_SUB_ID_LEN: usize = 255;

fn try_parse_session_id(session_id: &str) -> Option<SessionId> {
    uuid::Uuid::parse_str(session_id)
        .ok()
        .map(SessionId::from_uuid)
}

fn same_principal(a: &AuthContext, b: &AuthContext) -> bool {
    match (a.is_authenticated(), b.is_authenticated()) {
        (false, false) => true,
        (true, true) => a.principal_id().is_some() && a.principal_id() == b.principal_id(),
        _ => false,
    }
}

fn authorize_session_access(
    session: &SseSessionData,
    session_secret: &str,
    requester_auth: &AuthContext,
) -> Result<AuthContext, (StatusCode, Json<SseSubscribeResponse>)> {
    if session.session_secret != session_secret {
        return Err(subscribe_error(
            StatusCode::UNAUTHORIZED,
            "INVALID_SESSION_SECRET",
            "Session secret mismatch",
        ));
    }

    if !same_principal(&session.auth_context, requester_auth) {
        return Err(subscribe_error(
            StatusCode::FORBIDDEN,
            "SESSION_PRINCIPAL_MISMATCH",
            "Request principal does not match session principal",
        ));
    }

    Ok(session.auth_context.clone())
}

/// SSE configuration.
#[derive(Debug, Clone)]
pub struct SseConfig {
    /// Maximum sessions per server instance.
    pub max_sessions: usize,
    /// Channel buffer size for SSE messages.
    pub channel_buffer_size: usize,
    /// Keepalive interval in seconds.
    pub keepalive_interval_secs: u64,
}

impl Default for SseConfig {
    fn default() -> Self {
        Self {
            max_sessions: 10_000,
            channel_buffer_size: 256,
            keepalive_interval_secs: 30,
        }
    }
}

/// SSE query parameters.
#[derive(Debug, Deserialize)]
pub struct SseQuery {
    /// Authentication token.
    pub token: Option<String>,
}

struct SseSessionData {
    auth_context: AuthContext,
    session_secret: String,
    /// Maps client subscription ID -> internal SubscriptionId
    subscriptions: HashMap<String, SubscriptionId>,
}

/// State for SSE handler.
#[derive(Clone)]
pub struct SseState {
    reactor: Arc<Reactor>,
    auth_middleware: Arc<AuthMiddleware>,
    /// Per-session data: auth context and subscription mappings
    sessions: Arc<RwLock<HashMap<SessionId, SseSessionData>>>,
    config: SseConfig,
}

impl SseState {
    /// Create new SSE state with default config.
    pub fn new(reactor: Arc<Reactor>, auth_middleware: Arc<AuthMiddleware>) -> Self {
        Self::with_config(reactor, auth_middleware, SseConfig::default())
    }

    /// Create new SSE state with custom config.
    pub fn with_config(
        reactor: Arc<Reactor>,
        auth_middleware: Arc<AuthMiddleware>,
        config: SseConfig,
    ) -> Self {
        Self {
            reactor,
            auth_middleware,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    /// Check if we can accept new sessions.
    pub async fn can_accept_session(&self) -> bool {
        self.sessions.read().await.len() < self.config.max_sessions
    }
}

/// Guard to ensure session cleanup on disconnect.
/// Spawns a cleanup task on drop to handle abrupt disconnects.
struct SessionCleanupGuard {
    session_id: SessionId,
    reactor: Arc<Reactor>,
    sessions: Arc<RwLock<HashMap<SessionId, SseSessionData>>>,
    dropped: bool,
}

impl SessionCleanupGuard {
    fn new(
        session_id: SessionId,
        reactor: Arc<Reactor>,
        sessions: Arc<RwLock<HashMap<SessionId, SseSessionData>>>,
    ) -> Self {
        Self {
            session_id,
            reactor,
            sessions,
            dropped: false,
        }
    }

    /// Mark as cleanly closed (cleanup will be skipped).
    fn mark_closed(&mut self) {
        self.dropped = true;
    }
}

impl Drop for SessionCleanupGuard {
    fn drop(&mut self) {
        if self.dropped {
            return;
        }
        let session_id = self.session_id;
        let reactor = self.reactor.clone();
        let sessions = self.sessions.clone();

        // Spawn cleanup task since we can't await in drop
        // Use spawn to handle cleanup even if the runtime is shutting down
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                reactor.remove_session(session_id).await;
                sessions.write().await.remove(&session_id);
                tracing::debug!(%session_id, "SSE session cleaned up on disconnect");
            });
        } else {
            // Runtime not available, likely shutting down. Session will be cleaned up on restart.
            tracing::warn!(%session_id, "Could not spawn cleanup task, runtime unavailable");
        }
    }
}

/// SSE event payload sent to clients.
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SsePayload {
    /// Subscription data update.
    Update {
        target: String,
        payload: serde_json::Value,
    },
    /// Error message.
    Error {
        target: String,
        code: String,
        message: String,
    },
    /// Connection acknowledged.
    Connected {
        session_id: String,
        session_secret: String,
    },
}

/// Internal message type for SSE stream.
#[derive(Debug)]
pub enum SseMessage {
    Data {
        target: String,
        payload: serde_json::Value,
    },
    Error {
        target: String,
        code: String,
        message: String,
    },
}

/// SSE subscribe request.
#[derive(Debug, Deserialize)]
pub struct SseSubscribeRequest {
    pub session_id: String,
    pub session_secret: String,
    pub id: String,
    pub function: String,
    #[serde(default)]
    pub args: serde_json::Value,
}

/// SSE unsubscribe request.
#[derive(Debug, Deserialize)]
pub struct SseUnsubscribeRequest {
    pub session_id: String,
    pub session_secret: String,
    pub id: String,
}

/// SSE job subscribe request.
#[derive(Debug, Deserialize)]
pub struct SseJobSubscribeRequest {
    pub session_id: String,
    pub session_secret: String,
    pub id: String,
    pub job_id: String,
}

/// SSE workflow subscribe request.
#[derive(Debug, Deserialize)]
pub struct SseWorkflowSubscribeRequest {
    pub session_id: String,
    pub session_secret: String,
    pub id: String,
    pub workflow_id: String,
}

/// SSE error response.
#[derive(Debug, Serialize)]
pub struct SseError {
    pub code: String,
    pub message: String,
}

/// SSE subscribe response.
#[derive(Debug, Serialize)]
pub struct SseSubscribeResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<SseError>,
}

/// SSE unsubscribe response.
#[derive(Debug, Serialize)]
pub struct SseUnsubscribeResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<SseError>,
}

/// Create an SSE subscribe error response.
fn subscribe_error(
    status: StatusCode,
    code: impl Into<String>,
    message: impl Into<String>,
) -> (StatusCode, Json<SseSubscribeResponse>) {
    (
        status,
        Json(SseSubscribeResponse {
            success: false,
            data: None,
            error: Some(SseError {
                code: code.into(),
                message: message.into(),
            }),
        }),
    )
}

/// Create an SSE unsubscribe error response.
fn unsubscribe_error(
    status: StatusCode,
    code: impl Into<String>,
    message: impl Into<String>,
) -> (StatusCode, Json<SseUnsubscribeResponse>) {
    (
        status,
        Json(SseUnsubscribeResponse {
            success: false,
            error: Some(SseError {
                code: code.into(),
                message: message.into(),
            }),
        }),
    )
}

/// SSE handler for GET /events.
pub async fn sse_handler(
    State(state): State<Arc<SseState>>,
    Query(query): Query<SseQuery>,
) -> impl IntoResponse {
    // Check session limit
    if !state.can_accept_session().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "Server at capacity".to_string(),
        )
            .into_response();
    }

    let session_id = SessionId::new();
    let buffer_size = state.config.channel_buffer_size;
    let keepalive_secs = state.config.keepalive_interval_secs;
    let (tx, mut rx) = mpsc::channel::<SseMessage>(buffer_size);
    let cancel_token = CancellationToken::new();

    let auth_context = if let Some(token) = &query.token {
        match state.auth_middleware.validate_token_async(token).await {
            Ok(claims) => super::auth::build_auth_context_from_claims(claims),
            Err(e) => {
                tracing::warn!("SSE token validation failed: {}", e);
                return (
                    StatusCode::UNAUTHORIZED,
                    "Invalid authentication token".to_string(),
                )
                    .into_response();
            }
        }
    } else {
        forge_core::function::AuthContext::unauthenticated()
    };
    let session_secret = uuid::Uuid::new_v4().to_string();

    // Register session with reactor
    let reactor = state.reactor.clone();
    let cancel = cancel_token.clone();

    // Create a bridge channel for the reactor's message format
    let (rt_tx, mut rt_rx) = mpsc::channel(buffer_size);
    reactor.register_session(session_id, rt_tx);

    // Store session data for subscription handlers
    {
        let mut sessions = state.sessions.write().await;
        sessions.insert(
            session_id,
            SseSessionData {
                auth_context: auth_context.clone(),
                session_secret: session_secret.clone(),
                subscriptions: HashMap::new(),
            },
        );
    }

    // Capture sessions for cleanup guard
    let sessions = state.sessions.clone();

    // Create cleanup guard - will clean up on drop if stream ends unexpectedly
    let cleanup_guard = SessionCleanupGuard::new(session_id, reactor.clone(), sessions.clone());

    // Bridge reactor messages to SSE messages
    let bridge_cancel = cancel_token.clone();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                msg = rt_rx.recv() => {
                    match msg {
                        Some(rt_msg) => {
                            if let Some(sse_msg) = convert_realtime_to_sse(rt_msg)
                                && tx.send(sse_msg).await.is_err() {
                                    break;
                                }
                        }
                        None => break,
                    }
                }
                _ = bridge_cancel.cancelled() => break,
            }
        }
    });

    // Spawn a task that feeds SSE events into a channel
    let (event_tx, event_rx) = mpsc::channel::<Result<Event, Infallible>>(buffer_size);

    tokio::spawn(async move {
        let mut _guard = cleanup_guard;

        // Send connected event
        let connected = SsePayload::Connected {
            session_id: session_id.to_string(),
            session_secret: session_secret.clone(),
        };
        match serde_json::to_string(&connected) {
            Ok(json) => {
                let _ = event_tx
                    .send(Ok(Event::default().event("connected").data(json)))
                    .await;
            }
            Err(e) => {
                tracing::error!("Failed to serialize SSE connected payload: {}", e);
            }
        }

        loop {
            tokio::select! {
                msg = rx.recv() => {
                    match msg {
                        Some(sse_msg) => {
                            let event = match sse_msg {
                                SseMessage::Data { target, payload } => {
                                    let data = SsePayload::Update { target, payload };
                                    serde_json::to_string(&data).ok().map(|json| {
                                        Event::default().event("update").data(json)
                                    })
                                }
                                SseMessage::Error { target, code, message } => {
                                    let data = SsePayload::Error { target, code, message };
                                    serde_json::to_string(&data).ok().map(|json| {
                                        Event::default().event("error").data(json)
                                    })
                                }
                            };
                            if let Some(evt) = event
                                && event_tx.send(Ok(evt)).await.is_err()
                            {
                                break;
                            }
                        }
                        None => break,
                    }
                }
                _ = cancel.cancelled() => break,
            }
        }

        // Clean shutdown
        _guard.mark_closed();
        reactor.remove_session(session_id).await;
        sessions.write().await.remove(&session_id);
    });

    let stream = ReceiverStream { rx: event_rx };

    Sse::new(stream)
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(keepalive_secs))
                .text("ping"),
        )
        .into_response()
}

/// Convert realtime message to SSE message.
fn convert_realtime_to_sse(msg: RealtimeMessage) -> Option<SseMessage> {
    match msg {
        RealtimeMessage::Data {
            subscription_id,
            data,
        } => Some(SseMessage::Data {
            target: format!("sub:{}", subscription_id),
            payload: data,
        }),
        RealtimeMessage::DeltaUpdate {
            subscription_id,
            delta,
        } => match serde_json::to_value(&delta) {
            Ok(payload) => Some(SseMessage::Data {
                target: format!("sub:{}", subscription_id),
                payload,
            }),
            Err(e) => {
                tracing::error!("Failed to serialize delta update: {}", e);
                Some(SseMessage::Error {
                    target: format!("sub:{}", subscription_id),
                    code: "SERIALIZATION_ERROR".to_string(),
                    message: "Failed to serialize update data".to_string(),
                })
            }
        },
        RealtimeMessage::JobUpdate { client_sub_id, job } => match serde_json::to_value(&job) {
            Ok(payload) => Some(SseMessage::Data {
                target: format!("job:{}", client_sub_id),
                payload,
            }),
            Err(e) => {
                tracing::error!("Failed to serialize job update: {}", e);
                Some(SseMessage::Error {
                    target: format!("job:{}", client_sub_id),
                    code: "SERIALIZATION_ERROR".to_string(),
                    message: "Failed to serialize job update".to_string(),
                })
            }
        },
        RealtimeMessage::WorkflowUpdate {
            client_sub_id,
            workflow,
        } => match serde_json::to_value(&workflow) {
            Ok(payload) => Some(SseMessage::Data {
                target: format!("wf:{}", client_sub_id),
                payload,
            }),
            Err(e) => {
                tracing::error!("Failed to serialize workflow update: {}", e);
                Some(SseMessage::Error {
                    target: format!("wf:{}", client_sub_id),
                    code: "SERIALIZATION_ERROR".to_string(),
                    message: "Failed to serialize workflow update".to_string(),
                })
            }
        },
        RealtimeMessage::Error { code, message } => Some(SseMessage::Error {
            target: String::new(),
            code,
            message,
        }),
        RealtimeMessage::ErrorWithId { id, code, message } => Some(SseMessage::Error {
            target: id,
            code,
            message,
        }),
        // Ignore control messages
        RealtimeMessage::Subscribe { .. }
        | RealtimeMessage::Unsubscribe { .. }
        | RealtimeMessage::Ping
        | RealtimeMessage::Pong
        | RealtimeMessage::AuthSuccess
        | RealtimeMessage::AuthFailed { .. }
        | RealtimeMessage::Lagging => None,
    }
}

/// SSE subscribe handler for POST /subscribe.
pub async fn sse_subscribe_handler(
    State(state): State<Arc<SseState>>,
    Extension(request_auth): Extension<AuthContext>,
    Json(request): Json<SseSubscribeRequest>,
) -> impl IntoResponse {
    // Validate subscription ID length to prevent memory bloat
    if request.id.len() > MAX_CLIENT_SUB_ID_LEN {
        return subscribe_error(
            StatusCode::BAD_REQUEST,
            "INVALID_ID",
            format!(
                "Subscription ID too long (max {} chars)",
                MAX_CLIENT_SUB_ID_LEN
            ),
        );
    }

    let Some(session_id) = try_parse_session_id(&request.session_id) else {
        return subscribe_error(
            StatusCode::BAD_REQUEST,
            "INVALID_SESSION",
            "Invalid session ID format",
        );
    };

    // Get session data (auth context)
    let sessions = state.sessions.read().await;
    let session_data = match sessions.get(&session_id) {
        Some(data) => {
            match authorize_session_access(data, &request.session_secret, &request_auth) {
                Ok(auth) => auth,
                Err(resp) => return resp,
            }
        }
        None => {
            return subscribe_error(
                StatusCode::NOT_FOUND,
                "SESSION_NOT_FOUND",
                "Session not found or expired",
            );
        }
    };
    drop(sessions);

    // Subscribe via reactor
    let result = state
        .reactor
        .subscribe(
            session_id,
            request.id.clone(),
            request.function,
            request.args,
            session_data,
        )
        .await;

    match result {
        Ok((subscription_id, data)) => {
            // Store the subscription mapping
            let mut sessions = state.sessions.write().await;
            match sessions.get_mut(&session_id) {
                Some(session) => {
                    session.subscriptions.insert(request.id, subscription_id);
                }
                None => {
                    // Session was removed between read and write lock
                    return subscribe_error(
                        StatusCode::NOT_FOUND,
                        "SESSION_NOT_FOUND",
                        "Session expired during subscription",
                    );
                }
            }

            tracing::debug!(
                %session_id,
                %subscription_id,
                "SSE subscription registered"
            );

            (
                StatusCode::OK,
                Json(SseSubscribeResponse {
                    success: true,
                    data: Some(data),
                    error: None,
                }),
            )
        }
        Err(e) => {
            tracing::warn!(%session_id, error = %e, "SSE subscription failed");
            match e {
                forge_core::ForgeError::Unauthorized(msg) => {
                    subscribe_error(StatusCode::UNAUTHORIZED, "UNAUTHORIZED", msg)
                }
                forge_core::ForgeError::Forbidden(msg) => {
                    subscribe_error(StatusCode::FORBIDDEN, "FORBIDDEN", msg)
                }
                forge_core::ForgeError::InvalidArgument(msg)
                | forge_core::ForgeError::Validation(msg) => {
                    subscribe_error(StatusCode::BAD_REQUEST, "INVALID_ARGUMENT", msg)
                }
                forge_core::ForgeError::NotFound(msg) => {
                    subscribe_error(StatusCode::NOT_FOUND, "NOT_FOUND", msg)
                }
                _ => subscribe_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "SUBSCRIPTION_FAILED",
                    "Subscription failed",
                ),
            }
        }
    }
}

/// SSE unsubscribe handler for POST /unsubscribe.
pub async fn sse_unsubscribe_handler(
    State(state): State<Arc<SseState>>,
    Extension(request_auth): Extension<AuthContext>,
    Json(request): Json<SseUnsubscribeRequest>,
) -> impl IntoResponse {
    let Some(session_id) = try_parse_session_id(&request.session_id) else {
        return unsubscribe_error(
            StatusCode::BAD_REQUEST,
            "INVALID_SESSION",
            "Invalid session ID format",
        );
    };

    // Look up internal subscription ID and validate session ownership
    let subscription_id = {
        let sessions = state.sessions.read().await;
        match sessions.get(&session_id) {
            Some(session) => {
                if session.session_secret != request.session_secret
                    || !same_principal(&session.auth_context, &request_auth)
                {
                    return unsubscribe_error(
                        StatusCode::FORBIDDEN,
                        "SESSION_PRINCIPAL_MISMATCH",
                        "Request principal does not match session principal",
                    );
                }
                session.subscriptions.get(&request.id).copied()
            }
            None => None,
        }
    };

    let Some(subscription_id) = subscription_id else {
        return unsubscribe_error(
            StatusCode::NOT_FOUND,
            "SUBSCRIPTION_NOT_FOUND",
            "Subscription not found",
        );
    };

    // Unsubscribe via reactor
    state.reactor.unsubscribe(subscription_id);

    // Remove from session tracking
    {
        let mut sessions = state.sessions.write().await;
        if let Some(session) = sessions.get_mut(&session_id) {
            session.subscriptions.remove(&request.id);
        }
    }

    tracing::debug!(
        %session_id,
        %subscription_id,
        "SSE subscription removed"
    );

    (
        StatusCode::OK,
        Json(SseUnsubscribeResponse {
            success: true,
            error: None,
        }),
    )
}

/// SSE job subscribe handler for POST /subscribe-job.
pub async fn sse_job_subscribe_handler(
    State(state): State<Arc<SseState>>,
    Extension(request_auth): Extension<AuthContext>,
    Json(request): Json<SseJobSubscribeRequest>,
) -> impl IntoResponse {
    if request.id.len() > MAX_CLIENT_SUB_ID_LEN {
        return subscribe_error(
            StatusCode::BAD_REQUEST,
            "INVALID_ID",
            format!(
                "Subscription ID too long (max {} chars)",
                MAX_CLIENT_SUB_ID_LEN
            ),
        );
    }

    let Some(session_id) = try_parse_session_id(&request.session_id) else {
        return subscribe_error(
            StatusCode::BAD_REQUEST,
            "INVALID_SESSION",
            "Invalid session ID format",
        );
    };

    // Validate session exists + principal binding
    let session_auth = {
        let sessions = state.sessions.read().await;
        match sessions.get(&session_id) {
            Some(session) => {
                match authorize_session_access(session, &request.session_secret, &request_auth) {
                    Ok(auth) => auth,
                    Err(resp) => return resp,
                }
            }
            None => {
                return subscribe_error(
                    StatusCode::NOT_FOUND,
                    "SESSION_NOT_FOUND",
                    "Session not found or expired",
                );
            }
        }
    };

    // Parse job ID
    let job_uuid = match uuid::Uuid::parse_str(&request.job_id) {
        Ok(uuid) => uuid,
        Err(_) => {
            return subscribe_error(
                StatusCode::BAD_REQUEST,
                "INVALID_JOB_ID",
                "Invalid job ID format",
            );
        }
    };

    // Subscribe to job updates via reactor
    match state
        .reactor
        .subscribe_job(session_id, request.id.clone(), job_uuid, &session_auth)
        .await
    {
        Ok(job_data) => {
            let data = match serde_json::to_value(&job_data) {
                Ok(v) => v,
                Err(e) => {
                    tracing::error!("Failed to serialize job data: {}", e);
                    return subscribe_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "SERIALIZE_ERROR",
                        "Failed to serialize job data",
                    );
                }
            };
            tracing::debug!(
                %session_id,
                job_id = %request.job_id,
                client_sub_id = %request.id,
                "SSE job subscription registered"
            );
            (
                StatusCode::OK,
                Json(SseSubscribeResponse {
                    success: true,
                    data: Some(data),
                    error: None,
                }),
            )
        }
        Err(e) => match e {
            forge_core::ForgeError::Unauthorized(msg) => {
                subscribe_error(StatusCode::UNAUTHORIZED, "UNAUTHORIZED", msg)
            }
            forge_core::ForgeError::Forbidden(msg) => {
                subscribe_error(StatusCode::FORBIDDEN, "FORBIDDEN", msg)
            }
            forge_core::ForgeError::NotFound(msg) => {
                subscribe_error(StatusCode::NOT_FOUND, "JOB_NOT_FOUND", msg)
            }
            _ => subscribe_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "SUBSCRIPTION_FAILED",
                "Subscription failed",
            ),
        },
    }
}

/// SSE workflow subscribe handler for POST /subscribe-workflow.
pub async fn sse_workflow_subscribe_handler(
    State(state): State<Arc<SseState>>,
    Extension(request_auth): Extension<AuthContext>,
    Json(request): Json<SseWorkflowSubscribeRequest>,
) -> impl IntoResponse {
    if request.id.len() > MAX_CLIENT_SUB_ID_LEN {
        return subscribe_error(
            StatusCode::BAD_REQUEST,
            "INVALID_ID",
            format!(
                "Subscription ID too long (max {} chars)",
                MAX_CLIENT_SUB_ID_LEN
            ),
        );
    }

    let Some(session_id) = try_parse_session_id(&request.session_id) else {
        return subscribe_error(
            StatusCode::BAD_REQUEST,
            "INVALID_SESSION",
            "Invalid session ID format",
        );
    };

    // Validate session exists + principal binding
    let session_auth = {
        let sessions = state.sessions.read().await;
        match sessions.get(&session_id) {
            Some(session) => {
                match authorize_session_access(session, &request.session_secret, &request_auth) {
                    Ok(auth) => auth,
                    Err(resp) => return resp,
                }
            }
            None => {
                return subscribe_error(
                    StatusCode::NOT_FOUND,
                    "SESSION_NOT_FOUND",
                    "Session not found or expired",
                );
            }
        }
    };

    // Parse workflow ID
    let workflow_uuid = match uuid::Uuid::parse_str(&request.workflow_id) {
        Ok(uuid) => uuid,
        Err(_) => {
            return subscribe_error(
                StatusCode::BAD_REQUEST,
                "INVALID_WORKFLOW_ID",
                "Invalid workflow ID format",
            );
        }
    };

    // Subscribe to workflow updates via reactor
    match state
        .reactor
        .subscribe_workflow(session_id, request.id.clone(), workflow_uuid, &session_auth)
        .await
    {
        Ok(workflow_data) => {
            let data = match serde_json::to_value(&workflow_data) {
                Ok(v) => v,
                Err(e) => {
                    tracing::error!("Failed to serialize workflow data: {}", e);
                    return subscribe_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "SERIALIZE_ERROR",
                        "Failed to serialize workflow data",
                    );
                }
            };
            tracing::debug!(
                %session_id,
                workflow_id = %request.workflow_id,
                client_sub_id = %request.id,
                "SSE workflow subscription registered"
            );
            (
                StatusCode::OK,
                Json(SseSubscribeResponse {
                    success: true,
                    data: Some(data),
                    error: None,
                }),
            )
        }
        Err(e) => match e {
            forge_core::ForgeError::Unauthorized(msg) => {
                subscribe_error(StatusCode::UNAUTHORIZED, "UNAUTHORIZED", msg)
            }
            forge_core::ForgeError::Forbidden(msg) => {
                subscribe_error(StatusCode::FORBIDDEN, "FORBIDDEN", msg)
            }
            forge_core::ForgeError::NotFound(msg) => {
                subscribe_error(StatusCode::NOT_FOUND, "WORKFLOW_NOT_FOUND", msg)
            }
            _ => subscribe_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "SUBSCRIPTION_FAILED",
                "Subscription failed",
            ),
        },
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_sse_payload_serialization() {
        let payload = SsePayload::Update {
            target: "sub:123".to_string(),
            payload: serde_json::json!({"id": 1}),
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"type\":\"update\""));
        assert!(json.contains("\"target\":\"sub:123\""));
    }

    #[test]
    fn test_sse_error_serialization() {
        let payload = SsePayload::Error {
            target: "sub:456".to_string(),
            code: "NOT_FOUND".to_string(),
            message: "Subscription not found".to_string(),
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"type\":\"error\""));
        assert!(json.contains("NOT_FOUND"));
    }
}
