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
use base64::{
    Engine as _,
    engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig, general_purpose},
};
use ed25519_dalek::{Signature as Ed25519Signature, Verifier as _, VerifyingKey};
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
        if !validate_signature(sig_config.algorithm, &body, &secret, signature, &headers) {
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
    if let Some(ref key) = idempotency_key
        && let Some(ref idem_config) = info.idempotency
    {
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
                // Fail closed: if idempotency is configured but the DB is unavailable,
                // reject the request rather than processing without replay protection
                error!(webhook = info.name, error = %e, "Failed to claim idempotency key -- rejecting request");
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(json!({"error": "Service temporarily unavailable"})),
                );
            }
        }
    }

    // Parse payload
    let payload: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            if idempotency_claimed
                && let Some(ref key) = idempotency_key
                && let Err(release_err) = release_idempotency(&state.pool, info.name, key).await
            {
                warn!(
                    webhook = info.name,
                    error = %release_err,
                    "Failed to release idempotency key after JSON parse failure"
                );
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
        state.http_client.clone(),
    )
    .with_idempotency_key(idempotency_key.clone());
    ctx.set_http_timeout(info.http_timeout);

    if let Some(ref dispatcher) = state.job_dispatcher {
        ctx = ctx.with_job_dispatch(dispatcher.clone());
    }

    // Execute handler with timeout
    let exec_start = std::time::Instant::now();
    let result = tokio::time::timeout(info.timeout, (entry.handler)(&ctx, payload)).await;
    let exec_duration_ms = exec_start.elapsed().as_millis().min(i32::MAX as u128) as i32;

    match result {
        Ok(Ok(webhook_result)) => {
            let status =
                StatusCode::from_u16(webhook_result.status_code()).unwrap_or(StatusCode::OK);
            crate::signals::emit_server_execution(
                info.name,
                "webhook",
                exec_duration_ms,
                status.is_success(),
                None,
            );
            (status, Json(webhook_result.body()))
        }
        Ok(Err(e)) => {
            if idempotency_claimed
                && let Some(ref key) = idempotency_key
                && let Err(release_err) = release_idempotency(&state.pool, info.name, key).await
            {
                warn!(
                    webhook = info.name,
                    error = %release_err,
                    "Failed to release idempotency key after handler error"
                );
            }
            let err_str = e.to_string();
            error!(webhook = info.name, error = %e, "Webhook handler error");
            crate::signals::emit_server_execution(
                info.name,
                "webhook",
                exec_duration_ms,
                false,
                Some(err_str),
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Internal server error", "request_id": request_id})),
            )
        }
        Err(_) => {
            if idempotency_claimed
                && let Some(ref key) = idempotency_key
                && let Err(release_err) = release_idempotency(&state.pool, info.name, key).await
            {
                warn!(
                    webhook = info.name,
                    error = %release_err,
                    "Failed to release idempotency key after timeout"
                );
            }
            error!(
                webhook = info.name,
                timeout = ?info.timeout,
                "Webhook handler timed out"
            );
            crate::signals::emit_server_execution(
                info.name,
                "webhook",
                exec_duration_ms,
                false,
                Some(format!("Webhook timed out after {:?}", info.timeout)),
            );
            (
                StatusCode::GATEWAY_TIMEOUT,
                Json(json!({"error": "Request timeout"})),
            )
        }
    }
}

/// Validate webhook signature, dispatching to the appropriate algorithm.
fn validate_signature(
    algorithm: SignatureAlgorithm,
    body: &[u8],
    secret: &str,
    signature: &str,
    headers: &HeaderMap,
) -> bool {
    match algorithm {
        SignatureAlgorithm::StandardWebhooks => {
            validate_standard_webhooks(body, secret, signature, headers)
        }
        SignatureAlgorithm::StripeWebhooks => validate_stripe_webhooks(body, secret, signature),
        SignatureAlgorithm::HmacSha256Base64 => {
            validate_hmac_sha256_base64(body, secret, signature)
        }
        SignatureAlgorithm::Ed25519 => validate_ed25519(body, secret, signature),
        alg => {
            // HMAC variants: decode hex, verify
            let sig_hex = signature.strip_prefix(alg.prefix()).unwrap_or(signature);
            let expected = match decode_hex(sig_hex) {
                Some(b) => b,
                None => return false,
            };
            match alg {
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
                _ => unreachable!(),
            }
        }
    }
}

