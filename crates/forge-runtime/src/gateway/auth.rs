use std::sync::Arc;

use axum::{
    body::Body,
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use forge_core::auth::Claims;
use forge_core::config::JwtAlgorithm as CoreJwtAlgorithm;
use forge_core::function::AuthContext;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, dangerous, decode};
use tracing::debug;

use super::jwks::JwksClient;

/// Authentication configuration for the runtime.
#[derive(Debug, Clone)]
pub struct AuthConfig {
    /// JWT secret for HMAC algorithms (HS256, HS384, HS512).
    pub jwt_secret: Option<String>,
    /// JWT algorithm.
    pub algorithm: JwtAlgorithm,
    /// JWKS client for RSA algorithms.
    pub jwks_client: Option<Arc<JwksClient>>,
    /// Expected token issuer (iss claim).
    pub issuer: Option<String>,
    /// Expected audience (aud claim).
    pub audience: Option<String>,
    /// Whether to allow unauthenticated requests.
    pub allow_anonymous: bool,
    /// Skip signature verification (DEV MODE ONLY - NEVER USE IN PRODUCTION).
    pub skip_verification: bool,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            jwt_secret: None,
            algorithm: JwtAlgorithm::HS256,
            jwks_client: None,
            issuer: None,
            audience: None,
            allow_anonymous: true,
            skip_verification: false,
        }
    }
}

impl AuthConfig {
    /// Create auth config from forge core config.
    pub fn from_forge_config(config: &forge_core::config::AuthConfig) -> Self {
        let algorithm = JwtAlgorithm::from(config.jwt_algorithm);

        let jwks_client = config
            .jwks_url
            .as_ref()
            .map(|url| Arc::new(JwksClient::new(url.clone(), config.jwks_cache_ttl_secs)));

        Self {
            jwt_secret: config.jwt_secret.clone(),
            algorithm,
            jwks_client,
            issuer: config.jwt_issuer.clone(),
            audience: config.jwt_audience.clone(),
            allow_anonymous: config.allow_anonymous,
            skip_verification: false,
        }
    }

    /// Create a new auth config with the given HMAC secret.
    pub fn with_secret(secret: impl Into<String>) -> Self {
        Self {
            jwt_secret: Some(secret.into()),
            ..Default::default()
        }
    }

    /// Create a dev mode config that skips signature verification.
    /// WARNING: Only use this for development and testing!
    pub fn dev_mode() -> Self {
        Self {
            jwt_secret: None,
            algorithm: JwtAlgorithm::HS256,
            jwks_client: None,
            issuer: None,
            audience: None,
            allow_anonymous: true,
            skip_verification: true,
        }
    }

    /// Check if this config uses HMAC (symmetric) algorithms.
    pub fn is_hmac(&self) -> bool {
        matches!(
            self.algorithm,
            JwtAlgorithm::HS256 | JwtAlgorithm::HS384 | JwtAlgorithm::HS512
        )
    }

    /// Check if this config uses RSA (asymmetric) algorithms.
    pub fn is_rsa(&self) -> bool {
        matches!(
            self.algorithm,
            JwtAlgorithm::RS256 | JwtAlgorithm::RS384 | JwtAlgorithm::RS512
        )
    }
}

/// Supported JWT algorithms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JwtAlgorithm {
    #[default]
    HS256,
    HS384,
    HS512,
    RS256,
    RS384,
    RS512,
}

impl From<JwtAlgorithm> for Algorithm {
    fn from(alg: JwtAlgorithm) -> Self {
        match alg {
            JwtAlgorithm::HS256 => Algorithm::HS256,
            JwtAlgorithm::HS384 => Algorithm::HS384,
            JwtAlgorithm::HS512 => Algorithm::HS512,
            JwtAlgorithm::RS256 => Algorithm::RS256,
            JwtAlgorithm::RS384 => Algorithm::RS384,
            JwtAlgorithm::RS512 => Algorithm::RS512,
        }
    }
}

impl From<CoreJwtAlgorithm> for JwtAlgorithm {
    fn from(alg: CoreJwtAlgorithm) -> Self {
        match alg {
            CoreJwtAlgorithm::HS256 => JwtAlgorithm::HS256,
            CoreJwtAlgorithm::HS384 => JwtAlgorithm::HS384,
            CoreJwtAlgorithm::HS512 => JwtAlgorithm::HS512,
            CoreJwtAlgorithm::RS256 => JwtAlgorithm::RS256,
            CoreJwtAlgorithm::RS384 => JwtAlgorithm::RS384,
            CoreJwtAlgorithm::RS512 => JwtAlgorithm::RS512,
        }
    }
}

