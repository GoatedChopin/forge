use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{ItemFn, parse_macro_input};

use crate::utils::{has_attr_flag, parse_duration_tokens, to_pascal_case};

#[derive(Debug, Default)]
struct WebhookAttrs {
    path: Option<String>,
    signature_algorithm: Option<String>,
    signature_header: Option<String>,
    signature_secret_env: Option<String>,
    allow_unsigned: bool,
    idempotency: Option<String>,
    timeout: Option<String>,
}

fn parse_webhook_attrs(attr: TokenStream) -> syn::Result<WebhookAttrs> {
    let mut result = WebhookAttrs::default();
    let attr_str = attr.to_string();

    if has_attr_flag(&attr_str, "allow_unsigned") {
        result.allow_unsigned = true;
    }

    if let Some(path_start) = attr_str.find("path")
        && let Some(eq_pos) = attr_str[path_start..].find('=')
    {
        let after_eq = &attr_str[path_start + eq_pos + 1..];
        if let Some(quote_start) = after_eq.find('"') {
            let after_quote = &after_eq[quote_start + 1..];
            if let Some(quote_end) = after_quote.find('"') {
                result.path = Some(after_quote[..quote_end].to_string());
            }
        }
    }

    if let Some(sig_start) = attr_str.find("signature") {
        let remaining = &attr_str[sig_start..];

        if remaining.contains("hmac_sha256") {
            result.signature_algorithm = Some("HmacSha256".to_string());
        } else if remaining.contains("hmac_sha1") {
            result.signature_algorithm = Some("HmacSha1".to_string());
        } else if remaining.contains("hmac_sha512") {
            result.signature_algorithm = Some("HmacSha512".to_string());
        }

        if let Some(paren_start) = remaining.find('(') {
            let inside_parens = &remaining[paren_start + 1..];

            let mut depth = 1;
            let mut end_pos = 0;
            for (i, c) in inside_parens.char_indices() {
                match c {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            end_pos = i;
                            break;
                        }
                    }
                    _ => {}
                }
            }

            let args_str = &inside_parens[..end_pos];

            let quotes: Vec<_> = args_str.match_indices('"').collect();
            if quotes.len() >= 4 {
                let header_start = quotes[0].0 + 1;
                let header_end = quotes[1].0;
                result.signature_header = Some(args_str[header_start..header_end].to_string());

                let secret_start = quotes[2].0 + 1;
                let secret_end = quotes[3].0;
                result.signature_secret_env = Some(args_str[secret_start..secret_end].to_string());
            }
        }
    }

    if let Some(idem_start) = attr_str.find("idempotency")
        && let Some(eq_pos) = attr_str[idem_start..].find('=')
    {
        let after_eq = &attr_str[idem_start + eq_pos + 1..];
        if let Some(quote_start) = after_eq.find('"') {
            let after_quote = &after_eq[quote_start + 1..];
            if let Some(quote_end) = after_quote.find('"') {
                result.idempotency = Some(after_quote[..quote_end].to_string());
            }
        }
    }

    if let Some(timeout_start) = attr_str.find("timeout")
        && let Some(eq_pos) = attr_str[timeout_start..].find('=')
    {
        let after_eq = &attr_str[timeout_start + eq_pos + 1..];
        if let Some(quote_start) = after_eq.find('"') {
            let after_quote = &after_eq[quote_start + 1..];
            if let Some(quote_end) = after_quote.find('"') {
                result.timeout = Some(after_quote[..quote_end].to_string());
            }
        }
    }

    if result.path.is_none() {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "webhook requires path attribute",
        ));
    }

    Ok(result)
}

pub fn webhook_impl(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    let attrs = match parse_webhook_attrs(attr) {
        Ok(attrs) => attrs,
        Err(e) => return e.to_compile_error().into(),
    };

    let fn_name = &input.sig.ident;
    let fn_name_str = fn_name.to_string();
    let struct_name = format_ident!("{}Webhook", to_pascal_case(&fn_name.to_string()));

    let vis = &input.vis;
    let block = &input.block;

    let path = attrs.path.unwrap_or_else(|| "/webhooks".to_string());
    let allow_unsigned = attrs.allow_unsigned;

    let timeout = if let Some(ref t) = attrs.timeout {
        parse_duration_tokens(t, 30)
    } else {
        quote! { std::time::Duration::from_secs(30) }
    };

    let signature = if let (Some(alg), Some(header), Some(secret_env)) = (
        &attrs.signature_algorithm,
        &attrs.signature_header,
        &attrs.signature_secret_env,
    ) {
        let alg_token = match alg.as_str() {
            "HmacSha256" => quote! { forge::forge_core::webhook::SignatureAlgorithm::HmacSha256 },
            "HmacSha1" => quote! { forge::forge_core::webhook::SignatureAlgorithm::HmacSha1 },
            "HmacSha512" => quote! { forge::forge_core::webhook::SignatureAlgorithm::HmacSha512 },
            _ => quote! { forge::forge_core::webhook::SignatureAlgorithm::HmacSha256 },
        };
        quote! {
            Some(forge::forge_core::webhook::SignatureConfig {
                algorithm: #alg_token,
                header_name: #header,
                secret_env: #secret_env,
            })
        }
    } else {
        quote! { None }
    };

    let idempotency = if let Some(ref idem) = attrs.idempotency {
        if let Some((prefix, value)) = idem.split_once(':') {
            match prefix {
                "header" => {
                    quote! {
                        Some(forge::forge_core::webhook::IdempotencyConfig::new(
                            forge::forge_core::webhook::IdempotencySource::Header(#value)
                        ))
                    }
                }
                "body" => {
                    quote! {
                        Some(forge::forge_core::webhook::IdempotencyConfig::new(
                            forge::forge_core::webhook::IdempotencySource::Body(#value)
                        ))
                    }
                }
                _ => quote! { None },
            }
        } else {
            quote! { None }
        }
    } else {
        quote! { None }
    };

    let other_attrs = &input.attrs;

    let expanded = quote! {
        #(#other_attrs)*
        #vis struct #struct_name;

        impl forge::forge_core::webhook::ForgeWebhook for #struct_name {
            fn info() -> forge::forge_core::webhook::WebhookInfo {
                forge::forge_core::webhook::WebhookInfo {
                    name: #fn_name_str,
                    path: #path,
                    signature: #signature,
                    allow_unsigned: #allow_unsigned,
                    idempotency: #idempotency,
                    timeout: #timeout,
                }
            }

            fn execute(
                ctx: &forge::forge_core::webhook::WebhookContext,
                payload: serde_json::Value,
            ) -> std::pin::Pin<Box<dyn std::future::Future<Output = forge::forge_core::Result<forge::forge_core::webhook::WebhookResult>> + Send + '_>> {
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
        assert_eq!(to_pascal_case("github_webhook"), "GithubWebhook");
        assert_eq!(to_pascal_case("stripe_events"), "StripeEvents");
        assert_eq!(to_pascal_case("simple"), "Simple");
    }

    #[test]
    fn test_parse_duration_seconds() {
        let ts = parse_duration_tokens("30s", 30);
        assert!(!ts.is_empty());
    }

    #[test]
    fn test_parse_duration_minutes() {
        let ts = parse_duration_tokens("5m", 300);
        assert!(!ts.is_empty());
    }
}
