use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{ItemFn, parse_macro_input};

#[derive(Debug, Default)]
struct DaemonAttrs {
    leader_elected: Option<bool>,
    restart_on_panic: Option<bool>,
    restart_delay: Option<String>,
    startup_delay: Option<String>,
    max_restarts: Option<u32>,
}

fn parse_daemon_attrs(attr: TokenStream) -> DaemonAttrs {
    let mut result = DaemonAttrs::default();
    let attr_str = attr.to_string();

    // Parse leader_elected = true/false
    if let Some(le_start) = attr_str.find("leader_elected") {
        if let Some(eq_pos) = attr_str[le_start..].find('=') {
            let after_eq = &attr_str[le_start + eq_pos + 1..];
            let value = after_eq
                .split(&[',', ')'])
                .next()
                .unwrap_or("")
                .trim();
            result.leader_elected = Some(value == "true");
        }
    }

    // Parse restart_on_panic = true/false
    if let Some(rop_start) = attr_str.find("restart_on_panic") {
        if let Some(eq_pos) = attr_str[rop_start..].find('=') {
            let after_eq = &attr_str[rop_start + eq_pos + 1..];
            let value = after_eq
                .split(&[',', ')'])
                .next()
                .unwrap_or("")
                .trim();
            result.restart_on_panic = Some(value == "true");
        }
    }

    // Parse restart_delay = "5s"
    if let Some(rd_start) = attr_str.find("restart_delay") {
        if let Some(eq_pos) = attr_str[rd_start..].find('=') {
            let after_eq = &attr_str[rd_start + eq_pos + 1..];
            if let Some(quote_start) = after_eq.find('"') {
                let after_quote = &after_eq[quote_start + 1..];
                if let Some(quote_end) = after_quote.find('"') {
                    result.restart_delay = Some(after_quote[..quote_end].to_string());
                }
            }
        }
    }

    // Parse startup_delay = "10s"
    if let Some(sd_start) = attr_str.find("startup_delay") {
        if let Some(eq_pos) = attr_str[sd_start..].find('=') {
            let after_eq = &attr_str[sd_start + eq_pos + 1..];
            if let Some(quote_start) = after_eq.find('"') {
                let after_quote = &after_eq[quote_start + 1..];
                if let Some(quote_end) = after_quote.find('"') {
                    result.startup_delay = Some(after_quote[..quote_end].to_string());
                }
            }
        }
    }

    // Parse max_restarts = 10
    if let Some(mr_start) = attr_str.find("max_restarts") {
        if let Some(eq_pos) = attr_str[mr_start..].find('=') {
            let after_eq = &attr_str[mr_start + eq_pos + 1..];
            if let Ok(n) = after_eq
                .split(&[',', ')'])
                .next()
                .unwrap_or("")
                .trim()
                .parse::<u32>()
            {
                result.max_restarts = Some(n);
            }
        }
    }

    result
}

fn parse_duration(s: &str) -> proc_macro2::TokenStream {
    let s = s.trim();
    if s.ends_with("ms") {
        let n: u64 = s.trim_end_matches("ms").parse().unwrap_or(1000);
        quote! { std::time::Duration::from_millis(#n) }
    } else if s.ends_with('s') {
        let n: u64 = s.trim_end_matches('s').parse().unwrap_or(5);
        quote! { std::time::Duration::from_secs(#n) }
    } else if s.ends_with('m') {
        let n: u64 = s.trim_end_matches('m').parse().unwrap_or(5);
        let secs = n * 60;
        quote! { std::time::Duration::from_secs(#secs) }
    } else if s.ends_with('h') {
        let n: u64 = s.trim_end_matches('h').parse().unwrap_or(1);
        let secs = n * 3600;
        quote! { std::time::Duration::from_secs(#secs) }
    } else {
        let n: u64 = s.parse().unwrap_or(5);
        quote! { std::time::Duration::from_secs(#n) }
    }
}

pub fn daemon_impl(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    let attrs = parse_daemon_attrs(attr);

    let fn_name = &input.sig.ident;
    let fn_name_str = fn_name.to_string();
    let struct_name = format_ident!("{}Daemon", to_pascal_case(&fn_name.to_string()));

    let vis = &input.vis;
    let block = &input.block;

    let leader_elected = attrs.leader_elected.unwrap_or(true);
    let restart_on_panic = attrs.restart_on_panic.unwrap_or(true);

    let restart_delay = if let Some(ref d) = attrs.restart_delay {
        parse_duration(d)
    } else {
        quote! { std::time::Duration::from_secs(5) }
    };

    let startup_delay = if let Some(ref d) = attrs.startup_delay {
        parse_duration(d)
    } else {
        quote! { std::time::Duration::from_secs(0) }
    };

    let max_restarts = if let Some(n) = attrs.max_restarts {
        quote! { Some(#n) }
    } else {
        quote! { None }
    };

    let other_attrs = &input.attrs;

    let expanded = quote! {
        #(#other_attrs)*
        #vis struct #struct_name;

        impl forge::forge_core::daemon::ForgeDaemon for #struct_name {
            fn info() -> forge::forge_core::daemon::DaemonInfo {
                forge::forge_core::daemon::DaemonInfo {
                    name: #fn_name_str,
                    leader_elected: #leader_elected,
                    restart_on_panic: #restart_on_panic,
                    restart_delay: #restart_delay,
                    startup_delay: #startup_delay,
                    max_restarts: #max_restarts,
                }
            }

            fn execute(
                ctx: &forge::forge_core::daemon::DaemonContext,
            ) -> std::pin::Pin<Box<dyn std::future::Future<Output = forge::forge_core::Result<()>> + Send + '_>> {
                Box::pin(async move #block)
            }
        }
    };

    TokenStream::from(expanded)
}

fn to_pascal_case(s: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_pascal_case() {
        assert_eq!(to_pascal_case("heartbeat_daemon"), "HeartbeatDaemon");
        assert_eq!(to_pascal_case("data_sync"), "DataSync");
        assert_eq!(to_pascal_case("simple"), "Simple");
    }

    #[test]
    fn test_parse_duration_seconds() {
        let ts = parse_duration("5s");
        assert!(!ts.is_empty());
    }

    #[test]
    fn test_parse_duration_minutes() {
        let ts = parse_duration("10m");
        assert!(!ts.is_empty());
    }

    #[test]
    fn test_parse_duration_milliseconds() {
        let ts = parse_duration("500ms");
        assert!(!ts.is_empty());
    }
}
