use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{ItemFn, parse_macro_input};

use crate::utils::{parse_duration_tokens, to_pascal_case};

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

    if let Some(le_start) = attr_str.find("leader_elected")
        && let Some(eq_pos) = attr_str[le_start..].find('=')
    {
        let after_eq = &attr_str[le_start + eq_pos + 1..];
        let value = after_eq.split(&[',', ')']).next().unwrap_or("").trim();
        result.leader_elected = Some(value == "true");
    }

    if let Some(rop_start) = attr_str.find("restart_on_panic")
        && let Some(eq_pos) = attr_str[rop_start..].find('=')
    {
        let after_eq = &attr_str[rop_start + eq_pos + 1..];
        let value = after_eq.split(&[',', ')']).next().unwrap_or("").trim();
        result.restart_on_panic = Some(value == "true");
    }

    if let Some(rd_start) = attr_str.find("restart_delay")
        && let Some(eq_pos) = attr_str[rd_start..].find('=')
    {
        let after_eq = &attr_str[rd_start + eq_pos + 1..];
        if let Some(quote_start) = after_eq.find('"')
            && let Some(quote_end) = after_eq[quote_start + 1..].find('"')
        {
            result.restart_delay = Some(after_eq[quote_start + 1..][..quote_end].to_string());
        }
    }

    if let Some(sd_start) = attr_str.find("startup_delay")
        && let Some(eq_pos) = attr_str[sd_start..].find('=')
    {
        let after_eq = &attr_str[sd_start + eq_pos + 1..];
        if let Some(quote_start) = after_eq.find('"')
            && let Some(quote_end) = after_eq[quote_start + 1..].find('"')
        {
            result.startup_delay = Some(after_eq[quote_start + 1..][..quote_end].to_string());
        }
    }

    if let Some(mr_start) = attr_str.find("max_restarts")
        && let Some(eq_pos) = attr_str[mr_start..].find('=')
    {
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

    result
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
        parse_duration_tokens(d, 5)
    } else {
        quote! { std::time::Duration::from_secs(5) }
    };

    let startup_delay = if let Some(ref d) = attrs.startup_delay {
        parse_duration_tokens(d, 0)
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

        forge::inventory::submit!(forge::AutoDaemon(|registry| {
            registry.register::<#struct_name>();
        }));
    };

    TokenStream::from(expanded)
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
        let ts = parse_duration_tokens("5s", 5);
        assert!(!ts.is_empty());
    }

    #[test]
    fn test_parse_duration_minutes() {
        let ts = parse_duration_tokens("10m", 600);
        assert!(!ts.is_empty());
    }

    #[test]
    fn test_parse_duration_milliseconds() {
        let ts = parse_duration_tokens("500ms", 500);
        assert!(!ts.is_empty());
    }
}
