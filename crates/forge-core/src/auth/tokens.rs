//! Refresh token management.
//!
//! Provides token pair issuance (access + refresh), rotation, and revocation.
//! Refresh tokens are stored as SHA-256 hashes in `forge_refresh_tokens`.
//!
//! ## Token format
//!
//! Raw refresh tokens encode their family UUID so chain reuse detection can
//! revoke the whole family even after the specific token row is deleted:
//!
//! ```text
//! <family_uuid_hex>.<random_uuid_hex>
//! ```
//!
//! Only the SHA-256 hash of the full string is stored in the database.
//!
//! ## Chain reuse detection
//!
//! Each token family represents a single login session. During rotation the old
//! token is deleted and a new one with the same `token_family` is inserted
//! atomically. If a previously-rotated (deleted) token is presented:
//!
//! 1. The DELETE returns 0 rows.
//! 2. The family UUID is decoded from the raw token value.
//! 3. All tokens sharing that family are revoked immediately.
//! 4. `Unauthorized` is returned to the caller.
//!
//! This terminates the session for both the legitimate user (who holds the
//! current token) and the attacker who replayed the old one.

use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::{ForgeError, Result};

/// An access token + refresh token pair.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
}

/// SHA-256 hash a raw token string for storage.
pub fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Generate a raw refresh token that encodes its family UUID.
///
/// Format: `<family_hex>.<random_hex>` — the dot separator lets
/// `extract_family` recover the family without a DB lookup.
fn generate_refresh_token_for_family(family: Uuid) -> String {
    let random = Uuid::new_v4();
    format!("{}.{}", family.simple(), random.simple())
}

/// Extract the family UUID from a raw refresh token.
///
/// Returns `None` for legacy tokens that pre-date the family format.
fn extract_family(raw_token: &str) -> Option<Uuid> {
    let (family_hex, _) = raw_token.split_once('.')?;
    Uuid::parse_str(family_hex).ok()
}

/// Issue a token pair: sign an access JWT and store a refresh token.
///
/// `issue_access_fn` is called to sign the access token (wraps `ctx.issue_token`).
/// `client_id` binds the refresh token to an OAuth client (pass `None` for non-OAuth usage).
pub async fn issue_token_pair(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    roles: &[&str],
    access_token_ttl_secs: i64,
    refresh_token_ttl_days: i64,
    issue_access_fn: impl FnOnce(Uuid, &[&str], i64) -> Result<String>,
) -> Result<TokenPair> {
    issue_token_pair_with_client(
        pool,
        user_id,
        roles,
        access_token_ttl_secs,
        refresh_token_ttl_days,
        None,
        issue_access_fn,
    )
    .await
}

/// Issue a token pair with optional OAuth client binding.
///
/// When `client_id` is `Some`, the refresh token is bound to that client
/// and can only be rotated by presenting the same client_id.
pub async fn issue_token_pair_with_client(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    roles: &[&str],
    access_token_ttl_secs: i64,
    refresh_token_ttl_days: i64,
    client_id: Option<&str>,
    issue_access_fn: impl FnOnce(Uuid, &[&str], i64) -> Result<String>,
) -> Result<TokenPair> {
    let family = Uuid::new_v4();
    issue_token_in_family(
        pool,
        user_id,
        roles,
        access_token_ttl_secs,
        refresh_token_ttl_days,
        client_id,
        family,
        issue_access_fn,
    )
    .await
}

/// Internal: insert a new refresh token carrying an existing family ID.
///
/// Used both by `issue_token_pair_with_client` (new family) and
/// `rotate_refresh_token_with_client` (carry family forward).
#[allow(clippy::too_many_arguments)]
async fn issue_token_in_family(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    roles: &[&str],
    access_token_ttl_secs: i64,
    refresh_token_ttl_days: i64,
    client_id: Option<&str>,
    family: Uuid,
    issue_access_fn: impl FnOnce(Uuid, &[&str], i64) -> Result<String>,
) -> Result<TokenPair> {
    let access_token = issue_access_fn(user_id, roles, access_token_ttl_secs)?;

    let refresh_raw = generate_refresh_token_for_family(family);
    let refresh_hash = hash_token(&refresh_raw);
    let expires_at = chrono::Utc::now() + chrono::Duration::days(refresh_token_ttl_days);

    sqlx::query!(
        "INSERT INTO forge_refresh_tokens (user_id, token_hash, client_id, expires_at, token_family) \
         VALUES ($1, $2, $3, $4, $5)",
        user_id,
        &refresh_hash,
        client_id,
        expires_at,
        family,
    )
    .execute(pool)
    .await
    .map_err(|e| ForgeError::Internal(format!("Failed to store refresh token: {e}")))?;

    Ok(TokenPair {
        access_token,
        refresh_token: refresh_raw,
    })
}

/// Rotate a refresh token: validate expiry, delete the old one, issue a new pair.
pub async fn rotate_refresh_token(
    pool: &sqlx::PgPool,
    old_refresh_token: &str,
    roles: &[&str],
    access_token_ttl_secs: i64,
    refresh_token_ttl_days: i64,
    issue_access_fn: impl FnOnce(Uuid, &[&str], i64) -> Result<String>,
) -> Result<TokenPair> {
    rotate_refresh_token_with_client(
        pool,
        old_refresh_token,
        roles,
        access_token_ttl_secs,
        refresh_token_ttl_days,
        None,
        issue_access_fn,
    )
    .await
}

