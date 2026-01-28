use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{ItemFn, parse_macro_input};

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

    // Parse schedule from first quoted string argument
    if let Some(quote_start) = attr_str.find('"') {
        let remaining = &attr_str[quote_start + 1..];
        if let Some(quote_end) = remaining.find('"') {
            result.schedule = Some(remaining[..quote_end].to_string());
        }
    }

    // Parse timezone = "America/New_York"
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

    // Parse timeout = "30m"
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

    // Parse catch_up (boolean flag)
    if attr_str.contains("catch_up") {
        // Make sure it's not just catch_up_limit
        let catch_up_positions: Vec<_> = attr_str.match_indices("catch_up").collect();
        for (pos, _) in catch_up_positions {
            let after = &attr_str[pos + 8..];
            // If it's followed by '_limit', skip it
            if !after.starts_with("_limit") {
                result.catch_up = true;
                break;
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
        let n: u64 = s.trim_end_matches('s').parse().unwrap_or(30);
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
        let n: u64 = s.parse().unwrap_or(3600);
        quote! { std::time::Duration::from_secs(#n) }
    }
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
        parse_duration(t)
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
        assert_eq!(to_pascal_case("daily_cleanup"), "DailyCleanup");
        assert_eq!(to_pascal_case("hourly_report"), "HourlyReport");
        assert_eq!(to_pascal_case("simple"), "Simple");
    }

    #[test]
    fn test_parse_duration_hours() {
        let ts = parse_duration("2h");
        assert!(!ts.is_empty());
    }

    #[test]
    fn test_parse_duration_minutes() {
        let ts = parse_duration("30m");
        assert!(!ts.is_empty());
    }
}
