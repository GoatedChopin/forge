//! HTTP ingestion endpoints for client-side signal events.
//!
//! Routes (under /_api/):
//! - POST /signal/event  -- custom events
//! - POST /signal/view   -- page views
//! - POST /signal/user   -- identify (link session to user)
//! - POST /signal/report -- diagnostic error reports

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use forge_core::AuthContext;
use forge_core::signals::{
    DiagnosticReport, IdentifyPayload, PageViewPayload, SignalEvent, SignalEventBatch,
    SignalEventType, SignalResponse, UtmParams,
};
use serde_json::Value;
use sqlx::PgPool;
use tracing::warn;
use uuid::Uuid;

use super::bot;
use super::collector::SignalsCollector;
use super::device;
use super::session;
use super::visitor;

/// Maximum events per batch request.
const MAX_BATCH_SIZE: usize = 50;

/// Shared state for signal endpoints.
#[derive(Clone)]
pub struct SignalsState {
    pub collector: SignalsCollector,
    pub pool: PgPool,
    pub server_secret: String,
}

/// POST /signal/event -- batch custom events.
pub async fn event_handler(
    State(state): State<Arc<SignalsState>>,
    auth: Option<axum::Extension<AuthContext>>,
    headers: HeaderMap,
    Json(batch): Json<SignalEventBatch>,
) -> impl IntoResponse {
    if batch.events.len() > MAX_BATCH_SIZE {
        return Json(SignalResponse {
            ok: false,
            session_id: None,
        });
    }

    let ctx = extract_request_ctx(&headers, &auth, &state.server_secret);
    let session_id =
        resolve_session_id(batch.context.as_ref().and_then(|c| c.session_id.as_deref()));
    let page_url = batch.context.as_ref().and_then(|c| c.page_url.clone());

    let session_id = session::upsert_session(
        &state.pool,
        session_id,
        &ctx.visitor_id,
        ctx.user_id,
        ctx.tenant_id,
        page_url.as_deref(),
        batch.context.as_ref().and_then(|c| c.referrer.as_deref()),
        ctx.user_agent.as_deref(),
        ctx.client_ip.as_deref(),
        ctx.is_bot,
        "track",
        ctx.device_type.as_deref(),
        ctx.browser.as_deref(),
        ctx.os.as_deref(),
    )
    .await;

    for event in batch.events {
        let signal = SignalEvent {
            event_type: SignalEventType::Track,
            event_name: Some(event.event),
            correlation_id: event.correlation_id,
            session_id,
            visitor_id: Some(ctx.visitor_id.clone()),
            user_id: ctx.user_id,
            tenant_id: ctx.tenant_id,
            properties: event.properties,
            page_url: page_url.clone(),
            referrer: None,
            function_name: None,
            function_kind: None,
            duration_ms: None,
            status: None,
            error_message: None,
            error_stack: None,
            error_context: None,
            client_ip: ctx.client_ip.clone(),
            user_agent: ctx.user_agent.clone(),
            device_type: ctx.device_type.clone(),
            browser: ctx.browser.clone(),
            os: ctx.os.clone(),
            utm: None,
            is_bot: ctx.is_bot,
            timestamp: event.timestamp.unwrap_or_else(chrono::Utc::now),
        };
        state.collector.try_send(signal);
    }

    Json(SignalResponse {
        ok: true,
        session_id,
    })
}

/// POST /signal/view -- page view event.
pub async fn view_handler(
    State(state): State<Arc<SignalsState>>,
    auth: Option<axum::Extension<AuthContext>>,
    headers: HeaderMap,
    Json(payload): Json<PageViewPayload>,
) -> impl IntoResponse {
    let ctx = extract_request_ctx(&headers, &auth, &state.server_secret);
    let session_id_header = extract_header(&headers, "x-session-id");
    let session_id = resolve_session_id(session_id_header.as_deref());

    let session_id = session::upsert_session(
        &state.pool,
        session_id,
        &ctx.visitor_id,
        ctx.user_id,
        ctx.tenant_id,
        Some(&payload.url),
        payload.referrer.as_deref(),
        ctx.user_agent.as_deref(),
        ctx.client_ip.as_deref(),
        ctx.is_bot,
        "page_view",
        ctx.device_type.as_deref(),
        ctx.browser.as_deref(),
        ctx.os.as_deref(),
    )
    .await;

    let utm = if payload.utm_source.is_some()
        || payload.utm_medium.is_some()
        || payload.utm_campaign.is_some()
    {
        Some(UtmParams {
            source: payload.utm_source,
            medium: payload.utm_medium,
            campaign: payload.utm_campaign,
            term: payload.utm_term,
            content: payload.utm_content,
        })
    } else {
        None
    };

    let signal = SignalEvent {
        event_type: SignalEventType::PageView,
        event_name: payload.title,
        correlation_id: payload.correlation_id,
        session_id,
        visitor_id: Some(ctx.visitor_id),
        user_id: ctx.user_id,
        tenant_id: ctx.tenant_id,
        properties: Value::Object(serde_json::Map::new()),
        page_url: Some(payload.url),
        referrer: payload.referrer,
        function_name: None,
        function_kind: None,
        duration_ms: None,
        status: None,
        error_message: None,
        error_stack: None,
        error_context: None,
        client_ip: ctx.client_ip,
        user_agent: ctx.user_agent,
        device_type: ctx.device_type,
        browser: ctx.browser,
        os: ctx.os,
        utm,
        is_bot: ctx.is_bot,
        timestamp: chrono::Utc::now(),
    };
    state.collector.try_send(signal);

    Json(SignalResponse {
        ok: true,
        session_id,
    })
}

