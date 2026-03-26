//! Refresh token management.
//!
//! Provides token pair issuance (access + refresh), rotation, and revocation.
//! Refresh tokens are stored as SHA-256 hashes in `forge_refresh_tokens`.

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

/// Generate a cryptographically random refresh token string.
pub fn generate_refresh_token() -> String {
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    format!("{}{}", a.simple(), b.simple())
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
    let access_token = issue_access_fn(user_id, roles, access_token_ttl_secs)?;

    let refresh_raw = generate_refresh_token();
    let refresh_hash = hash_token(&refresh_raw);
    let expires_at = chrono::Utc::now() + chrono::Duration::days(refresh_token_ttl_days);

    sqlx::query!(
        "INSERT INTO forge_refresh_tokens (user_id, token_hash, client_id, expires_at) \
         VALUES ($1, $2, $3, $4)",
        user_id,
        &refresh_hash,
        client_id,
        expires_at,
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
/// The new token is also bound to the same client.
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

    // Atomically delete only non-expired tokens, optionally matching client_id.
    let row = if let Some(cid) = client_id {
        sqlx::query_scalar!(
            "DELETE FROM forge_refresh_tokens WHERE token_hash = $1 AND expires_at > now() AND client_id = $2 RETURNING user_id",
            hash,
            cid
        )
        .fetch_optional(pool)
        .await
    } else {
        sqlx::query_scalar!(
            "DELETE FROM forge_refresh_tokens WHERE token_hash = $1 AND expires_at > now()\n         RETURNING user_id",
            hash
        )
        .fetch_optional(pool)
        .await
    }
    .map_err(|e| ForgeError::Internal(format!("Failed to rotate refresh token: {e}")))?;

    let user_id = match row {
        Some(r) => r,
        None => {
            return Err(ForgeError::Unauthorized(
                "Invalid or expired refresh token".into(),
            ));
        }
    };

    issue_token_pair_with_client(
        pool,
        user_id,
        roles,
        access_token_ttl_secs,
        refresh_token_ttl_days,
        client_id,
        issue_access_fn,
    )
    .await
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
