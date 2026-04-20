//! Process-wide signal emission helpers for background executions.
//!
//! The RPC path threads a [`SignalsCollector`] through [`FunctionExecutor`]
//! directly, but jobs, crons, workflows, daemons, webhooks, and middleware
//! don't have that plumbing. This module lets them emit signals without
//! changing their call signatures.
//!
//! Callers use the thin helpers below ([`emit_server_execution`],
//! [`emit_diagnostic`], [`emit_raw`]). Emission is a no-op when signals are
//! disabled or no global collector has been installed, so handlers don't
//! need to branch on configuration.

use std::sync::OnceLock;

use forge_core::signals::{SignalEvent, SignalEventType};
use tracing::debug;

use super::collector::SignalsCollector;

/// Installed once at gateway startup. `None` means signals are disabled.
static GLOBAL_COLLECTOR: OnceLock<Option<SignalsCollector>> = OnceLock::new();

/// Install the process-wide collector. Idempotent: the first installation
/// wins (subsequent calls are no-ops). Pass `None` to explicitly disable.
pub fn install(collector: Option<SignalsCollector>) {
    if GLOBAL_COLLECTOR.set(collector).is_err() {
        debug!("signals::emit global collector already installed");
    }
}

/// Check if a collector is installed and active.
pub fn is_installed() -> bool {
    matches!(GLOBAL_COLLECTOR.get(), Some(Some(_)))
}

fn collector() -> Option<&'static SignalsCollector> {
    GLOBAL_COLLECTOR.get().and_then(|c| c.as_ref())
}

/// Emit a pre-built event. No-op if no collector is installed.
pub fn emit_raw(event: SignalEvent) {
    if let Some(c) = collector() {
        c.try_send(event);
    }
}

/// Emit a server-initiated execution event (job/cron/workflow/webhook/daemon).
///
/// `kind` values: `"job"`, `"cron"`, `"workflow"`, `"workflow_step"`,
/// `"webhook"`, `"daemon"`. `name` is the function/handler name.
pub fn emit_server_execution(
    name: &str,
    kind: &str,
    duration_ms: i32,
    success: bool,
    error_message: Option<String>,
) {
    if let Some(c) = collector() {
        c.try_send(SignalEvent::server_execution(
            name,
            kind,
            duration_ms,
            success,
            error_message,
        ));
    }
}

/// Emit a diagnostic event (auth failure, rate limit hit, panic, etc.) with
/// optional request context. Stored as a `track` event so it shows up in the
/// standard dashboards under the given event name.
#[allow(clippy::too_many_arguments)]
pub fn emit_diagnostic(
    event_name: &str,
    properties: serde_json::Value,
    client_ip: Option<String>,
    user_agent: Option<String>,
    visitor_id: Option<String>,
    user_id: Option<uuid::Uuid>,
    is_bot: bool,
) {
    if let Some(c) = collector() {
        c.try_send(SignalEvent::diagnostic(
            event_name, properties, client_ip, user_agent, visitor_id, user_id, is_bot,
        ));
    }
}

/// Emit a Web Vitals / browser-perf measurement ingested from a client.
#[allow(clippy::too_many_arguments)]
pub fn emit_web_vital(
    name: &str,
    value: f64,
    rating: Option<String>,
    attribution: serde_json::Value,
    page_url: Option<String>,
    correlation_id: Option<String>,
    session_id: Option<uuid::Uuid>,
    visitor_id: Option<String>,
    user_id: Option<uuid::Uuid>,
    tenant_id: Option<uuid::Uuid>,
    client_ip: Option<String>,
    user_agent: Option<String>,
    device_type: Option<String>,
    browser: Option<String>,
    os: Option<String>,
    is_bot: bool,
    timestamp: Option<chrono::DateTime<chrono::Utc>>,
) {
    let Some(c) = collector() else {
        return;
    };
    // Store the numeric value in properties so dashboards can aggregate.
    // Also copy it into duration_ms for timing vitals so existing views work.
    let mut props = serde_json::Map::new();
    props.insert(
        "value".to_string(),
        serde_json::Number::from_f64(value)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
    );
    if let Some(r) = rating.clone() {
        props.insert("rating".to_string(), serde_json::Value::String(r));
    }
    if !attribution.is_null() {
        props.insert("attribution".to_string(), attribution);
    }
    let duration_ms = if value.is_finite() && value >= 0.0 && value <= i32::MAX as f64 {
        Some(value.round() as i32)
    } else {
        None
    };
    c.try_send(SignalEvent {
        event_type: SignalEventType::WebVital,
        event_name: Some(name.to_string()),
        correlation_id,
        session_id,
        visitor_id,
        user_id,
        tenant_id,
        properties: serde_json::Value::Object(props),
        page_url,
        referrer: None,
        function_name: None,
        function_kind: None,
        duration_ms,
        status: rating,
        error_message: None,
        error_stack: None,
        error_context: None,
        client_ip,
        country: None,
        city: None,
        user_agent,
        device_type,
        browser,
        os,
        utm: None,
        is_bot,
        timestamp: timestamp.unwrap_or_else(chrono::Utc::now),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn emit_is_noop_without_installed_collector() {
        // On a fresh process, GLOBAL_COLLECTOR may be unset. These calls
        // should simply not panic. We don't touch GLOBAL_COLLECTOR here so
        // other tests in this crate aren't affected.
        emit_server_execution("not_installed", "job", 10, true, None);
        emit_diagnostic(
            "not_installed",
            serde_json::json!({}),
            None,
            None,
            None,
            None,
            false,
        );
        emit_raw(SignalEvent::server_execution("x", "job", 1, true, None));
    }
}