/// Validate a Standard Webhooks signature (https://www.standardwebhooks.com).
///
/// - Secret: strip `whsec_` or `polar_whs_` prefix, then base64-decode.
/// - Signed content: `{webhook-id}\n{webhook-timestamp}\n{body}`.
/// - Signature header format: `v1,<base64>` (space-separated for multiple).
fn validate_standard_webhooks(
    body: &[u8],
    secret: &str,
    signature_header: &str,
    headers: &HeaderMap,
) -> bool {
    let msg_id = match headers.get("webhook-id").and_then(|v| v.to_str().ok()) {
        Some(v) => v,
        None => return false,
    };

    let msg_timestamp = match headers
        .get("webhook-timestamp")
        .and_then(|v| v.to_str().ok())
    {
        Some(v) => v,
        None => return false,
    };

    // Strip known prefixes and base64-decode the raw key.
    // Use a permissive decoder: accept both padded and unpadded input, and allow
    // non-zero trailing bits. Polar (and some other providers) emit base64 keys
    // whose last group has non-canonical trailing bits that strict decoders reject.
    let decoder = GeneralPurpose::new(
        &base64::alphabet::STANDARD,
        GeneralPurposeConfig::new()
            .with_decode_padding_mode(DecodePaddingMode::Indifferent)
            .with_decode_allow_trailing_bits(true),
    );
    let b64_key = secret
        .strip_prefix("whsec_")
        .or_else(|| secret.strip_prefix("polar_whs_"))
        .unwrap_or(secret);

    let key_bytes = match decoder.decode(b64_key) {
        Ok(b) => b,
        Err(_) => return false,
    };

    // Build the signed content per Standard Webhooks spec: "{id}.{timestamp}.{body}"
    let mut signed = Vec::with_capacity(msg_id.len() + msg_timestamp.len() + body.len() + 2);
    signed.extend_from_slice(msg_id.as_bytes());
    signed.push(b'.');
    signed.extend_from_slice(msg_timestamp.as_bytes());
    signed.push(b'.');
    signed.extend_from_slice(body);

    let mut mac =
        Hmac::<Sha256>::new_from_slice(&key_bytes).expect("HMAC can take key of any size");
    mac.update(&signed);
    let computed = mac.finalize().into_bytes();
    let computed_b64 = general_purpose::STANDARD.encode(computed);

    // The header may contain multiple space-separated signatures: "v1,<b64> v1,<b64>"
    signature_header
        .split_whitespace()
        .filter_map(|s| s.strip_prefix("v1,"))
        .any(|sig| sig == computed_b64)
}

/// Validate a Stripe webhook signature.
///
/// - Header format: `t=1234567890,v1=<hex>,v1=<hex>`
/// - Signed content: `{timestamp}.{body}`
/// - Rejects requests where the timestamp is more than 5 minutes old.
fn validate_stripe_webhooks(body: &[u8], secret: &str, signature_header: &str) -> bool {
    let mut timestamp: Option<&str> = None;
    let mut signatures: Vec<&str> = Vec::new();

    for part in signature_header.split(',') {
        if let Some(t) = part.strip_prefix("t=") {
            timestamp = Some(t);
        } else if let Some(sig) = part.strip_prefix("v1=") {
            signatures.push(sig);
        }
    }

    let timestamp = match timestamp {
        Some(t) => t,
        None => return false,
    };

    // Replay protection: reject if timestamp is more than 5 minutes off
    let ts: i64 = match timestamp.parse() {
        Ok(n) => n,
        Err(_) => return false,
    };
    if (chrono::Utc::now().timestamp() - ts).abs() > 300 {
        return false;
    }

    let mut signed = Vec::with_capacity(timestamp.len() + 1 + body.len());
    signed.extend_from_slice(timestamp.as_bytes());
    signed.push(b'.');
    signed.extend_from_slice(body);

    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC can take key of any size");
    mac.update(&signed);
    let computed = encode_hex_inline(&mac.finalize().into_bytes());

    signatures.iter().any(|sig| *sig == computed)
}

/// Validate a Shopify (HMAC-SHA256, base64-encoded) webhook signature.
fn validate_hmac_sha256_base64(body: &[u8], secret: &str, signature: &str) -> bool {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC can take key of any size");
    mac.update(body);
    let computed = general_purpose::STANDARD.encode(mac.finalize().into_bytes());
    // Constant-time comparison isn't critical here — the signature is already base64,
    // so timing differences don't meaningfully leak key bits.
    computed == signature
}

