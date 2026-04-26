use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// JWT claims structure.
///
/// Fields are intentionally crate-private. Construct via [`Claims::builder`]
/// and read via the accessor methods. The `custom` map is gated by
/// [`Claims::get_claim`] / [`Claims::sanitized_custom`] so reserved JWT
/// claim names (`iss`, `aud`, `nbf`, `jti`, …) can never be retrieved as
/// custom data, even when serde's `#[serde(flatten)]` lets them in.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Claims {
    /// Subject (user ID). Use [`Claims::sub`] / [`Claims::user_id`].
    pub(crate) sub: String,
    /// Issued at (Unix timestamp). Use [`Claims::iat`].
    pub(crate) iat: i64,
    /// Expiration time (Unix timestamp). Use [`Claims::exp`] /
    /// [`Claims::is_expired`].
    pub(crate) exp: i64,
    /// User roles. Use [`Claims::roles`] / [`Claims::has_role`].
    #[serde(default)]
    pub(crate) roles: Vec<String>,
    /// Custom claims, with reserved JWT claims filtered out on read.
    /// Use [`Claims::get_claim`] / [`Claims::sanitized_custom`].
    #[serde(flatten)]
    pub(crate) custom: HashMap<String, serde_json::Value>,
}

impl Claims {
    /// Get the subject (raw `sub` claim).
    pub fn sub(&self) -> &str {
        &self.sub
    }

    /// Get the issued-at Unix timestamp.
    pub fn iat(&self) -> i64 {
        self.iat
    }

    /// Get the expiration Unix timestamp.
    pub fn exp(&self) -> i64 {
        self.exp
    }

    /// Get the user roles.
    pub fn roles(&self) -> &[String] {
        &self.roles
    }

    /// Consume the claims and return the owned roles vector.
    pub fn into_roles(self) -> Vec<String> {
        self.roles
    }

    /// Consume the claims and return the owned subject string.
    pub fn into_sub(self) -> String {
        self.sub
    }

    /// Get the user ID as UUID.
    pub fn user_id(&self) -> Option<Uuid> {
        Uuid::parse_str(&self.sub).ok()
    }

    /// Check if the token is expired.
    pub fn is_expired(&self) -> bool {
        let now = chrono::Utc::now().timestamp();
        self.exp < now
    }

    /// Check if the user has a role.
    pub fn has_role(&self, role: &str) -> bool {
        self.roles.iter().any(|r| r == role)
    }

    /// Reserved JWT claim names that should not be treated as custom claims.
    const RESERVED_CLAIMS: &'static [&'static str] =
        &["iss", "aud", "nbf", "jti", "sub", "iat", "exp", "roles"];

    /// Get a custom claim value.
    ///
    /// Returns `None` for reserved JWT claims (iss, aud, nbf, jti, etc.)
    /// to prevent claim injection via `#[serde(flatten)]`.
    pub fn get_claim(&self, key: &str) -> Option<&serde_json::Value> {
        if Self::RESERVED_CLAIMS.contains(&key) {
            return None;
        }
        self.custom.get(key)
    }

    /// Get custom claims with reserved JWT claims filtered out.
    ///
    /// Prevents claim injection where standard JWT claims like `iss`, `aud`,
    /// or `jti` end up in the custom claims map via `#[serde(flatten)]`.
    pub fn sanitized_custom(&self) -> HashMap<String, serde_json::Value> {
        self.custom
            .iter()
            .filter(|(k, _)| !Self::RESERVED_CLAIMS.contains(&k.as_str()))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    /// Get the tenant ID if present in claims.
    pub fn tenant_id(&self) -> Option<Uuid> {
        self.custom
            .get("tenant_id")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok())
    }

    /// Create a builder for constructing claims.
    pub fn builder() -> ClaimsBuilder {
        ClaimsBuilder::new()
    }
}

/// Builder for JWT claims.
#[derive(Debug, Default)]
pub struct ClaimsBuilder {
    sub: Option<String>,
    roles: Vec<String>,
    custom: HashMap<String, serde_json::Value>,
    duration_secs: i64,
}