/// Authentication middleware.
#[derive(Clone)]
pub struct AuthMiddleware {
    config: Arc<AuthConfig>,
    /// Pre-computed HMAC decoding key (for performance).
    hmac_key: Option<DecodingKey>,
}

impl std::fmt::Debug for AuthMiddleware {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthMiddleware")
            .field("config", &self.config)
            .field("hmac_key", &self.hmac_key.is_some())
            .finish()
    }
}

impl AuthMiddleware {
    /// Create a new auth middleware.
    pub fn new(config: AuthConfig) -> Self {
        // Pre-compute HMAC key if using HMAC algorithm
        let hmac_key = if config.skip_verification {
            None
        } else if config.is_hmac() {
            config
                .jwt_secret
                .as_ref()
                .filter(|s| !s.is_empty())
                .map(|secret| DecodingKey::from_secret(secret.as_bytes()))
        } else {
            None
        };

        Self {
            config: Arc::new(config),
            hmac_key,
        }
    }

    /// Create a middleware that allows all requests (development mode).
    /// WARNING: This skips signature verification! Never use in production.
    pub fn permissive() -> Self {
        Self::new(AuthConfig::dev_mode())
    }

    /// Get the config.
    pub fn config(&self) -> &AuthConfig {
        &self.config
    }

    /// Validate a JWT token and extract claims.
    pub async fn validate_token_async(&self, token: &str) -> Result<Claims, AuthError> {
        if self.config.skip_verification {
            return self.decode_without_verification(token);
        }

        if self.config.is_hmac() {
            self.validate_hmac(token)
        } else {
            self.validate_rsa(token).await
        }
    }

    /// Validate HMAC-signed token.
    fn validate_hmac(&self, token: &str) -> Result<Claims, AuthError> {
        let key = self.hmac_key.as_ref().ok_or_else(|| {
            AuthError::InvalidToken("JWT secret not configured for HMAC".to_string())
        })?;

        self.decode_and_validate(token, key)
    }

    /// Validate RSA-signed token using JWKS.
    async fn validate_rsa(&self, token: &str) -> Result<Claims, AuthError> {
        let jwks = self.config.jwks_client.as_ref().ok_or_else(|| {
            AuthError::InvalidToken("JWKS URL not configured for RSA".to_string())
        })?;

        // Extract key ID from token header
        let header = jsonwebtoken::decode_header(token)
            .map_err(|e| AuthError::InvalidToken(format!("Invalid token header: {}", e)))?;

        debug!(kid = ?header.kid, alg = ?header.alg, "Validating RSA token");

        // Get key from JWKS
        let key = if let Some(kid) = header.kid {
            jwks.get_key(&kid).await.map_err(|e| {
                AuthError::InvalidToken(format!("Failed to get key '{}': {}", kid, e))
            })?
        } else {
            jwks.get_any_key()
                .await
                .map_err(|e| AuthError::InvalidToken(format!("Failed to get JWKS key: {}", e)))?
        };

        self.decode_and_validate(token, &key)
    }

    /// Decode and validate token with the given key.
    fn decode_and_validate(&self, token: &str, key: &DecodingKey) -> Result<Claims, AuthError> {
        let mut validation = Validation::new(self.config.algorithm.into());

        // Configure validation
        validation.validate_exp = true;
        validation.validate_nbf = false;
        validation.leeway = 60; // 60 seconds clock skew tolerance

        // Require exp and sub claims
        validation.set_required_spec_claims(&["exp", "sub"]);

        // Validate issuer if configured
        if let Some(ref issuer) = self.config.issuer {
            validation.set_issuer(&[issuer]);
        }

        // Validate audience if configured
        if let Some(ref audience) = self.config.audience {
            validation.set_audience(&[audience]);
        } else {
            validation.validate_aud = false;
        }

        let token_data =
            decode::<Claims>(token, key, &validation).map_err(|e| self.map_jwt_error(e))?;

        Ok(token_data.claims)
    }

    /// Map jsonwebtoken errors to AuthError.
    fn map_jwt_error(&self, e: jsonwebtoken::errors::Error) -> AuthError {
        match e.kind() {
            jsonwebtoken::errors::ErrorKind::ExpiredSignature => AuthError::TokenExpired,
            jsonwebtoken::errors::ErrorKind::InvalidSignature => {
                AuthError::InvalidToken("Invalid signature".to_string())
            }
            jsonwebtoken::errors::ErrorKind::InvalidToken => {
                AuthError::InvalidToken("Invalid token format".to_string())
            }
            jsonwebtoken::errors::ErrorKind::MissingRequiredClaim(claim) => {
                AuthError::InvalidToken(format!("Missing required claim: {}", claim))
            }
            jsonwebtoken::errors::ErrorKind::InvalidIssuer => {
                AuthError::InvalidToken("Invalid issuer".to_string())
            }
            jsonwebtoken::errors::ErrorKind::InvalidAudience => {
                AuthError::InvalidToken("Invalid audience".to_string())
            }
            _ => AuthError::InvalidToken(e.to_string()),
        }
    }

