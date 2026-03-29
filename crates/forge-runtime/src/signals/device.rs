//! Device classification from User-Agent strings and platform headers.
//!
//! Extracts device_type, browser, and os from the raw User-Agent header
//! and the optional `x-forge-platform` header sent by client SDKs.

/// Parsed device info for a request.
pub struct DeviceInfo {
    pub device_type: Option<String>,
    pub browser: Option<String>,
    pub os: Option<String>,
}

/// Parse device info from the `x-forge-platform` header and User-Agent.
///
/// The platform header is authoritative for device_type when present (the
/// client SDK knows whether it's running as web, desktop, or mobile).
/// Browser and OS are always parsed from the User-Agent.
pub fn parse(platform_header: Option<&str>, user_agent: Option<&str>) -> DeviceInfo {
    let ua = user_agent.unwrap_or_default();
    parse_lowered(platform_header, &ua.to_ascii_lowercase())
}

/// Parse device info from a pre-lowercased User-Agent string.
pub fn parse_lowered(platform_header: Option<&str>, ua_lower: &str) -> DeviceInfo {
    let device_type = platform_header
        .map(classify_platform)
        .unwrap_or_else(|| classify_device_from_ua(ua_lower));

    DeviceInfo {
        device_type,
        browser: detect_browser(ua_lower),
        os: detect_os(ua_lower),
    }
}

/// Map the x-forge-platform header value to a device_type.
fn classify_platform(platform: &str) -> Option<String> {
    let p = platform.to_ascii_lowercase();
    let device_type = if p.starts_with("desktop") {
        "desktop"
    } else if p == "web" || p == "wasm" {
        "web"
    } else if p == "mobile"
        || p == "ios"
        || p.starts_with("ios-")
        || p == "android"
        || p.starts_with("android-")
    {
        "mobile"
    } else {
        return Some(p);
    };
    Some(device_type.to_string())
}

/// Fallback: infer device type from User-Agent when no platform header is present.
fn classify_device_from_ua(ua: &str) -> Option<String> {
    if ua.is_empty() {
        return None;
    }
    if ua.contains("ipad") || ua.contains("tablet") {
        Some("tablet".to_string())
    } else if ua.contains("mobile")
        || ua.contains("iphone")
        || (ua.contains("android") && !ua.contains("tablet"))
    {
        Some("mobile".to_string())
    } else if ua.contains("mozilla/") || ua.contains("chrome/") || ua.contains("safari/") {
        Some("web".to_string())
    } else {
        None
    }
}

fn detect_browser(ua: &str) -> Option<String> {
    if ua.is_empty() {
        return None;
    }
    // Order matters: check specific browsers before generic ones
    if ua.contains("edg/") || ua.contains("edge/") {
        Some("Edge".to_string())
    } else if ua.contains("opr/") || ua.contains("opera") {
        Some("Opera".to_string())
    } else if ua.contains("firefox/") {
        Some("Firefox".to_string())
    } else if ua.contains("chrome/") && !ua.contains("chromium/") {
        Some("Chrome".to_string())
    } else if ua.contains("chromium/") {
        Some("Chromium".to_string())
    } else if ua.contains("safari/") && ua.contains("version/") {
        Some("Safari".to_string())
    } else if ua.contains("dioxus") || ua.contains("forge-dioxus") {
        Some("Dioxus".to_string())
    } else {
        None
    }
}