/// POST /signal/user -- identify user.
pub async fn user_handler(
    State(state): State<Arc<SignalsState>>,
    auth: Option<axum::Extension<AuthContext>>,
    headers: HeaderMap,
    Json(payload): Json<IdentifyPayload>,
) -> impl IntoResponse {
    let user_id = Uuid::parse_str(&payload.user_id).ok().or_else(|| {
        warn!(raw_id = %payload.user_id, "identify called with non-UUID user_id, ignoring");
        None
    });

    let Some(user_id) = user_id else {
        return Json(SignalResponse {
            ok: false,
            session_id: None,
        });
    };

    let session_id_header = extract_header(&headers, "x-session-id");
    let session_id = resolve_session_id(session_id_header.as_deref());

    if let Some(sid) = session_id {
        session::identify_session(&state.pool, sid, user_id).await;
    }

    let referrer: Option<&str> = None;
    session::upsert_user(
        &state.pool,
        user_id,
        &payload.traits,
        referrer,
        None,
        None,
        None,
    )
    .await;

    let ctx = extract_request_ctx(&headers, &auth, &state.server_secret);

    let signal = SignalEvent {
        event_type: SignalEventType::Identify,
        event_name: None,
        correlation_id: None,
        session_id,
        visitor_id: Some(ctx.visitor_id),
        user_id: Some(user_id),
        tenant_id: ctx.tenant_id,
        properties: payload.traits,
        page_url: None,
        referrer: None,
        function_name: None,
        function_kind: None,
        duration_ms: None,
        status: None,
        error_message: None,
        error_stack: None,
        error_context: None,
        client_ip: ctx.client_ip,
        user_agent: ctx.user_agent,
        device_type: ctx.device_type,
        browser: ctx.browser,
        os: ctx.os,
        utm: None,
        is_bot: ctx.is_bot,
        timestamp: chrono::Utc::now(),
    };
    state.collector.try_send(signal);

    Json(SignalResponse {
        ok: true,
        session_id,
    })
}

/// POST /signal/report -- diagnostic error reports.
pub async fn report_handler(
    State(state): State<Arc<SignalsState>>,
    auth: Option<axum::Extension<AuthContext>>,
    headers: HeaderMap,
    Json(report): Json<DiagnosticReport>,
) -> impl IntoResponse {
    if report.errors.len() > MAX_BATCH_SIZE {
        return Json(SignalResponse {
            ok: false,
            session_id: None,
        });
    }

    let ctx = extract_request_ctx(&headers, &auth, &state.server_secret);
    let session_id_header = extract_header(&headers, "x-session-id");
    let session_id = resolve_session_id(session_id_header.as_deref());

    if let Some(sid) = session_id {
        session::upsert_session(
            &state.pool,
            Some(sid),
            &ctx.visitor_id,
            ctx.user_id,
            ctx.tenant_id,
            None,
            None,
            ctx.user_agent.as_deref(),
            ctx.client_ip.as_deref(),
            ctx.is_bot,
            "error",
            ctx.device_type.as_deref(),
            ctx.browser.as_deref(),
            ctx.os.as_deref(),
        )
        .await;
    }

    for err in report.errors {
        let signal = SignalEvent {
            event_type: SignalEventType::Error,
            event_name: Some(err.message.clone()),
            correlation_id: err.correlation_id,
            session_id,
            visitor_id: Some(ctx.visitor_id.clone()),
            user_id: ctx.user_id,
            tenant_id: ctx.tenant_id,
            properties: Value::Object(serde_json::Map::new()),
            page_url: err.page_url,
            referrer: None,
            function_name: None,
            function_kind: None,
            duration_ms: None,
            status: None,
            error_message: Some(err.message),
            error_stack: err.stack,
            error_context: err.context,
            client_ip: ctx.client_ip.clone(),
            user_agent: ctx.user_agent.clone(),
            device_type: ctx.device_type.clone(),
            browser: ctx.browser.clone(),
            os: ctx.os.clone(),
            utm: None,
            is_bot: ctx.is_bot,
            timestamp: chrono::Utc::now(),
        };
        state.collector.try_send(signal);
    }

    Json(SignalResponse {
        ok: true,
        session_id,
    })
}

