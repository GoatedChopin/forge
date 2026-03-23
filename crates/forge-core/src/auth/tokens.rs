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
pub async fn issue_token_pair(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    roles: &[&str],
    access_token_ttl_secs: i64,
    refresh_token_ttl_days: i64,
    issue_access_fn: impl FnOnce(Uuid, &[&str], i64) -> Result<String>,
) -> Result<TokenPair> {
    let access_token = issue_access_fn(user_id, roles, access_token_ttl_secs)?;

    let refresh_raw = generate_refresh_token();
    let refresh_hash = hash_token(&refresh_raw);
    let expires_at = chrono::Utc::now() + chrono::Duration::days(refresh_token_ttl_days);

    sqlx::query(
        "INSERT INTO forge_refresh_tokens (user_id, token_hash, expires_at) VALUES ($1, $2, $3)",
    )
    .bind(user_id)
    .bind(&refresh_hash)
    .bind(expires_at)
    .execute(pool)
    .await
    .map_err(|e| ForgeError::Internal(format!("Failed to store refresh token: {e}")))?;

    Ok(TokenPair {
        access_token,
        refresh_token: refresh_raw,
    })
}

/// Rotate a refresh token: validate and delete the old one, issue a new pair.
pub async fn rotate_refresh_token(
    pool: &sqlx::PgPool,
    old_refresh_token: &str,
    roles: &[&str],
    access_token_ttl_secs: i64,
    refresh_token_ttl_days: i64,
    issue_access_fn: impl FnOnce(Uuid, &[&str], i64) -> Result<String>,
) -> Result<TokenPair> {
    let hash = hash_token(old_refresh_token);

    // Atomically delete the old token and get its user_id
    let row: Option<(Uuid, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        "DELETE FROM forge_refresh_tokens WHERE token_hash = $1
         RETURNING user_id, expires_at",
    )
    .bind(&hash)
    .fetch_optional(pool)
    .await
    .map_err(|e| ForgeError::Internal(format!("Failed to rotate refresh token: {e}")))?;

    let (user_id, expires_at) =
        row.ok_or_else(|| ForgeError::Unauthorized("Invalid refresh token".into()))?;

    if expires_at < chrono::Utc::now() {
        return Err(ForgeError::Unauthorized("Refresh token expired".into()));
    }

    // Issue a new pair
    issue_token_pair(
        pool,
        user_id,
        roles,
        access_token_ttl_secs,
        refresh_token_ttl_days,
        issue_access_fn,
    )
    .await
}

/// Revoke a specific refresh token.
pub async fn revoke_refresh_token(pool: &sqlx::PgPool, refresh_token: &str) -> Result<()> {
    let hash = hash_token(refresh_token);
    sqlx::query("DELETE FROM forge_refresh_tokens WHERE token_hash = $1")
        .bind(&hash)
        .execute(pool)
        .await
        .map_err(|e| ForgeError::Internal(format!("Failed to revoke refresh token: {e}")))?;
    Ok(())
}

/// Revoke all refresh tokens for a user.
pub async fn revoke_all_refresh_tokens(pool: &sqlx::PgPool, user_id: Uuid) -> Result<()> {
    sqlx::query("DELETE FROM forge_refresh_tokens WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .map_err(|e| ForgeError::Internal(format!("Failed to revoke refresh tokens: {e}")))?;
    Ok(())
}
