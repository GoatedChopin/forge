//! OAuth 2.1 Authorization Code + PKCE support.
//!
//! Forge acts as an OAuth 2.1 Authorization Server for its MCP endpoint.
//! Enable with `mcp.oauth = true` in `forge.toml`.

pub mod pkce;

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use uuid::Uuid;

/// An OAuth 2.1 client registration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OAuthClient {
    pub client_id: String,
    pub client_name: Option<String>,
    pub redirect_uris: Vec<String>,
    pub token_endpoint_auth_method: String,
    pub created_at: DateTime<Utc>,
}

/// A pending authorization code with PKCE challenge.
#[derive(Debug, Clone)]
pub struct AuthorizationCode {
    pub code: String,
    pub client_id: String,
    pub user_id: Uuid,
    pub redirect_uri: String,
    pub code_challenge: String,
    pub code_challenge_method: String,
    pub scopes: Vec<String>,
    pub expires_at: DateTime<Utc>,
}

/// 256 bits of CSPRNG entropy (two UUIDv4s), base64url-encoded.
pub fn generate_random_token() -> String {
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    let mut bytes = [0u8; 32];
    bytes[..16].copy_from_slice(a.as_bytes());
    bytes[16..].copy_from_slice(b.as_bytes());
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Validate a redirect URI against a client's registered URIs.
///
/// Uses exact string match per OAuth 2.1, with RFC 8252 localhost exception:
/// for `http://localhost` URIs, the port is allowed to differ.
pub fn validate_redirect_uri(requested: &str, registered: &[String]) -> bool {
    for uri in registered {
        if requested == uri {
            return true;
        }
        // RFC 8252 Section 7.3: localhost with any port is allowed
        if is_localhost_uri(uri) && is_localhost_uri(requested) {
            let (scheme_host_a, path_a) = split_localhost_uri(uri);
            let (scheme_host_b, path_b) = split_localhost_uri(requested);
            if scheme_host_a == scheme_host_b && path_a == path_b {
                return true;
            }
        }
    }
    false
}

fn is_localhost_uri(uri: &str) -> bool {
    uri.starts_with("http://localhost") || uri.starts_with("http://127.0.0.1")
}

/// Split a localhost URI into (scheme+host, path), stripping the port.
/// e.g. "http://localhost:12345/callback" -> ("http://localhost", "/callback")
fn split_localhost_uri(uri: &str) -> (&str, &str) {
    let scheme_end = if uri.starts_with("http://localhost") {
        "http://localhost".len()
    } else {
        "http://127.0.0.1".len()
    };
    let rest = &uri[scheme_end..];
    // Skip the port if present
    let path_start = rest.find('/').unwrap_or(rest.len());
    let path = &rest[path_start..];
    let scheme_host = &uri[..scheme_end];
    (scheme_host, path)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_random_token_length() {
        let code = generate_random_token();
        assert_eq!(code.len(), 43); // 32 bytes -> 43 base64url chars
    }

    #[test]
    fn test_generate_random_token_uniqueness() {
        let a = generate_random_token();
        let b = generate_random_token();
        assert_ne!(a, b);
    }

    #[test]
    fn test_validate_redirect_uri_exact_match() {
        let registered = vec!["https://example.com/callback".to_string()];
        assert!(validate_redirect_uri("https://example.com/callback", &registered));
        assert!(!validate_redirect_uri("https://example.com/other", &registered));
    }

    #[test]
    fn test_validate_redirect_uri_localhost_port_exception() {
        let registered = vec!["http://localhost:3000/callback".to_string()];
        assert!(validate_redirect_uri(
            "http://localhost:9999/callback",
            &registered
        ));
        assert!(validate_redirect_uri(
            "http://localhost:3000/callback",
            &registered
        ));
    }

    #[test]
    fn test_validate_redirect_uri_localhost_different_path() {
        let registered = vec!["http://localhost:3000/callback".to_string()];
        assert!(!validate_redirect_uri(
            "http://localhost:9999/other",
            &registered
        ));
    }

    #[test]
    fn test_validate_redirect_uri_no_localhost_exception_for_https() {
        let registered = vec!["https://localhost:3000/callback".to_string()];
        assert!(!validate_redirect_uri(
            "https://localhost:9999/callback",
            &registered
        ));
    }

    #[test]
    fn test_validate_redirect_uri_127_0_0_1() {
        let registered = vec!["http://127.0.0.1:3000/callback".to_string()];
        assert!(validate_redirect_uri(
            "http://127.0.0.1:9999/callback",
            &registered
        ));
    }
}