    /// Decode JWT token without signature verification (DEV MODE ONLY).
    fn decode_without_verification(&self, token: &str) -> Result<Claims, AuthError> {
        let token_data =
            dangerous::insecure_decode::<Claims>(token).map_err(|e| match e.kind() {
                jsonwebtoken::errors::ErrorKind::InvalidToken => {
                    AuthError::InvalidToken("Invalid token format".to_string())
                }
                _ => AuthError::InvalidToken(e.to_string()),
            })?;

        // Still check expiration in dev mode
        if token_data.claims.is_expired() {
            return Err(AuthError::TokenExpired);
        }

        Ok(token_data.claims)
    }
}

/// Authentication errors.
#[derive(Debug, Clone, thiserror::Error)]
pub enum AuthError {
    #[error("Missing authorization header")]
    MissingHeader,
    #[error("Invalid authorization header format")]
    InvalidHeader,
    #[error("Invalid token: {0}")]
    InvalidToken(String),
    #[error("Token expired")]
    TokenExpired,
}

/// Extract token from request headers.
pub fn extract_token(req: &Request<Body>) -> Option<String> {
    req.headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .filter(|header| header.starts_with("Bearer "))
        .map(|header| header.trim_start_matches("Bearer ").trim().to_string())
}

/// Extract auth context from token (async, supports both HMAC and RSA/JWKS).
pub async fn extract_auth_context_async(
    token: Option<String>,
    middleware: &AuthMiddleware,
) -> AuthContext {
    match token {
        Some(token) => match middleware.validate_token_async(&token).await {
            Ok(claims) => build_auth_context_from_claims(claims),
            Err(e) => {
                tracing::warn!(error = %e, "Token validation failed");
                AuthContext::unauthenticated()
            }
        },
        None => AuthContext::unauthenticated(),
    }
}

/// Build auth context from validated claims.
///
/// This handles both UUID and non-UUID subjects properly:
/// - UUID subjects: uses `authenticated()` with the parsed UUID
/// - Non-UUID subjects: uses `authenticated_without_uuid()` and stores raw subject in claims
pub fn build_auth_context_from_claims(claims: Claims) -> AuthContext {
    // Try to parse subject as UUID first (before moving claims)
    let user_id = claims.user_id();

    // Build custom claims with raw subject included
    let mut custom_claims = claims.custom;
    custom_claims.insert("sub".to_string(), serde_json::Value::String(claims.sub));

    match user_id {
        Some(uuid) => {
            // Subject is a valid UUID
            AuthContext::authenticated(uuid, claims.roles, custom_claims)
        }
        None => {
            // Subject is not a UUID (e.g., Firebase uid, Clerk user_xxx, email)
            // Still authenticated, but user_id() will return None
            AuthContext::authenticated_without_uuid(claims.roles, custom_claims)
        }
    }
}

