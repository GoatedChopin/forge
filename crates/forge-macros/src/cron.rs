use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{ItemFn, parse_macro_input};

use crate::utils::{has_attr_flag, parse_duration_tokens, to_pascal_case};

#[derive(Debug, Default)]
struct CronAttrs {
    schedule: Option<String>,
    timezone: Option<String>,
    catch_up: bool,
    catch_up_limit: Option<u32>,
    timeout: Option<String>,
}

fn parse_cron_attrs(attr: TokenStream) -> CronAttrs {
    let mut result = CronAttrs::default();
    let attr_str = attr.to_string();

    if let Some(quote_start) = attr_str.find('"') {
        let remaining = &attr_str[quote_start + 1..];
        if let Some(quote_end) = remaining.find('"') {
            result.schedule = Some(remaining[..quote_end].to_string());
        }
    }

    if let Some(tz_start) = attr_str.find("timezone") {
        if let Some(eq_pos) = attr_str[tz_start..].find('=') {
            let after_eq = &attr_str[tz_start + eq_pos + 1..];
            if let Some(quote_start) = after_eq.find('"') {
                let after_quote = &after_eq[quote_start + 1..];
                if let Some(quote_end) = after_quote.find('"') {
                    result.timezone = Some(after_quote[..quote_end].to_string());
                }
            }
        }
    }

    if let Some(timeout_start) = attr_str.find("timeout") {
        if let Some(eq_pos) = attr_str[timeout_start..].find('=') {
            let after_eq = &attr_str[timeout_start + eq_pos + 1..];
            if let Some(quote_start) = after_eq.find('"') {
                let after_quote = &after_eq[quote_start + 1..];
                if let Some(quote_end) = after_quote.find('"') {
                    result.timeout = Some(after_quote[..quote_end].to_string());
                }
            }
        }
    }

    // Parse catch_up_limit = 5 first (so catch_up doesn't match it)
    if let Some(limit_start) = attr_str.find("catch_up_limit") {
        if let Some(eq_pos) = attr_str[limit_start..].find('=') {
            let after_eq = &attr_str[limit_start + eq_pos + 1..];
            if let Ok(n) = after_eq
                .split(&[',', ')'])
                .next()
                .unwrap_or("")
                .trim()
                .parse::<u32>()
            {
                result.catch_up_limit = Some(n);
            }
        }
    }

    if has_attr_flag(&attr_str, "catch_up") {
        result.catch_up = true;
    }

    result
}

pub fn cron_impl(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    let attrs = parse_cron_attrs(attr);

    let fn_name = &input.sig.ident;
    let fn_name_str = fn_name.to_string();
    let struct_name = format_ident!("{}Cron", to_pascal_case(&fn_name.to_string()));

    let vis = &input.vis;
    let block = &input.block;

    let schedule = attrs.schedule.unwrap_or_else(|| "* * * * *".to_string());
    let timezone = attrs.timezone.unwrap_or_else(|| "UTC".to_string());
    let catch_up = attrs.catch_up;
    let catch_up_limit = attrs.catch_up_limit.unwrap_or(10);

    let timeout = if let Some(ref t) = attrs.timeout {
        parse_duration_tokens(t, 3600)
    } else {
        quote! { std::time::Duration::from_secs(3600) }
    };

    let other_attrs = &input.attrs;

    let expanded = quote! {
        #(#other_attrs)*
        #vis struct #struct_name;

        impl forge::forge_core::cron::ForgeCron for #struct_name {
            fn info() -> forge::forge_core::cron::CronInfo {
                forge::forge_core::cron::CronInfo {
                    name: #fn_name_str,
                    schedule: forge::forge_core::cron::CronSchedule::new(#schedule)
                        .expect("Invalid cron schedule"),
                    timezone: #timezone,
                    catch_up: #catch_up,
                    catch_up_limit: #catch_up_limit,
                    timeout: #timeout,
                }
            }

            fn execute(
                ctx: &forge::forge_core::cron::CronContext,
            ) -> std::pin::Pin<Box<dyn std::future::Future<Output = forge::forge_core::Result<()>> + Send + '_>> {
                Box::pin(async move #block)
            }
        }
    };

    TokenStream::from(expanded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_pascal_case() {
        assert_eq!(to_pascal_case("daily_cleanup"), "DailyCleanup");
        assert_eq!(to_pascal_case("hourly_report"), "HourlyReport");
        assert_eq!(to_pascal_case("simple"), "Simple");
    }

    #[test]
    fn test_parse_duration_hours() {
        let ts = parse_duration_tokens("2h", 7200);
        assert!(!ts.is_empty());
    }

    #[test]
    fn test_parse_duration_minutes() {
        let ts = parse_duration_tokens("30m", 1800);
        assert!(!ts.is_empty());
    }
}
