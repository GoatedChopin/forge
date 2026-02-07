//! Shared utility functions for the Forge framework.

use std::time::Duration;

/// Parse a duration string into `Duration`.
///
/// Supports the following suffixes:
/// - `ms` - milliseconds
/// - `s` - seconds
/// - `m` - minutes
/// - `h` - hours
/// - `d` - days
///
/// If no suffix is provided, the value is interpreted as seconds.
pub fn parse_duration(s: &str) -> Option<Duration> {
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

/// Convert a PascalCase or camelCase string to snake_case.
pub fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.extend(c.to_lowercase());
        } else {
            result.push(c);
        }
    }
    result
}

/// Convert a snake_case string to camelCase.
pub fn to_camel_case(s: &str) -> String {
    let mut result = String::new();
    let mut capitalize_next = false;
    for c in s.chars() {
        if c == '_' {
            capitalize_next = true;
        } else if capitalize_next {
            result.extend(c.to_uppercase());
            capitalize_next = false;
        } else {
            result.push(c);
        }
    }
    result
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_duration_milliseconds() {
        assert_eq!(parse_duration("100ms"), Some(Duration::from_millis(100)));
        assert_eq!(parse_duration("1000ms"), Some(Duration::from_millis(1000)));
    }

    #[test]
    fn test_parse_duration_seconds() {
        assert_eq!(parse_duration("30s"), Some(Duration::from_secs(30)));
        assert_eq!(parse_duration("60s"), Some(Duration::from_secs(60)));
    }

    #[test]
    fn test_parse_duration_minutes() {
        assert_eq!(parse_duration("5m"), Some(Duration::from_secs(300)));
        assert_eq!(parse_duration("10m"), Some(Duration::from_secs(600)));
    }

    #[test]
    fn test_parse_duration_hours() {
        assert_eq!(parse_duration("1h"), Some(Duration::from_secs(3600)));
        assert_eq!(parse_duration("24h"), Some(Duration::from_secs(86400)));
    }

    #[test]
    fn test_parse_duration_days() {
        assert_eq!(parse_duration("1d"), Some(Duration::from_secs(86400)));
        assert_eq!(parse_duration("7d"), Some(Duration::from_secs(604800)));
    }

    #[test]
    fn test_parse_duration_bare_number() {
        assert_eq!(parse_duration("60"), Some(Duration::from_secs(60)));
        assert_eq!(parse_duration("3600"), Some(Duration::from_secs(3600)));
    }

    #[test]
    fn test_parse_duration_whitespace() {
        assert_eq!(parse_duration("  30s  "), Some(Duration::from_secs(30)));
    }

    #[test]
    fn test_parse_duration_invalid() {
        assert_eq!(parse_duration("invalid"), None);
        assert_eq!(parse_duration("abc123"), None);
        assert_eq!(parse_duration(""), None);
    }

    #[test]
    fn test_to_snake_case() {
        assert_eq!(to_snake_case("GetUser"), "get_user");
        assert_eq!(to_snake_case("ListAllProjects"), "list_all_projects");
        assert_eq!(to_snake_case("Simple"), "simple");
        assert_eq!(to_snake_case("ProjectStatus"), "project_status");
    }

    #[test]
    fn test_to_pascal_case() {
        assert_eq!(to_pascal_case("get_user"), "GetUser");
        assert_eq!(to_pascal_case("list_all_projects"), "ListAllProjects");
        assert_eq!(to_pascal_case("simple"), "Simple");
    }

    #[test]
    fn test_to_camel_case() {
        assert_eq!(to_camel_case("get_user"), "getUser");
        assert_eq!(to_camel_case("list_all_projects"), "listAllProjects");
        assert_eq!(to_camel_case("simple"), "simple");
    }
}
