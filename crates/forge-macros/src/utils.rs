//! Shared utility functions for forge macros.
//!
//! Re-exports from forge-utils with proc-macro-specific helpers.

use proc_macro2::TokenStream;
use quote::quote;

#[allow(unused_imports)]
pub use forge_utils::{to_camel_case, to_pascal_case, to_snake_case};

/// Parse a duration string (e.g., "30s", "5m", "1h") into seconds.
/// Returns None if the string cannot be parsed.
pub fn parse_duration_secs(s: &str) -> Option<u64> {
    forge_utils::parse_duration(s).map(|d| d.as_secs())
}

/// Parse a duration string into a TokenStream representing std::time::Duration.
/// Falls back to the provided default_secs if parsing fails.
pub fn parse_duration_tokens(s: &str, default_secs: u64) -> TokenStream {
    let s = s.trim();
    if s.ends_with("ms") {
        let n: u64 = s
            .trim_end_matches("ms")
            .parse()
            .unwrap_or(default_secs * 1000);
        quote! { std::time::Duration::from_millis(#n) }
    } else if s.ends_with('s') {
        let n: u64 = s.trim_end_matches('s').parse().unwrap_or(default_secs);
        quote! { std::time::Duration::from_secs(#n) }
    } else if s.ends_with('m') {
        let n: u64 = s.trim_end_matches('m').parse().unwrap_or(default_secs / 60);
        let secs = n * 60;
        quote! { std::time::Duration::from_secs(#secs) }
    } else if s.ends_with('h') {
        let n: u64 = s
            .trim_end_matches('h')
            .parse()
            .unwrap_or(default_secs / 3600);
        let secs = n * 3600;
        quote! { std::time::Duration::from_secs(#secs) }
    } else if s.ends_with('d') {
        let n: u64 = s
            .trim_end_matches('d')
            .parse()
            .unwrap_or(default_secs / 86400);
        let secs = n * 86400;
        quote! { std::time::Duration::from_secs(#secs) }
    } else {
        let n: u64 = s.parse().unwrap_or(default_secs);
        quote! { std::time::Duration::from_secs(#n) }
    }
}

/// Check whether an attribute string contains a standalone flag identifier.
///
/// This avoids false positives from substring matching inside quoted values,
/// e.g. `require_role("public_api")` should not match `public`.
pub fn has_attr_flag(attr_str: &str, flag: &str) -> bool {
    if flag.is_empty() {
        return false;
    }

    let bytes = attr_str.as_bytes();
    let flag_bytes = flag.as_bytes();
    let mut i = 0usize;
    let mut in_quote: Option<u8> = None;
    let mut escaped = false;

    while i < bytes.len() {
        let b = bytes[i];

        if let Some(q) = in_quote {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == q {
                in_quote = None;
            }
            i += 1;
            continue;
        }

        if b == b'"' || b == b'\'' {
            in_quote = Some(b);
            i += 1;
            continue;
        }

        if i + flag_bytes.len() <= bytes.len() && &bytes[i..i + flag_bytes.len()] == flag_bytes {
            let prev = if i == 0 { None } else { Some(bytes[i - 1]) };
            let next = if i + flag_bytes.len() < bytes.len() {
                Some(bytes[i + flag_bytes.len()])
            } else {
                None
            };

            let prev_is_ident = prev.is_some_and(is_ident_char);
            let next_is_ident = next.is_some_and(is_ident_char);
            if !prev_is_ident && !next_is_ident {
                return true;
            }
        }

        i += 1;
    }

    false
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_pascal_case() {
        assert_eq!(to_pascal_case("get_user"), "GetUser");
        assert_eq!(to_pascal_case("list_all_projects"), "ListAllProjects");
        assert_eq!(to_pascal_case("simple"), "Simple");
        assert_eq!(to_pascal_case("send_welcome_email"), "SendWelcomeEmail");
    }

    #[test]
    fn test_to_snake_case() {
        assert_eq!(to_snake_case("GetUser"), "get_user");
        assert_eq!(to_snake_case("ListAllProjects"), "list_all_projects");
        assert_eq!(to_snake_case("Simple"), "simple");
        assert_eq!(to_snake_case("ProjectStatus"), "project_status");
    }

    #[test]
    fn test_to_camel_case() {
        assert_eq!(to_camel_case("get_user"), "getUser");
        assert_eq!(to_camel_case("list_all_projects"), "listAllProjects");
        assert_eq!(to_camel_case("simple"), "simple");
    }

    #[test]
    fn test_parse_duration_secs() {
        assert_eq!(parse_duration_secs("30s"), Some(30));
        assert_eq!(parse_duration_secs("5m"), Some(300));
        assert_eq!(parse_duration_secs("1h"), Some(3600));
        assert_eq!(parse_duration_secs("2d"), Some(172800));
        assert_eq!(parse_duration_secs("60"), Some(60));
        assert_eq!(parse_duration_secs("1000ms"), Some(1));
        assert_eq!(parse_duration_secs("invalid"), None);
    }

    #[test]
    fn test_parse_duration_tokens() {
        let ts = parse_duration_tokens("30s", 30);
        assert!(!ts.is_empty());

        let ts = parse_duration_tokens("5m", 300);
        assert!(!ts.is_empty());

        let ts = parse_duration_tokens("1h", 3600);
        assert!(!ts.is_empty());
    }

    #[test]
    fn test_has_attr_flag_matches_standalone() {
        assert!(has_attr_flag("public, timeout = 30", "public"));
        assert!(has_attr_flag(
            "transactional, require_role(\"admin\")",
            "transactional"
        ));
    }

    #[test]
    fn test_has_attr_flag_ignores_substrings_and_quotes() {
        assert!(!has_attr_flag("require_role(\"public_api\")", "public"));
        assert!(!has_attr_flag("my_public_flag", "public"));
        assert!(!has_attr_flag("public_api = true", "public"));
    }
}
