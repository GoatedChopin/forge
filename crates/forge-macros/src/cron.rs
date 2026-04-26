use std::str::FromStr;

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{ItemFn, parse_macro_input};

use crate::utils::{
    has_attr_flag, parse_attr_value, parse_duration_tokens, to_pascal_case, validate_attr_keys,
};

const ALLOWED_CRON_KEYS: &[&str] = &[
    "name",
    "timezone",
    "group",
    "catch_up",
    "catch_up_limit",
    "timeout",
];

#[derive(Debug, Default)]
struct CronAttrs {
    /// Override the registry name (default: function name).
    name: Option<String>,
    schedule: Option<String>,
    timezone: Option<String>,
    group: Option<String>,
    catch_up: bool,
    catch_up_limit: Option<u32>,
    timeout: Option<String>,
}

fn parse_cron_attrs(attr: TokenStream) -> CronAttrs {
    let mut result = CronAttrs::default();
    let attr_str = attr.to_string();

    if let Some(name) = parse_attr_value(&attr_str, "name") {
        result.name = Some(name);
    }

    if let Some(quote_start) = attr_str.find('"') {
        let remaining = &attr_str[quote_start + 1..];
        if let Some(quote_end) = remaining.find('"') {
            result.schedule = Some(remaining[..quote_end].to_string());
        }
    }

    if let Some(tz_start) = attr_str.find("timezone")
        && let Some(eq_pos) = attr_str[tz_start..].find('=')
    {
        let after_eq = &attr_str[tz_start + eq_pos + 1..];
        if let Some(quote_start) = after_eq.find('"')
            && let Some(quote_end) = after_eq[quote_start + 1..].find('"')
        {
            result.timezone = Some(after_eq[quote_start + 1..][..quote_end].to_string());
        }
    }

    if let Some(grp_start) = attr_str.find("group")
        && let Some(eq_pos) = attr_str[grp_start..].find('=')
    {
        let after_eq = &attr_str[grp_start + eq_pos + 1..];
        if let Some(quote_start) = after_eq.find('"')
            && let Some(quote_end) = after_eq[quote_start + 1..].find('"')
        {
            result.group = Some(after_eq[quote_start + 1..][..quote_end].to_string());
        }
    }

    if let Some(timeout) = parse_attr_value(&attr_str, "timeout") {
        result.timeout = Some(timeout);
    }

    // Parse catch_up_limit = 5 first (so catch_up doesn't match it)
    if let Some(limit_start) = attr_str.find("catch_up_limit")
        && let Some(eq_pos) = attr_str[limit_start..].find('=')
    {
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

    if has_attr_flag(&attr_str, "catch_up") {
        result.catch_up = true;
    }

    result
}

pub fn cron_impl(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    let attr_str = attr.to_string();

    if let Err(e) = validate_attr_keys(&attr_str, ALLOWED_CRON_KEYS, "cron") {
        return e.to_compile_error().into();
    }

    let attrs = parse_cron_attrs(attr);

    let fn_name = &input.sig.ident;
    let fn_name_str = fn_name.to_string();
    let rpc_name = attrs.name.as_deref().unwrap_or(&fn_name_str).to_string();
    let module_name = format_ident!("__forge_handler_{}", fn_name);
    let struct_name = format_ident!("{}Cron", to_pascal_case(&fn_name.to_string()));

    let _vis = &input.vis;
    let block = &input.block;

    let schedule = attrs.schedule.unwrap_or_else(|| "* * * * *".to_string());

    // Validate cron expression at compile time to prevent runtime panics.
    // Normalize 5-part to 6-part (prepend seconds) to match what CronSchedule::new does.
    {
        let parts: Vec<&str> = schedule.split_whitespace().collect();
        let normalized = if parts.len() == 5 {
            format!("0 {schedule}")
        } else {
            schedule.clone()
        };
        if cron::Schedule::from_str(&normalized).is_err() {
            return syn::Error::new_spanned(
                &input.sig.ident,
                format!("Invalid cron schedule: \"{schedule}\""),
            )
            .to_compile_error()
            .into();
        }
    }
    let timezone = attrs.timezone.unwrap_or_else(|| "UTC".to_string());
    let group = attrs.group.unwrap_or_else(|| "default".to_string());
    let catch_up = attrs.catch_up;
    let catch_up_limit = attrs.catch_up_limit.unwrap_or(10);

    let timeout = if let Some(ref t) = attrs.timeout {
        parse_duration_tokens(t, 3600)
    } else {
        quote! { std::time::Duration::from_secs(3600) }
    };
    let http_timeout = if let Some(ref t) = attrs.timeout {
        let timeout = parse_duration_tokens(t, 0);
        quote! { Some(#timeout) }
    } else {
        quote! { None }
    };

    let other_attrs = &input.attrs;

    let expanded = quote! {
        #[doc(hidden)]
        #[allow(non_snake_case)]
        mod #module_name {
            use super::*;

            #(#other_attrs)*
            pub struct #struct_name;

            impl forge::forge_core::__sealed::Sealed for #struct_name {}

            impl forge::forge_core::cron::ForgeCron for #struct_name {
                fn info() -> forge::forge_core::cron::CronInfo {
                    forge::forge_core::cron::CronInfo {
                        name: #rpc_name,
                        schedule: forge::forge_core::cron::CronSchedule::new(#schedule)
                            .expect("Invalid cron schedule"),
                        timezone: #timezone,
                        group: #group,
                        catch_up: #catch_up,
                        catch_up_limit: #catch_up_limit,
                        timeout: #timeout,
                        http_timeout: #http_timeout,
                    }
                }

                fn execute(
                    ctx: &forge::forge_core::cron::CronContext,
                ) -> std::pin::Pin<Box<dyn std::future::Future<Output = forge::forge_core::Result<()>> + Send + '_>> {
                    Box::pin(async move #block)
                }
            }

            forge::inventory::submit!(forge::AutoCron(|registry| {
                registry.register::<#struct_name>();
            }));
        }
    };

    TokenStream::from(expanded)
}

// Tests for to_pascal_case and parse_duration are in utils.rs (single source of truth).