fn detect_os(ua: &str) -> Option<String> {
    if ua.is_empty() {
        return None;
    }
    if ua.contains("iphone") || ua.contains("ipad") || ua.contains("ios") {
        Some("iOS".to_string())
    } else if ua.contains("android") {
        Some("Android".to_string())
    } else if ua.contains("mac os x") || ua.contains("macos") || ua.contains("macintosh") {
        Some("macOS".to_string())
    } else if ua.contains("windows") {
        Some("Windows".to_string())
    } else if ua.contains("linux") && !ua.contains("android") {
        Some("Linux".to_string())
    } else if ua.contains("cros") {
        Some("ChromeOS".to_string())
    } else {
        None
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn parses_chrome_on_macos() {
        let info = parse(
            None,
            Some(
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
            ),
        );
        assert_eq!(info.browser.as_deref(), Some("Chrome"));
        assert_eq!(info.os.as_deref(), Some("macOS"));
        assert_eq!(info.device_type.as_deref(), Some("web"));
    }

    #[tokio::test]
    async fn parses_mobile_safari() {
        let info = parse(
            None,
            Some(
                "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1",
            ),
        );
        assert_eq!(info.browser.as_deref(), Some("Safari"));
        assert_eq!(info.os.as_deref(), Some("iOS"));
        assert_eq!(info.device_type.as_deref(), Some("mobile"));
    }

    #[tokio::test]
    async fn platform_header_overrides_ua() {
        let info = parse(Some("desktop-macos"), Some("reqwest/0.12"));
        assert_eq!(info.device_type.as_deref(), Some("desktop"));
    }

    #[tokio::test]
    async fn detects_firefox_on_linux() {
        let info = parse(
            None,
            Some("Mozilla/5.0 (X11; Linux x86_64; rv:121.0) Gecko/20100101 Firefox/121.0"),
        );
        assert_eq!(info.browser.as_deref(), Some("Firefox"));
        assert_eq!(info.os.as_deref(), Some("Linux"));
    }

    #[tokio::test]
    async fn handles_empty_ua() {
        let info = parse(None, None);
        assert!(info.browser.is_none());
        assert!(info.os.is_none());
        assert!(info.device_type.is_none());
    }

    #[tokio::test]
    async fn ios_platform_header() {
        let info = parse(Some("ios"), None);
        assert_eq!(info.device_type.as_deref(), Some("mobile"));
    }

    #[tokio::test]
    async fn detects_edge_browser() {
        let info = parse(
            None,
            Some(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36 Edg/120.0.2210.91",
            ),
        );
        assert_eq!(info.browser.as_deref(), Some("Edge"));
    }

    #[tokio::test]
    async fn detects_opera() {
        let info = parse(
            None,
            Some(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36 OPR/106.0.0.0",
            ),
        );
        assert_eq!(info.browser.as_deref(), Some("Opera"));
    }

    #[tokio::test]
    async fn detects_chromium_vs_chrome() {
        let info = parse(
            None,
            Some(
                "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chromium/120.0.6099.129 Safari/537.36",
            ),
        );
        assert_eq!(info.browser.as_deref(), Some("Chromium"));
    }

    #[tokio::test]
    async fn detects_ipad_as_tablet() {
        let info = parse(
            None,
            Some(
                "Mozilla/5.0 (iPad; CPU OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1",
            ),
        );
        assert_eq!(info.device_type.as_deref(), Some("tablet"));
        assert_eq!(info.os.as_deref(), Some("iOS"));
    }

    #[tokio::test]
    async fn detects_android_phone() {
        let info = parse(
            None,
            Some(
                "Mozilla/5.0 (Linux; Android 14; Pixel 8 Pro) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.6099.144 Mobile Safari/537.36",
            ),
        );
        assert_eq!(info.device_type.as_deref(), Some("mobile"));
        assert_eq!(info.os.as_deref(), Some("Android"));
    }

    #[tokio::test]
    async fn detects_windows_chrome() {
        let info = parse(
            None,
            Some(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
            ),
        );
        assert_eq!(info.browser.as_deref(), Some("Chrome"));
        assert_eq!(info.os.as_deref(), Some("Windows"));
        assert_eq!(info.device_type.as_deref(), Some("web"));
    }

    #[tokio::test]
    async fn desktop_windows_platform_header() {
        let info = parse(Some("desktop-windows"), Some("reqwest/0.12"));
        assert_eq!(info.device_type.as_deref(), Some("desktop"));
    }

    #[tokio::test]
    async fn desktop_linux_platform_header() {
        let info = parse(Some("desktop-linux"), Some("reqwest/0.12"));
        assert_eq!(info.device_type.as_deref(), Some("desktop"));
    }

    #[tokio::test]
    async fn android_platform_header() {
        let info = parse(Some("android"), None);
        assert_eq!(info.device_type.as_deref(), Some("mobile"));
    }

    #[tokio::test]
    async fn wasm_platform_header() {
        let info = parse(Some("wasm"), None);
        assert_eq!(info.device_type.as_deref(), Some("web"));
    }

    #[tokio::test]
    async fn unknown_platform_passes_through() {
        let info = parse(Some("custom-device"), None);
        assert_eq!(info.device_type.as_deref(), Some("custom-device"));
    }

    #[tokio::test]
    async fn safari_needs_version() {
        // Safari/ present but no Version/ -- should NOT detect as Safari
        let info = parse(
            None,
            Some(
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
            ),
        );
        assert_ne!(info.browser.as_deref(), Some("Safari"));
    }

    #[tokio::test]
    async fn detects_chromeos() {
        let info = parse(
            None,
            Some(
                "Mozilla/5.0 (X11; CrOS x86_64 14541.0.0) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
            ),
        );
        assert_eq!(info.os.as_deref(), Some("ChromeOS"));
    }
}