/// Authentication middleware function.
pub async fn auth_middleware(
    State(middleware): State<Arc<AuthMiddleware>>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let token = extract_token(&req);
    tracing::trace!(
        token_present = token.is_some(),
        "Auth middleware processing request"
    );

    let auth_context = extract_auth_context_async(token, &middleware).await;
    tracing::trace!(
        authenticated = auth_context.is_authenticated(),
        "Auth context created"
    );

    let mut req = req;
    req.extensions_mut().insert(auth_context);

    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{EncodingKey, Header, encode};

    fn create_test_claims(expired: bool) -> Claims {
        use forge_core::auth::ClaimsBuilder;

        let mut builder = ClaimsBuilder::new().subject("test-user-id").role("user");

        if expired {
            builder = builder.duration_secs(-3600); // Expired 1 hour ago
        } else {
            builder = builder.duration_secs(3600); // Valid for 1 hour
        }

        builder.build().unwrap()
    }

    fn create_test_token(claims: &Claims, secret: &str) -> String {
        encode(
            &Header::default(),
            claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .unwrap()
    }

    #[test]
    fn test_auth_config_default() {
        let config = AuthConfig::default();
        assert!(config.allow_anonymous);
        assert_eq!(config.algorithm, JwtAlgorithm::HS256);
        assert!(!config.skip_verification);
    }

    #[test]
    fn test_auth_config_dev_mode() {
        let config = AuthConfig::dev_mode();
        assert!(config.skip_verification);
        assert!(config.allow_anonymous);
    }

    #[test]
    fn test_auth_middleware_permissive() {
        let middleware = AuthMiddleware::permissive();
        assert!(middleware.config.skip_verification);
    }

    #[tokio::test]
    async fn test_valid_token_with_correct_secret() {
        let secret = "test-secret-key";
        let config = AuthConfig::with_secret(secret);
        let middleware = AuthMiddleware::new(config);

        let claims = create_test_claims(false);
        let token = create_test_token(&claims, secret);

        let result = middleware.validate_token_async(&token).await;
        assert!(result.is_ok());
        let validated_claims = result.unwrap();
        assert_eq!(validated_claims.sub, "test-user-id");
    }

    #[tokio::test]
    async fn test_valid_token_with_wrong_secret() {
        let config = AuthConfig::with_secret("correct-secret");
        let middleware = AuthMiddleware::new(config);

        let claims = create_test_claims(false);
        let token = create_test_token(&claims, "wrong-secret");

        let result = middleware.validate_token_async(&token).await;
        assert!(result.is_err());
        match result {
            Err(AuthError::InvalidToken(_)) => {}
            _ => panic!("Expected InvalidToken error"),
        }
    }

    #[tokio::test]
    async fn test_expired_token() {
        let secret = "test-secret";
        let config = AuthConfig::with_secret(secret);
        let middleware = AuthMiddleware::new(config);

        let claims = create_test_claims(true); // Expired
        let token = create_test_token(&claims, secret);

        let result = middleware.validate_token_async(&token).await;
        assert!(result.is_err());
        match result {
            Err(AuthError::TokenExpired) => {}
            _ => panic!("Expected TokenExpired error"),
        }
    }

    #[tokio::test]
    async fn test_tampered_token() {
        let secret = "test-secret";
        let config = AuthConfig::with_secret(secret);
        let middleware = AuthMiddleware::new(config);

        let claims = create_test_claims(false);
        let mut token = create_test_token(&claims, secret);

        // Tamper with the token by modifying a character in the signature
        if let Some(last_char) = token.pop() {
            let replacement = if last_char == 'a' { 'b' } else { 'a' };
            token.push(replacement);
        }

        let result = middleware.validate_token_async(&token).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_dev_mode_skips_signature() {
        let config = AuthConfig::dev_mode();
        let middleware = AuthMiddleware::new(config);

        // Create token with any secret
        let claims = create_test_claims(false);
        let token = create_test_token(&claims, "any-secret");

        // Should still validate in dev mode
        let result = middleware.validate_token_async(&token).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_dev_mode_still_checks_expiration() {
        let config = AuthConfig::dev_mode();
        let middleware = AuthMiddleware::new(config);

        let claims = create_test_claims(true); // Expired
        let token = create_test_token(&claims, "any-secret");

        let result = middleware.validate_token_async(&token).await;
        assert!(result.is_err());
        match result {
            Err(AuthError::TokenExpired) => {}
            _ => panic!("Expected TokenExpired error even in dev mode"),
        }
    }

    #[tokio::test]
    async fn test_invalid_token_format() {
        let config = AuthConfig::with_secret("secret");
        let middleware = AuthMiddleware::new(config);

        let result = middleware.validate_token_async("not-a-valid-jwt").await;
        assert!(result.is_err());
        match result {
            Err(AuthError::InvalidToken(_)) => {}
            _ => panic!("Expected InvalidToken error"),
        }
    }

    #[test]
    fn test_algorithm_conversion() {
        // HMAC algorithms
        assert_eq!(Algorithm::from(JwtAlgorithm::HS256), Algorithm::HS256);
        assert_eq!(Algorithm::from(JwtAlgorithm::HS384), Algorithm::HS384);
        assert_eq!(Algorithm::from(JwtAlgorithm::HS512), Algorithm::HS512);
        // RSA algorithms
        assert_eq!(Algorithm::from(JwtAlgorithm::RS256), Algorithm::RS256);
        assert_eq!(Algorithm::from(JwtAlgorithm::RS384), Algorithm::RS384);
        assert_eq!(Algorithm::from(JwtAlgorithm::RS512), Algorithm::RS512);
    }

    #[test]
    fn test_is_hmac_and_is_rsa() {
        let hmac_config = AuthConfig::with_secret("test");
        assert!(hmac_config.is_hmac());
        assert!(!hmac_config.is_rsa());

        let rsa_config = AuthConfig {
            algorithm: JwtAlgorithm::RS256,
            ..Default::default()
        };
        assert!(!rsa_config.is_hmac());
        assert!(rsa_config.is_rsa());
    }
}