/// Rotate a refresh token with OAuth client binding validation.
///
/// When `client_id` is `Some`, the token must be bound to that client.
/// The new token is issued in the same family as the old one.
///
/// ## Chain reuse detection
///
/// If the DELETE returns 0 rows the token is either invalid, expired, or
/// already rotated. The family UUID is decoded from the raw token value
/// (no extra DB read needed). If it parses, all tokens in that family are
/// revoked immediately — the session is terminated for everyone holding a
/// token in the chain, cutting off both the attacker and the legitimate user.
pub async fn rotate_refresh_token_with_client(
    pool: &sqlx::PgPool,
    old_refresh_token: &str,
    roles: &[&str],
    access_token_ttl_secs: i64,
    refresh_token_ttl_days: i64,
    client_id: Option<&str>,
    issue_access_fn: impl FnOnce(Uuid, &[&str], i64) -> Result<String>,
) -> Result<TokenPair> {
    let hash = hash_token(old_refresh_token);

    // Atomically delete the token if valid, returning the family so the new
    // token is issued in the same chain.
    //
    // When client_id is provided, require exact match. When omitted, only
    // allow rotation of tokens that were NOT bound to any client (prevents
    // an attacker from bypassing client binding by omitting client_id).
    struct TokenRow {
        user_id: Uuid,
        token_family: Uuid,
    }

    let row = if let Some(cid) = client_id {
        sqlx::query!(
            "DELETE FROM forge_refresh_tokens \
             WHERE token_hash = $1 AND expires_at > now() AND client_id = $2 \
             RETURNING user_id, token_family",
            hash,
            cid
        )
        .fetch_optional(pool)
        .await
        .map(|r| {
            r.map(|r| TokenRow {
                user_id: r.user_id,
                token_family: r.token_family,
            })
        })
    } else {
        sqlx::query!(
            "DELETE FROM forge_refresh_tokens \
             WHERE token_hash = $1 AND expires_at > now() AND client_id IS NULL \
             RETURNING user_id, token_family",
            hash
        )
        .fetch_optional(pool)
        .await
        .map(|r| {
            r.map(|r| TokenRow {
                user_id: r.user_id,
                token_family: r.token_family,
            })
        })
    }
    .map_err(|e| ForgeError::Internal(format!("Failed to rotate refresh token: {e}")))?;

    match row {
        Some(token) => {
            issue_token_in_family(
                pool,
                token.user_id,
                roles,
                access_token_ttl_secs,
                refresh_token_ttl_days,
                client_id,
                token.token_family,
                issue_access_fn,
            )
            .await
        }
        None => {
            // Token not found. Decode the family from the raw token value —
            // if the format matches, this is a previously-rotated token being
            // replayed (reuse attack). Nuke the whole family to terminate the
            // session for everyone, then return Unauthorized.
            if let Some(family_id) = extract_family(old_refresh_token) {
                let deleted = sqlx::query!(
                    "DELETE FROM forge_refresh_tokens WHERE token_family = $1",
                    family_id
                )
                .execute(pool)
                .await
                .map(|r| r.rows_affected())
                .unwrap_or(0);

                if deleted > 0 {
                    tracing::warn!(
                        %family_id,
                        revoked = deleted,
                        "Refresh token reuse detected — entire family revoked"
                    );
                }
            }

            Err(ForgeError::Unauthorized(
                "Invalid or expired refresh token".into(),
            ))
        }
    }
}

/// Revoke a specific refresh token.
pub async fn revoke_refresh_token(pool: &sqlx::PgPool, refresh_token: &str) -> Result<()> {
    let hash = hash_token(refresh_token);
    sqlx::query!(
        "DELETE FROM forge_refresh_tokens WHERE token_hash = $1",
        &hash
    )
    .execute(pool)
    .await
    .map_err(|e| ForgeError::Internal(format!("Failed to revoke refresh token: {e}")))?;
    Ok(())
}

/// Revoke all refresh tokens for a user.
pub async fn revoke_all_refresh_tokens(pool: &sqlx::PgPool, user_id: Uuid) -> Result<()> {
    sqlx::query!(
        "DELETE FROM forge_refresh_tokens WHERE user_id = $1",
        user_id
    )
    .execute(pool)
    .await
    .map_err(|e| ForgeError::Internal(format!("Failed to revoke refresh tokens: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_refresh_token_for_family_encodes_family() {
        let family = Uuid::new_v4();
        let token = generate_refresh_token_for_family(family);

        assert!(token.contains('.'), "token must contain the dot separator");
        let recovered = extract_family(&token);
        assert_eq!(recovered, Some(family));
    }

    #[test]
    fn test_extract_family_returns_none_for_legacy_format() {
        // Old format: two UUIDs concatenated without separator
        let legacy = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        assert_eq!(extract_family(&legacy), None);
    }

    #[test]
    fn test_extract_family_returns_none_for_garbage() {
        assert_eq!(extract_family("not-a-token"), None);
        assert_eq!(extract_family(""), None);
    }

    #[test]
    fn test_hash_token_is_deterministic() {
        let token = "some-raw-token-value";
        assert_eq!(hash_token(token), hash_token(token));
    }

    #[test]
    fn test_hash_token_differs_for_different_inputs() {
        assert_ne!(hash_token("token-a"), hash_token("token-b"));
    }
}