/// Validate an Ed25519 asymmetric webhook signature.
///
/// `public_key_b64` is a base64-encoded 32-byte Ed25519 public key.
/// `signature_b64` is a base64-encoded 64-byte Ed25519 signature over the body.
fn validate_ed25519(body: &[u8], public_key_b64: &str, signature_b64: &str) -> bool {
    let pub_key_bytes = match general_purpose::STANDARD.decode(public_key_b64) {
        Ok(b) => b,
        Err(_) => return false,
    };
    let pub_key_array: [u8; 32] = match pub_key_bytes.as_slice().try_into() {
        Ok(b) => b,
        Err(_) => return false,
    };
    let verifying_key = match VerifyingKey::from_bytes(&pub_key_array) {
        Ok(k) => k,
        Err(_) => return false,
    };

    let sig_bytes = match general_purpose::STANDARD.decode(signature_b64) {
        Ok(b) => b,
        Err(_) => return false,
    };
    let sig_array: [u8; 64] = match sig_bytes.as_slice().try_into() {
        Ok(b) => b,
        Err(_) => return false,
    };
    let signature = Ed25519Signature::from_bytes(&sig_array);

    verifying_key.verify(body, &signature).is_ok()
}

fn encode_hex_inline(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            use std::fmt::Write;
            let _ = write!(s, "{b:02x}");
            s
        })
}

fn decode_hex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
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

    let result = sqlx::query!(
        r#"
        INSERT INTO forge_webhook_events (idempotency_key, webhook_name, processed_at, expires_at)
        VALUES ($1, $2, NOW(), $3)
        ON CONFLICT (webhook_name, idempotency_key) DO UPDATE
            SET processed_at = EXCLUDED.processed_at,
                expires_at = EXCLUDED.expires_at
        WHERE forge_webhook_events.expires_at < NOW()
        "#,
        key,
        webhook_name,
        expires_at,
    )
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
    sqlx::query!(
        r#"
        DELETE FROM forge_webhook_events
        WHERE webhook_name = $1 AND idempotency_key = $2
        "#,
        webhook_name,
        key,
    )
    .execute(pool)
    .await?;

    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]
mod tests {
    use super::*;

