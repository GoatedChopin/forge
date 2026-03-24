//! Shared utility functions for forge macros.

use std::time::Duration;

use proc_macro2::TokenStream;
use quote::quote;

/// Convert a snake_case string to PascalCase.
pub fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().chain(chars).collect(),
            }
        })
        .collect()
}

/// Parse a duration string (e.g., "30s", "5m", "1h") into a `Duration`.
fn parse_duration(s: &str) -> Option<Duration> {
    let s = s.trim();
    if let Some(num) = s.strip_suffix("ms") {
        num.parse::<u64>().ok().map(Duration::from_millis)
    } else if let Some(num) = s.strip_suffix('s') {
        num.parse::<u64>().ok().map(Duration::from_secs)
    } else if let Some(num) = s.strip_suffix('m') {
        num.parse::<u64>().ok().map(|m| Duration::from_secs(m * 60))
    } else if let Some(num) = s.strip_suffix('h') {
        num.parse::<u64>()
            .ok()
            .map(|h| Duration::from_secs(h * 3600))
    } else if let Some(num) = s.strip_suffix('d') {
        num.parse::<u64>()
            .ok()
            .map(|d| Duration::from_secs(d * 86400))
    } else {
        s.parse::<u64>().ok().map(Duration::from_secs)
    }
}

/// Parse a duration string into seconds.
/// Returns None if the string cannot be parsed.
pub fn parse_duration_secs(s: &str) -> Option<u64> {
    parse_duration(s).map(|d| d.as_secs())
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
    find_attr_key(attr_str, flag).is_some()
}

/// Find a standalone attribute key outside quoted strings.
///
/// Returns the byte index of the first match if found.
pub fn find_attr_key(attr_str: &str, key: &str) -> Option<usize> {
    if key.is_empty() {
        return None;
    }

    let bytes = attr_str.as_bytes();
    let flag_bytes = key.as_bytes();
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
                return Some(i);
            }
        }

        i += 1;
    }

    None
}

/// Parse a named attribute value, supporting quoted strings or bare tokens.
pub fn parse_attr_value(attr_str: &str, key: &str) -> Option<String> {
    let key_start = find_attr_key(attr_str, key)?;
    let eq_pos = attr_str[key_start..].find('=')?;
    let remaining = attr_str[key_start + eq_pos + 1..].trim_start();

    if let Some(after_quote) = remaining.strip_prefix('"') {
        let quote_end = after_quote.find('"')?;
        return Some(after_quote[..quote_end].to_string());
    }

    Some(
        remaining
            .split(&[',', ')'])
            .next()
            .unwrap_or("")
            .trim()
            .trim_matches('"')
            .to_string(),
    )
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

    #[test]
    fn test_find_attr_key_matches_exact_key() {
        let attr = r#"max_timeout = "5s", timeout = 30"#;
        let timeout_idx = find_attr_key(attr, "timeout").unwrap();
        let max_timeout_idx = find_attr_key(attr, "max_timeout").unwrap();

        assert!(max_timeout_idx < timeout_idx);
        assert_eq!(&attr[timeout_idx..timeout_idx + "timeout".len()], "timeout");
    }

    #[test]
    fn test_parse_attr_value_supports_quoted_and_bare_values() {
        let attr = r#"timeout = 30, max_timeout = "5s""#;

        assert_eq!(parse_attr_value(attr, "timeout").as_deref(), Some("30"));
        assert_eq!(parse_attr_value(attr, "max_timeout").as_deref(), Some("5s"));
    }
}
