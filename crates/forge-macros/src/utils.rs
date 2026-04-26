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
/// Bare integers without a unit suffix are rejected.
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
        // Bare integers without a unit suffix are not accepted. Require explicit
        // suffixes (e.g. "30s") so intent is unambiguous at the macro callsite.
        None
    }
}

/// Parse a duration string into seconds.
/// Returns None if the string cannot be parsed or has no unit suffix.
pub fn parse_duration_secs(s: &str) -> Option<u64> {
    parse_duration(s).map(|d| d.as_secs())
}

/// Parse a duration string into a TokenStream representing std::time::Duration.
/// Emits a `compile_error!` if the string has no recognized unit suffix.
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
        let msg = format!(
            "invalid duration \"{}\": use a suffix like \"30s\", \"5m\", or \"1h\"",
            s
        );
        quote! { compile_error!(#msg) }
    }
}

/// Parse a human-readable size string into bytes.
/// Returns None if the string cannot be parsed.
pub fn parse_size_bytes(s: &str) -> Option<usize> {
    let s = s.trim().to_lowercase();
    if let Some(num) = s.strip_suffix("gb") {
        num.trim()
            .parse::<usize>()
            .ok()
            .map(|n| n * 1024 * 1024 * 1024)
    } else if let Some(num) = s.strip_suffix("mb") {
        num.trim().parse::<usize>().ok().map(|n| n * 1024 * 1024)
    } else if let Some(num) = s.strip_suffix("kb") {
        num.trim().parse::<usize>().ok().map(|n| n * 1024)
    } else if let Some(num) = s.strip_suffix('b') {
        num.trim().parse::<usize>().ok()
    } else {
        s.parse::<usize>().ok()
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

/// Extract the top-level attribute key identifiers from an attribute string.
///
/// Recognizes flag form (`public`), key=value form (`cache = "5m"`), and
/// key(args) form (`rate_limit(...)`). Once a `=` is seen at top level, the
/// scanner skips the value expression until the next top-level comma so
/// values like `WebhookSignature::hmac_sha256("X", "Y")` don't leak in.
pub fn extract_top_level_keys(attr_str: &str) -> Vec<String> {
    let bytes = attr_str.as_bytes();
    let mut keys = Vec::new();
    let mut i = 0usize;
    let mut depth: u32 = 0;
    let mut in_quote: Option<u8> = None;
    let mut escaped = false;
    let mut expecting_key = true;

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

        if b == b'(' || b == b'[' || b == b'{' {
            depth += 1;
            i += 1;
            continue;
        }
        if b == b')' || b == b']' || b == b'}' {
            depth = depth.saturating_sub(1);
            i += 1;
            continue;
        }

        if depth == 0 && b == b',' {
            expecting_key = true;
            i += 1;
            continue;
        }

        if depth == 0 && b == b'=' {
            expecting_key = false;
            i += 1;
            continue;
        }

        if depth == 0 && expecting_key && (b.is_ascii_alphabetic() || b == b'_') {
            let start = i;
            while i < bytes.len() && is_ident_char(bytes[i]) {
                i += 1;
            }
            let key = &attr_str[start..i];
            if !key.is_empty() {
                keys.push(key.to_string());
            }
            // Stay in expecting_key until we hit `=` or `,`. Reaching `(` will
            // bump depth, so the args don't get scanned for keys here.
            continue;
        }

        i += 1;
    }

    keys
}

/// Suggest the closest allowed key for a misspelling, if any is close enough.
fn suggest_closest(unknown: &str, allowed: &[&str]) -> Option<String> {
    let mut best: Option<(usize, &str)> = None;
    for &candidate in allowed {
        let dist = levenshtein(unknown, candidate);
        if dist <= 2.max(candidate.len() / 3) && best.map(|(d, _)| dist < d).unwrap_or(true) {
            best = Some((dist, candidate));
        }
    }
    best.map(|(_, c)| c.to_string())
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr: Vec<usize> = vec![0; b.len() + 1];
    for i in 1..=a.len() {
        curr[0] = i;
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}

/// Validate that every top-level attribute key is in the allowlist.
/// Returns a `syn::Error` with a "did you mean" hint on the first unknown key.
pub fn validate_attr_keys(attr_str: &str, allowed: &[&str], macro_name: &str) -> syn::Result<()> {
    for key in extract_top_level_keys(attr_str) {
        if allowed.contains(&key.as_str()) {
            continue;
        }
        let msg = match suggest_closest(&key, allowed) {
            Some(hint) => {
                format!("Unknown attribute `{key}` for #[{macro_name}]. Did you mean `{hint}`?")
            }
            None => format!(
                "Unknown attribute `{key}` for #[{macro_name}]. Allowed: {}",
                allowed.join(", ")
            ),
        };
        return Err(syn::Error::new(proc_macro2::Span::call_site(), msg));
    }
    Ok(())
}

/// Reject use of attribute keys that are reserved for future Forge releases.
///
/// These keys are accepted by the parser (so adding behavior later is non-breaking)
/// but trigger a hard compile error when used today. Strict mode prevents apps from
/// thinking a feature works that doesn't.
pub fn reject_reserved_keys(
    attr_str: &str,
    reserved: &[&str],
    macro_name: &str,
) -> syn::Result<()> {
    for key in extract_top_level_keys(attr_str) {
        if reserved.contains(&key.as_str()) {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                format!(
                    "Attribute `{key}` is reserved for a future Forge release and is not yet \
                     implemented. Remove it from #[{macro_name}] until support lands."
                ),
            ));
        }
    }
    Ok(())
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
        // Bare integers are rejected — unit suffix required.
        assert_eq!(parse_duration_secs("60"), None);
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

        let ts = parse_duration_tokens("30", 30);
        let output = ts.to_string();
        assert!(
            output.contains("compile_error"),
            "bare integer should emit compile_error, got: {output}"
        );
    }

    #[test]
    fn test_parse_size_bytes() {
        assert_eq!(parse_size_bytes("100mb"), Some(100 * 1024 * 1024));
        assert_eq!(parse_size_bytes("1gb"), Some(1024 * 1024 * 1024));
        assert_eq!(parse_size_bytes("512kb"), Some(512 * 1024));
        assert_eq!(parse_size_bytes("1024b"), Some(1024));
        assert_eq!(parse_size_bytes("200MB"), Some(200 * 1024 * 1024));
        assert_eq!(parse_size_bytes("1048576"), Some(1048576));
        assert_eq!(parse_size_bytes("invalid"), None);
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

    #[test]
    fn extract_keys_handles_flags_and_kv_and_calls() {
        let attr = r#"public, timeout = 30, rate_limit(requests = 100, per = "60s")"#;
        let keys = extract_top_level_keys(attr);
        assert_eq!(keys, vec!["public", "timeout", "rate_limit"]);
    }

    #[test]
    fn extract_keys_skips_value_side_path_expressions() {
        let attr =
            r#"path = "/x", signature = WebhookSignature::hmac_sha256("H", "S"), timeout = "30s""#;
        let keys = extract_top_level_keys(attr);
        assert_eq!(keys, vec!["path", "signature", "timeout"]);
    }

    #[test]
    fn validate_keys_accepts_known() {
        let attr = r#"public, cache = "5m""#;
        assert!(validate_attr_keys(attr, &["public", "cache"], "query").is_ok());
    }

    #[test]
    fn validate_keys_rejects_misspelling_with_hint() {
        let attr = r#"cach = "5m""#;
        let err = validate_attr_keys(attr, &["cache", "timeout"], "query").unwrap_err();
        assert!(
            err.to_string().contains("cach") && err.to_string().contains("cache"),
            "{err}"
        );
    }

    #[test]
    fn validate_keys_rejects_completely_unknown() {
        let attr = r#"completely_unrelated = 1"#;
        let err = validate_attr_keys(attr, &["cache", "timeout"], "query").unwrap_err();
        assert!(err.to_string().contains("Allowed:"), "{err}");
    }

    #[test]
    fn reject_reserved_keys_passes_when_unused() {
        let attr = r#"public, cache = "5m""#;
        assert!(reject_reserved_keys(attr, &["debounce_ms", "max_rows"], "query").is_ok());
    }

    #[test]
    fn reject_reserved_keys_errors_with_helpful_message() {
        let attr = r#"public, debounce_ms = 50"#;
        let err = reject_reserved_keys(attr, &["debounce_ms", "max_rows"], "query").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("debounce_ms"), "{msg}");
        assert!(msg.contains("reserved"), "{msg}");
        assert!(msg.contains("query"), "{msg}");
    }
}