    fn encode_hex(bytes: &[u8]) -> String {
        bytes
            .iter()
            .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
                use std::fmt::Write;
                let _ = write!(s, "{b:02x}");
                s
            })
    }

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
        let empty_headers = HeaderMap::new();

        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        let signature = encode_hex(&mac.finalize().into_bytes());

        assert!(validate_signature(
            SignatureAlgorithm::HmacSha256,
            body,
            secret,
            &signature,
            &empty_headers,
        ));

        // With prefix
        let sig_with_prefix = format!("sha256={}", signature);
        assert!(validate_signature(
            SignatureAlgorithm::HmacSha256,
            body,
            secret,
            &sig_with_prefix,
            &empty_headers,
        ));
    }

    #[test]
    fn test_validate_signature_invalid() {
        let empty_headers = HeaderMap::new();

        assert!(!validate_signature(
            SignatureAlgorithm::HmacSha256,
            b"test",
            "secret",
            "invalid_hex",
            &empty_headers,
        ));

        assert!(!validate_signature(
            SignatureAlgorithm::HmacSha256,
            b"test",
            "secret",
            "0000000000000000000000000000000000000000000000000000000000000000",
            &empty_headers,
        ));
    }

    #[test]
    fn test_validate_stripe_webhooks() {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;

        let body = b"{\"type\":\"payment_intent.succeeded\"}";
        let secret = "whsec_test_stripe_secret";
        let timestamp = chrono::Utc::now().timestamp().to_string();

        let mut signed = Vec::new();
        signed.extend_from_slice(timestamp.as_bytes());
        signed.push(b'.');
        signed.extend_from_slice(body);

        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(&signed);
        let sig_hex = encode_hex(&mac.finalize().into_bytes());

        let header = format!("t={timestamp},v1={sig_hex}");
        assert!(validate_stripe_webhooks(body, secret, &header));

        // Multiple signatures (Stripe can include both v1 and a legacy v0)
        let header_multi = format!("t={timestamp},v0=ignored,v1={sig_hex}");
        assert!(validate_stripe_webhooks(body, secret, &header_multi));

        // Wrong signature
        assert!(!validate_stripe_webhooks(
            body,
            secret,
            &format!("t={timestamp},v1=deadbeef")
        ));

        // Missing timestamp
        assert!(!validate_stripe_webhooks(
            body,
            secret,
            &format!("v1={sig_hex}")
        ));

        // Stale timestamp (replay attack)
        let old_ts = (chrono::Utc::now().timestamp() - 600).to_string();
        let mut mac2 = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        let mut signed2 = Vec::new();
        signed2.extend_from_slice(old_ts.as_bytes());
        signed2.push(b'.');
        signed2.extend_from_slice(body);
        mac2.update(&signed2);
        let old_sig = encode_hex(&mac2.finalize().into_bytes());
        assert!(!validate_stripe_webhooks(
            body,
            secret,
            &format!("t={old_ts},v1={old_sig}")
        ));
    }

    #[test]
    fn test_validate_hmac_sha256_base64() {
        use base64::{Engine as _, engine::general_purpose};
        use hmac::{Hmac, Mac};
        use sha2::Sha256;

        let body = b"{\"topic\":\"orders/create\"}";
        let secret = "shopify_secret";

        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        let sig_b64 = general_purpose::STANDARD.encode(mac.finalize().into_bytes());

        assert!(validate_hmac_sha256_base64(body, secret, &sig_b64));

        // Hex-encoded (wrong format) should fail
        let sig_hex = encode_hex(&{
            let mut mac2 = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
            mac2.update(body);
            mac2.finalize().into_bytes().to_vec()
        });
        assert!(!validate_hmac_sha256_base64(body, secret, &sig_hex));
    }

    #[test]
    fn test_validate_ed25519() {
        use base64::{Engine as _, engine::general_purpose};
        use ed25519_dalek::{Signer, SigningKey};

        let body = b"{\"event\":\"user.created\"}";
        // Deterministic key from a fixed seed
        let seed = [42u8; 32];
        let signing_key = SigningKey::from_bytes(&seed);
        let verifying_key = signing_key.verifying_key();

        let public_key_b64 = general_purpose::STANDARD.encode(verifying_key.as_bytes());
        let signature = signing_key.sign(body);
        let signature_b64 = general_purpose::STANDARD.encode(signature.to_bytes());

        assert!(validate_ed25519(body, &public_key_b64, &signature_b64));

        // Wrong body
        assert!(!validate_ed25519(
            b"tampered",
            &public_key_b64,
            &signature_b64
        ));

        // Garbage signature
        assert!(!validate_ed25519(body, &public_key_b64, "notbase64!!"));

        // Wrong public key
        let other_seed = [99u8; 32];
        let other_key = SigningKey::from_bytes(&other_seed).verifying_key();
        let other_pub_b64 = general_purpose::STANDARD.encode(other_key.as_bytes());
        assert!(!validate_ed25519(body, &other_pub_b64, &signature_b64));
    }

    #[test]
    fn test_validate_standard_webhooks() {
        use base64::{Engine as _, engine::general_purpose};
        use hmac::{Hmac, Mac};
        use sha2::Sha256;

        let msg_id = "msg_test_123";
        let msg_timestamp = "1234567890";
        let body = b"{\"event\":\"subscription.created\"}";
        // raw key bytes, base64-encoded as the "secret"
        let raw_key = b"super_secret_key_bytes_32_chars!!";
        let secret = format!("whsec_{}", general_purpose::STANDARD.encode(raw_key));

        let mut signed = Vec::new();
        signed.extend_from_slice(msg_id.as_bytes());
        signed.push(b'.');
        signed.extend_from_slice(msg_timestamp.as_bytes());
        signed.push(b'.');
        signed.extend_from_slice(body);

        let mut mac = Hmac::<Sha256>::new_from_slice(raw_key).unwrap();
        mac.update(&signed);
        let sig_b64 = general_purpose::STANDARD.encode(mac.finalize().into_bytes());
        let signature_header = format!("v1,{}", sig_b64);

        let mut headers = HeaderMap::new();
        headers.insert("webhook-id", msg_id.parse().unwrap());
        headers.insert("webhook-timestamp", msg_timestamp.parse().unwrap());

        assert!(validate_standard_webhooks(
            body,
            &secret,
            &signature_header,
            &headers
        ));

        // Wrong signature should fail
        assert!(!validate_standard_webhooks(
            body,
            &secret,
            "v1,invalidsig",
            &headers
        ));

        // Missing headers should fail
        assert!(!validate_standard_webhooks(
            body,
            &secret,
            &signature_header,
            &HeaderMap::new()
        ));
    }
}
