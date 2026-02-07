//! Axum handler for webhook requests with signature validation.

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    Json,
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use forge_core::CircuitBreakerClient;
use forge_core::function::JobDispatch;
use forge_core::webhook::{IdempotencySource, SignatureAlgorithm, WebhookContext};
use hmac::{Hmac, Mac};
use serde_json::{Value, json};
use sha1::Sha1;
use sha2::{Sha256, Sha512};
use sqlx::PgPool;
use tracing::{error, info, warn};
use uuid::Uuid;

use super::registry::WebhookRegistry;

/// State for webhook handler.
#[derive(Clone)]
pub struct WebhookState {
    registry: Arc<WebhookRegistry>,
    pool: PgPool,
    http_client: CircuitBreakerClient,
    job_dispatcher: Option<Arc<dyn JobDispatch>>,
}

impl WebhookState {
    /// Create new webhook state.
    pub fn new(registry: Arc<WebhookRegistry>, pool: PgPool) -> Self {
        Self {
            registry,
            pool,
            http_client: CircuitBreakerClient::with_defaults(reqwest::Client::new()),
            job_dispatcher: None,
        }
    }

    /// Set job dispatcher.
    pub fn with_job_dispatcher(mut self, dispatcher: Arc<dyn JobDispatch>) -> Self {
        self.job_dispatcher = Some(dispatcher);
        self
    }
}