impl ClaimsBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self {
            sub: None,
            roles: Vec::new(),
            custom: HashMap::new(),
            duration_secs: 3600, // 1 hour default
        }
    }

    /// Set the subject (user ID).
    pub fn subject(mut self, sub: impl Into<String>) -> Self {
        self.sub = Some(sub.into());
        self
    }

    /// Set the user ID from UUID.
    pub fn user_id(mut self, id: Uuid) -> Self {
        self.sub = Some(id.to_string());
        self
    }

    /// Add a role.
    pub fn role(mut self, role: impl Into<String>) -> Self {
        self.roles.push(role.into());
        self
    }

    /// Set multiple roles.
    pub fn roles(mut self, roles: Vec<String>) -> Self {
        self.roles = roles;
        self
    }

    /// Add a custom claim.
    ///
    /// Rejects reserved JWT claim names to prevent duplicate-keyed tokens where
    /// structural fields (`sub`, `exp`, …) and a flattened custom key both serialize
    /// under the same JSON key — some validators read one, `ctx.claim()` reads the other.
    ///
    /// Use the typed setters instead:
    /// - `sub` / `iat` / `exp` → `.subject()` / `.user_id()` / `.duration_secs()`
    /// - `roles` → `.role()` / `.roles()`
    /// - `aud` → `.audience()`
    /// - `nbf`, `jti`, `iss` are not supported by this builder
    pub fn claim(
        mut self,
        key: impl Into<String>,
        value: serde_json::Value,
    ) -> crate::Result<Self> {
        let key = key.into();
        if Claims::RESERVED_CLAIMS.contains(&key.as_str()) {
            return Err(crate::ForgeError::InvalidArgument(format!(
                "'{key}' is a reserved JWT claim name; use the typed setter instead"
            )));
        }
        self.custom.insert(key, value);
        Ok(self)
    }

    /// Set the token audience (`aud` claim).
    ///
    /// Bypasses the reserved-name check because `aud` has no structural field in `Claims`
    /// but still needs to appear in the serialized JWT for external validators.
    pub fn audience(mut self, aud: impl Into<String>) -> Self {
        self.custom
            .insert("aud".to_string(), serde_json::json!(aud.into()));
        self
    }

    /// Set the tenant ID.
    pub fn tenant_id(mut self, id: Uuid) -> Self {
        self.custom
            .insert("tenant_id".to_string(), serde_json::json!(id.to_string()));
        self
    }

    /// Set token duration in seconds.
    pub fn duration_secs(mut self, secs: i64) -> Self {
        self.duration_secs = secs;
        self
    }

    /// Build the claims.
    pub fn build(self) -> Result<Claims, String> {
        let sub = self.sub.ok_or("Subject is required")?;
        let now = chrono::Utc::now().timestamp();

        Ok(Claims {
            sub,
            iat: now,
            exp: now + self.duration_secs,
            roles: self.roles,
            custom: self.custom,
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn test_claims_builder() {
        let user_id = Uuid::new_v4();
        let claims = Claims::builder()
            .user_id(user_id)
            .role("admin")
            .role("user")
            .claim("org_id", serde_json::json!("org-123"))
            .unwrap()
            .duration_secs(7200)
            .build()
            .unwrap();

        assert_eq!(claims.user_id(), Some(user_id));
        assert!(claims.has_role("admin"));
        assert!(claims.has_role("user"));
        assert!(!claims.has_role("superadmin"));
        assert_eq!(
            claims.get_claim("org_id"),
            Some(&serde_json::json!("org-123"))
        );
        assert!(!claims.is_expired());
    }

    #[test]
    fn claim_rejects_reserved_names() {
        for reserved in Claims::RESERVED_CLAIMS {
            let result = Claims::builder()
                .subject("user-1")
                .claim(*reserved, serde_json::json!("value"));
            assert!(
                result.is_err(),
                "Expected '{reserved}' to be rejected but it was accepted"
            );
        }
    }

    #[test]
    fn claim_accepts_custom_names() {
        let result = Claims::builder()
            .subject("user-1")
            .claim("org_id", serde_json::json!("org-123"));
        assert!(result.is_ok());
    }

    #[test]
    fn audience_sets_aud_without_error() {
        let claims = Claims::builder()
            .subject("user-1")
            .audience("my-service")
            .build()
            .unwrap();
        // aud ends up in the flattened custom map
        assert!(claims.custom.contains_key("aud"));
    }

    #[test]
    fn test_claims_expiration() {
        let claims = Claims {
            sub: "user-1".to_string(),
            iat: 0,
            exp: 1, // Expired timestamp
            roles: vec![],
            custom: HashMap::new(),
        };

        assert!(claims.is_expired());
    }

    #[test]
    fn test_claims_serialization() {
        let claims = Claims::builder()
            .subject("user-1")
            .role("admin")
            .build()
            .unwrap();

        let json = serde_json::to_string(&claims).unwrap();
        let deserialized: Claims = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.sub, claims.sub);
        assert_eq!(deserialized.roles, claims.roles);
    }
}