/// Shared request context extracted from headers and auth for all signal endpoints.
struct RequestCtx {
    user_agent: Option<String>,
    client_ip: Option<String>,
    is_bot: bool,
    visitor_id: String,
    user_id: Option<Uuid>,
    tenant_id: Option<Uuid>,
    device_type: Option<String>,
    browser: Option<String>,
    os: Option<String>,
}

fn extract_request_ctx(
    headers: &HeaderMap,
    auth: &Option<axum::Extension<AuthContext>>,
    server_secret: &str,
) -> RequestCtx {
    let user_agent = extract_header(headers, "user-agent");
    let platform_header = extract_header(headers, "x-forge-platform");
    let client_ip = extract_client_ip(headers);
    let ua_lower = user_agent
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let is_bot = bot::is_bot_lower(&ua_lower);
    let visitor_id =
        visitor::generate_visitor_id(client_ip.as_deref(), user_agent.as_deref(), server_secret);
    let user_id = auth.as_ref().and_then(|a| a.user_id());
    let tenant_id = auth.as_ref().and_then(|a| a.tenant_id());
    let device_info = device::parse_lowered(platform_header.as_deref(), &ua_lower);
    RequestCtx {
        user_agent,
        client_ip,
        is_bot,
        visitor_id,
        user_id,
        tenant_id,
        device_type: device_info.device_type,
        browser: device_info.browser,
        os: device_info.os,
    }
}

fn extract_header(headers: &HeaderMap, name: &str) -> Option<String> {
    crate::gateway::extract_header(headers, name)
}

fn extract_client_ip(headers: &HeaderMap) -> Option<String> {
    crate::gateway::extract_client_ip(headers)
}

fn resolve_session_id(raw: Option<&str>) -> Option<Uuid> {
    raw.and_then(|s| Uuid::parse_str(s).ok())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use axum::http::{HeaderMap, HeaderValue};
    use uuid::Uuid;

    use super::{extract_client_ip, extract_header, resolve_session_id};

    #[tokio::test]
    async fn extract_header_returns_value() {
        let mut headers = HeaderMap::new();
        headers.insert("x-custom", HeaderValue::from_static("hello"));
        assert_eq!(extract_header(&headers, "x-custom"), Some("hello".into()));
    }

    #[tokio::test]
    async fn extract_header_returns_none_for_missing() {
        let headers = HeaderMap::new();
        assert_eq!(extract_header(&headers, "x-custom"), None);
    }

    #[tokio::test]
    async fn extract_header_returns_none_for_empty_value() {
        let mut headers = HeaderMap::new();
        headers.insert("x-custom", HeaderValue::from_static(""));
        assert_eq!(extract_header(&headers, "x-custom"), None);
    }

    #[tokio::test]
    async fn extract_client_ip_from_forwarded_for_single() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", HeaderValue::from_static("1.2.3.4"));
        assert_eq!(extract_client_ip(&headers), Some("1.2.3.4".into()));
    }

    #[tokio::test]
    async fn extract_client_ip_from_forwarded_for_multiple() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("1.2.3.4, 5.6.7.8"),
        );
        assert_eq!(extract_client_ip(&headers), Some("1.2.3.4".into()));
    }

    #[tokio::test]
    async fn extract_client_ip_falls_back_to_real_ip() {
        let mut headers = HeaderMap::new();
        headers.insert("x-real-ip", HeaderValue::from_static("9.8.7.6"));
        assert_eq!(extract_client_ip(&headers), Some("9.8.7.6".into()));
    }

    #[tokio::test]
    async fn extract_client_ip_returns_none_when_no_headers() {
        let headers = HeaderMap::new();
        assert_eq!(extract_client_ip(&headers), None);
    }

    #[tokio::test]
    async fn resolve_session_id_parses_valid_uuid() {
        let raw = "550e8400-e29b-41d4-a716-446655440000";
        let expected = Uuid::parse_str(raw).unwrap();
        assert_eq!(resolve_session_id(Some(raw)), Some(expected));
    }

    #[tokio::test]
    async fn resolve_session_id_returns_none_for_garbage() {
        assert_eq!(resolve_session_id(Some("not-a-uuid")), None);
    }

    #[tokio::test]
    async fn resolve_session_id_returns_none_for_none() {
        assert_eq!(resolve_session_id(None), None);
    }
}