/// Handle webhook requests.
///
/// This handler:
/// 1. Looks up webhook by path
/// 2. Validates signature if configured
/// 3. Checks idempotency
/// 4. Executes handler
/// 5. Records idempotency key
pub async fn webhook_handler(
    State(state): State<Arc<WebhookState>>,
    Path(path): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let full_path = format!("/webhooks/{}", path);
    let request_id = Uuid::new_v4().to_string();

    // Look up webhook by path
    let entry = match state.registry.get_by_path(&full_path) {
        Some(e) => e,
        None => {
            warn!(path = %full_path, "Webhook not found");
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "Webhook not found"})),
            );
        }
    };

    let info = &entry.info;
    info!(
        webhook = info.name,
        path = %full_path,
        request_id = %request_id,
        "Webhook request received"
    );

    if info.signature.is_none() && !info.allow_unsigned {
        warn!(
            webhook = info.name,
            "Unsigned webhook rejected (set allow_unsigned to opt in)"
        );
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "Webhook signature is required"})),
        );
    }

    // Validate signature if configured
    if let Some(ref sig_config) = info.signature {
        // Get signature from header
        let signature = match headers
            .get(sig_config.header_name)
            .and_then(|v| v.to_str().ok())
        {
            Some(s) => s,
            None => {
                warn!(webhook = info.name, "Missing signature header");
                return (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({"error": "Missing signature"})),
                );
            }
        };

        // Get secret from environment
        let secret = match std::env::var(sig_config.secret_env) {
            Ok(s) => s,
            Err(_) => {
                error!(
                    webhook = info.name,
                    env = sig_config.secret_env,
                    "Webhook secret not configured"
                );
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": "Webhook configuration error"})),
                );
            }
        };

        // Validate signature
        if !validate_signature(sig_config.algorithm, &body, &secret, signature) {
            warn!(webhook = info.name, "Invalid signature");
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "Invalid signature"})),
            );
        }
    }

    // Extract idempotency key if configured
    let idempotency_key = if let Some(ref idem_config) = info.idempotency {
        match &idem_config.source {
            IdempotencySource::Header(header_name) => headers
                .get(*header_name)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string()),
            IdempotencySource::Body(json_path) => {
                // Parse body and extract value using JSON path
                if let Ok(payload) = serde_json::from_slice::<Value>(&body) {
                    extract_json_path(&payload, json_path)
                } else {
                    None
                }
            }
        }
    } else {
        None
    };

    // Atomically claim idempotency key before execution.
    let mut idempotency_claimed = false;
    if let Some(ref key) = idempotency_key {
        if let Some(ref idem_config) = info.idempotency {
            match claim_idempotency(&state.pool, info.name, key, idem_config.ttl).await {
                Ok(true) => {
                    idempotency_claimed = true;
                }
                Ok(false) => {
                    info!(
                        webhook = info.name,
                        idempotency_key = %key,
                        "Request already processed (idempotent)"
                    );
                    return (StatusCode::OK, Json(json!({"status": "already_processed"})));
                }
                Err(e) => {
                    warn!(webhook = info.name, error = %e, "Failed to claim idempotency key");
                }
            }
        }
    }

    // Parse payload
    let payload: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            if idempotency_claimed {
                if let Some(ref key) = idempotency_key {
                    if let Err(release_err) = release_idempotency(&state.pool, info.name, key).await
                    {
                        warn!(
                            webhook = info.name,
                            error = %release_err,
                            "Failed to release idempotency key after JSON parse failure"
                        );
                    }
                }
            }
            warn!(webhook = info.name, error = %e, "Invalid JSON payload");
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "Invalid JSON"})),
            );
        }
    };

    // Build headers map (lowercase keys)
    let header_map: HashMap<String, String> = headers
        .iter()
        .filter_map(|(k, v)| {
            v.to_str()
                .ok()
                .map(|v| (k.as_str().to_lowercase(), v.to_string()))
        })
        .collect();

    // Create context
    let mut ctx = WebhookContext::new(
        info.name.to_string(),
        request_id.clone(),
        header_map,
        state.pool.clone(),
        state.http_client.inner().clone(),
    )
    .with_idempotency_key(idempotency_key.clone());

    if let Some(ref dispatcher) = state.job_dispatcher {
        ctx = ctx.with_job_dispatch(dispatcher.clone());
    }

    // Execute handler with timeout
    let result = tokio::time::timeout(info.timeout, (entry.handler)(&ctx, payload)).await;

    match result {
        Ok(Ok(webhook_result)) => {
            let status =
                StatusCode::from_u16(webhook_result.status_code()).unwrap_or(StatusCode::OK);
            (status, Json(webhook_result.body()))
        }
        Ok(Err(e)) => {
            if idempotency_claimed {
                if let Some(ref key) = idempotency_key {
                    if let Err(release_err) = release_idempotency(&state.pool, info.name, key).await
                    {
                        warn!(
                            webhook = info.name,
                            error = %release_err,
                            "Failed to release idempotency key after handler error"
                        );
                    }
                }
            }
            error!(webhook = info.name, error = %e, "Webhook handler error");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        }
        Err(_) => {
            if idempotency_claimed {
                if let Some(ref key) = idempotency_key {
                    if let Err(release_err) = release_idempotency(&state.pool, info.name, key).await
                    {
                        warn!(
                            webhook = info.name,
                            error = %release_err,
                            "Failed to release idempotency key after timeout"
                        );
                    }
                }
            }
            error!(
                webhook = info.name,
                timeout = ?info.timeout,
                "Webhook handler timed out"
            );
            (
                StatusCode::GATEWAY_TIMEOUT,
                Json(json!({"error": "Request timeout"})),
            )
        }
    }
}

/// Validate HMAC signature.
fn validate_signature(
    algorithm: SignatureAlgorithm,
    body: &[u8],
    secret: &str,
    signature: &str,
) -> bool {
    // Strip algorithm prefix if present (e.g., "sha256=")
    let sig_hex = signature
        .strip_prefix(algorithm.prefix())
        .unwrap_or(signature);

    // Decode expected signature from hex
    let expected = match hex::decode(sig_hex) {
        Ok(b) => b,
        Err(_) => return false,
    };

    match algorithm {
        SignatureAlgorithm::HmacSha256 => {
            let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
                .expect("HMAC can take key of any size");
            mac.update(body);
            mac.verify_slice(&expected).is_ok()
        }
        SignatureAlgorithm::HmacSha1 => {
            let mut mac = Hmac::<Sha1>::new_from_slice(secret.as_bytes())
                .expect("HMAC can take key of any size");
            mac.update(body);
            mac.verify_slice(&expected).is_ok()
        }
        SignatureAlgorithm::HmacSha512 => {
            let mut mac = Hmac::<Sha512>::new_from_slice(secret.as_bytes())
                .expect("HMAC can take key of any size");
            mac.update(body);
            mac.verify_slice(&expected).is_ok()
        }
    }
}

