use std::time::Duration;

/// Configuration for webhook signature validation.
#[derive(Debug, Clone)]
pub struct SignatureConfig {
    /// Algorithm used for signature verification.
    pub algorithm: SignatureAlgorithm,
    /// Header name containing the signature.
    pub header_name: &'static str,
    /// Environment variable name containing the secret.
    pub secret_env: &'static str,
}

/// Supported signature algorithms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureAlgorithm {
    /// HMAC-SHA256 (e.g., Stripe, GitHub)
    HmacSha256,
    /// HMAC-SHA1 (legacy, e.g., older GitHub)
    HmacSha1,
    /// HMAC-SHA512
    HmacSha512,
}

impl SignatureAlgorithm {
    /// Get the algorithm prefix used in signatures (e.g., "sha256=").
    pub fn prefix(&self) -> &'static str {
        match self {
            Self::HmacSha256 => "sha256=",
            Self::HmacSha1 => "sha1=",
            Self::HmacSha512 => "sha512=",
        }
    }
}

/// Source for extracting idempotency key.
#[derive(Debug, Clone)]
pub enum IdempotencySource {
    /// Extract from a header (e.g., "X-Request-Id").
    Header(&'static str),
    /// Extract from request body using JSONPath (e.g., "$.id").
    Body(&'static str),
}

impl IdempotencySource {
    /// Parse from attribute string (e.g., "header:X-Request-Id" or "body:$.id").
    pub fn parse(s: &str) -> Option<Self> {
        let (prefix, value) = s.split_once(':')?;
        match prefix {
            "header" => Some(Self::Header(Box::leak(value.to_string().into_boxed_str()))),
            "body" => Some(Self::Body(Box::leak(value.to_string().into_boxed_str()))),
            _ => None,
        }
    }
}

/// Configuration for webhook idempotency.
#[derive(Debug, Clone)]
pub struct IdempotencyConfig {
    /// Source for the idempotency key.
    pub source: IdempotencySource,
    /// TTL for idempotency records (default: 24 hours).
    pub ttl: Duration,
}

impl IdempotencyConfig {
    /// Create a new idempotency config with default TTL.
    pub fn new(source: IdempotencySource) -> Self {
        Self {
            source,
            ttl: Duration::from_secs(24 * 60 * 60), // 24 hours
        }
    }

    /// Set a custom TTL.
    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = ttl;
        self
    }
}

/// Helper for constructing signature configurations.
///
/// Use in webhook attributes like:
/// ```ignore
/// #[forge::webhook(
///     signature = WebhookSignature::hmac_sha256("X-Stripe-Signature", "STRIPE_SECRET"),
/// )]
/// ```
pub struct WebhookSignature;

impl WebhookSignature {
    /// Create HMAC-SHA256 signature config.
    ///
    /// # Arguments
    /// * `header` - The HTTP header containing the signature (e.g., "X-Hub-Signature-256")
    /// * `secret_env` - Environment variable containing the secret
    pub const fn hmac_sha256(header: &'static str, secret_env: &'static str) -> SignatureConfig {
        SignatureConfig {
            algorithm: SignatureAlgorithm::HmacSha256,
            header_name: header,
            secret_env,
        }
    }

    /// Create HMAC-SHA1 signature config.
    ///
    /// # Arguments
    /// * `header` - The HTTP header containing the signature
    /// * `secret_env` - Environment variable containing the secret
    pub const fn hmac_sha1(header: &'static str, secret_env: &'static str) -> SignatureConfig {
        SignatureConfig {
            algorithm: SignatureAlgorithm::HmacSha1,
            header_name: header,
            secret_env,
        }
    }

    /// Create HMAC-SHA512 signature config.
    ///
    /// # Arguments
    /// * `header` - The HTTP header containing the signature
    /// * `secret_env` - Environment variable containing the secret
    pub const fn hmac_sha512(header: &'static str, secret_env: &'static str) -> SignatureConfig {
        SignatureConfig {
            algorithm: SignatureAlgorithm::HmacSha512,
            header_name: header,
            secret_env,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn test_signature_config_creation() {
        let config = WebhookSignature::hmac_sha256("X-Hub-Signature-256", "GITHUB_SECRET");
        assert_eq!(config.algorithm, SignatureAlgorithm::HmacSha256);
        assert_eq!(config.header_name, "X-Hub-Signature-256");
        assert_eq!(config.secret_env, "GITHUB_SECRET");
    }

    #[test]
    fn test_algorithm_prefix() {
        assert_eq!(SignatureAlgorithm::HmacSha256.prefix(), "sha256=");
        assert_eq!(SignatureAlgorithm::HmacSha1.prefix(), "sha1=");
        assert_eq!(SignatureAlgorithm::HmacSha512.prefix(), "sha512=");
    }

    #[test]
    fn test_idempotency_source_parsing() {
        let header = IdempotencySource::parse("header:X-Request-Id");
        assert!(matches!(
            header,
            Some(IdempotencySource::Header("X-Request-Id"))
        ));

        let body = IdempotencySource::parse("body:$.id");
        assert!(matches!(body, Some(IdempotencySource::Body("$.id"))));

        let invalid = IdempotencySource::parse("invalid");
        assert!(invalid.is_none());
    }

    #[test]
    fn test_idempotency_config_default_ttl() {
        let config = IdempotencyConfig::new(IdempotencySource::Header("X-Id"));
        assert_eq!(config.ttl, Duration::from_secs(24 * 60 * 60));
    }
}