/// Extract value from JSON using a simple path (e.g., "$.id" or "$.data.id").
fn extract_json_path(value: &Value, path: &str) -> Option<String> {
    let path = path.strip_prefix("$.").unwrap_or(path);
    let parts: Vec<&str> = path.split('.').collect();

    let mut current = value;
    for part in parts {
        current = current.get(part)?;
    }

    match current {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => Some(current.to_string()),
    }
}

/// Atomically claim idempotency key before processing.
///
/// Returns:
/// - `Ok(true)` if this request acquired the claim
/// - `Ok(false)` if key is already active
async fn claim_idempotency(
    pool: &PgPool,
    webhook_name: &str,
    key: &str,
    ttl: std::time::Duration,
) -> Result<bool, sqlx::Error> {
    let expires_at =
        chrono::Utc::now() + chrono::Duration::from_std(ttl).unwrap_or(chrono::Duration::hours(24));

    let result = sqlx::query(
        r#"
        INSERT INTO forge_webhook_events (idempotency_key, webhook_name, processed_at, expires_at)
        VALUES ($1, $2, NOW(), $3)
        ON CONFLICT (webhook_name, idempotency_key) DO UPDATE
            SET processed_at = EXCLUDED.processed_at,
                expires_at = EXCLUDED.expires_at
        WHERE forge_webhook_events.expires_at < NOW()
        "#,
    )
    .bind(key)
    .bind(webhook_name)
    .bind(expires_at)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

/// Release idempotency key after failure so retries can proceed.
async fn release_idempotency(
    pool: &PgPool,
    webhook_name: &str,
    key: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        DELETE FROM forge_webhook_events
        WHERE webhook_name = $1 AND idempotency_key = $2
        "#,
    )
    .bind(webhook_name)
    .bind(key)
    .execute(pool)
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_json_path_simple() {
        let value = json!({"id": "test-123"});
        assert_eq!(
            extract_json_path(&value, "$.id"),
            Some("test-123".to_string())
        );
    }

    #[test]
    fn test_extract_json_path_nested() {
        let value = json!({"data": {"id": "nested-456"}});
        assert_eq!(
            extract_json_path(&value, "$.data.id"),
            Some("nested-456".to_string())
        );
    }

    #[test]
    fn test_extract_json_path_number() {
        let value = json!({"count": 42});
        assert_eq!(extract_json_path(&value, "$.count"), Some("42".to_string()));
    }

    #[test]
    fn test_extract_json_path_missing() {
        let value = json!({"other": "value"});
        assert_eq!(extract_json_path(&value, "$.id"), None);
    }

    #[test]
    fn test_validate_signature_sha256() {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;

        let body = b"test payload";
        let secret = "test_secret";

        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        let signature = hex::encode(mac.finalize().into_bytes());

        assert!(validate_signature(
            SignatureAlgorithm::HmacSha256,
            body,
            secret,
            &signature
        ));

        // With prefix
        let sig_with_prefix = format!("sha256={}", signature);
        assert!(validate_signature(
            SignatureAlgorithm::HmacSha256,
            body,
            secret,
            &sig_with_prefix
        ));
    }

    #[test]
    fn test_validate_signature_invalid() {
        assert!(!validate_signature(
            SignatureAlgorithm::HmacSha256,
            b"test",
            "secret",
            "invalid_hex"
        ));

        assert!(!validate_signature(
            SignatureAlgorithm::HmacSha256,
            b"test",
            "secret",
            "0000000000000000000000000000000000000000000000000000000000000000"
        ));
    }
}
