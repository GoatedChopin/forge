use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::visit::Visit;
use syn::{FnArg, ItemFn, Pat, ReturnType, Type, parse_macro_input};

use crate::sql_extractor::{SqlStringExtractor, extract_tables_from_sql};

/// Expand the #[forge::query] attribute.
///
/// This transforms an async function into a query handler that:
/// - Takes a QueryContext as the first parameter
/// - Returns a Result<T>
/// - Generates a struct implementing ForgeQuery trait
pub fn expand_query(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    let attrs = parse_query_attrs(attr);

    expand_query_impl(input, attrs)
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

#[derive(Default)]
struct QueryAttrs {
    cache_ttl: Option<u64>,
    requires_auth: bool,
    required_role: Option<String>,
    is_public: bool,
    timeout: Option<u64>,
    rate_limit_requests: Option<u32>,
    rate_limit_per_secs: Option<u64>,
    rate_limit_key: Option<String>,
    log_level: Option<String>,
    /// Explicitly specified table dependencies (override for dynamic SQL).
    tables: Option<Vec<String>>,
}

fn parse_query_attrs(attr: TokenStream) -> QueryAttrs {
    let mut attrs = QueryAttrs::default();

    // Parse attribute arguments like #[forge::query(cache = "5m", public)]
    let attr_str = attr.to_string();

    if attr_str.contains("public") {
        attrs.is_public = true;
    }

    if attr_str.contains("require_auth") {
        attrs.requires_auth = true;
    }

    // Parse cache TTL (simple parsing)
    if let Some(cache_start) = attr_str.find("cache") {
        if let Some(quote_start) = attr_str[cache_start..].find('"') {
            let remaining = &attr_str[cache_start + quote_start + 1..];
            if let Some(quote_end) = remaining.find('"') {
                let ttl_str = &remaining[..quote_end];
                attrs.cache_ttl = parse_duration(ttl_str);
            }
        }
    }

    // Parse timeout
    if let Some(timeout_start) = attr_str.find("timeout") {
        if let Some(eq_pos) = attr_str[timeout_start..].find('=') {
            let remaining = &attr_str[timeout_start + eq_pos + 1..];
            let trimmed = remaining.trim();
            if let Ok(secs) = trimmed
                .split(&[',', ')'])
                .next()
                .unwrap_or("")
                .trim()
                .parse::<u64>()
            {
                attrs.timeout = Some(secs);
            }
        }
    }

    // Parse rate_limit(requests = N, per = "Xm", key = "user")
    if let Some(rl_start) = attr_str.find("rate_limit") {
        if let Some(paren_start) = attr_str[rl_start..].find('(') {
            let remaining = &attr_str[rl_start + paren_start + 1..];
            if let Some(paren_end) = remaining.find(')') {
                let rl_content = &remaining[..paren_end];

                // Parse requests = N
                if let Some(req_start) = rl_content.find("requests") {
                    if let Some(eq_pos) = rl_content[req_start..].find('=') {
                        let after_eq = &rl_content[req_start + eq_pos + 1..];
                        if let Ok(n) = after_eq
                            .split(',')
                            .next()
                            .unwrap_or("")
                            .trim()
                            .parse::<u32>()
                        {
                            attrs.rate_limit_requests = Some(n);
                        }
                    }
                }

                // Parse per = "Xm" or per = "Xs"
                if let Some(per_start) = rl_content.find("per") {
                    if let Some(quote_start) = rl_content[per_start..].find('"') {
                        let after_quote = &rl_content[per_start + quote_start + 1..];
                        if let Some(quote_end) = after_quote.find('"') {
                            let per_str = &after_quote[..quote_end];
                            attrs.rate_limit_per_secs = parse_duration(per_str);
                        }
                    }
                }

                // Parse key = "user" or key = "ip" etc
                if let Some(key_start) = rl_content.find("key") {
                    if let Some(quote_start) = rl_content[key_start..].find('"') {
                        let after_quote = &rl_content[key_start + quote_start + 1..];
                        if let Some(quote_end) = after_quote.find('"') {
                            let key = &after_quote[..quote_end];
                            attrs.rate_limit_key = Some(key.to_string());
                        }
                    }
                }
            }
        }
    }

    // Parse log = "level" (trace, debug, info, warn, error, off)
    if let Some(log_start) = attr_str.find("log") {
        // Make sure it's not "require_auth" or similar
        let before = if log_start > 0 {
            attr_str.chars().nth(log_start - 1)
        } else {
            None
        };
        if before.is_none() || !before.unwrap().is_alphanumeric() {
            if let Some(quote_start) = attr_str[log_start..].find('"') {
                let after_quote = &attr_str[log_start + quote_start + 1..];
                if let Some(quote_end) = after_quote.find('"') {
                    let level = &after_quote[..quote_end];
                    attrs.log_level = Some(level.to_string());
                }
            }
        }
    }

    // Parse tables = ["table1", "table2"] for explicit table dependencies
    if let Some(tables_start) = attr_str.find("tables") {
        if let Some(bracket_start) = attr_str[tables_start..].find('[') {
            let remaining = &attr_str[tables_start + bracket_start + 1..];
            if let Some(bracket_end) = remaining.find(']') {
                let tables_str = &remaining[..bracket_end];
                let tables: Vec<String> = tables_str
                    .split(',')
                    .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                if !tables.is_empty() {
                    attrs.tables = Some(tables);
                }
            }
        }
    }

    attrs
}

fn parse_duration(s: &str) -> Option<u64> {
    let s = s.trim();
    if let Some(num) = s.strip_suffix('s') {
        num.parse().ok()
    } else if let Some(num) = s.strip_suffix('m') {
        num.parse::<u64>().ok().map(|m| m * 60)
    } else if let Some(num) = s.strip_suffix('h') {
        num.parse::<u64>().ok().map(|h| h * 3600)
    } else {
        s.parse().ok()
    }
}

fn expand_query_impl(input: ItemFn, attrs: QueryAttrs) -> syn::Result<TokenStream2> {
    let fn_name = &input.sig.ident;
    let fn_name_str = fn_name.to_string();
    let struct_name = syn::Ident::new(
        &format!("{}Query", to_pascal_case(&fn_name_str)),
        fn_name.span(),
    );

    let vis = &input.vis;
    let asyncness = &input.sig.asyncness;
    let fn_block = &input.block;
    let fn_attrs = &input.attrs;

    // Validate async
    if asyncness.is_none() {
        return Err(syn::Error::new_spanned(
            &input.sig,
            "Query functions must be async",
        ));
    }

    // Extract parameters (skip first which should be &QueryContext)
    let params: Vec<_> = input.sig.inputs.iter().collect();
    if params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.sig,
            "Query functions must have at least a QueryContext parameter",
        ));
    }

    // Get context param - extract name and ensure it uses reference
    let (ctx_name, ctx_type) = match &params[0] {
        FnArg::Typed(pat_type) => {
            let name = if let Pat::Ident(pat_ident) = &*pat_type.pat {
                pat_ident.ident.clone()
            } else {
                return Err(syn::Error::new_spanned(
                    pat_type,
                    "Expected context parameter to be an identifier",
                ));
            };
            (name, &*pat_type.ty)
        }
        _ => {
            return Err(syn::Error::new_spanned(
                params[0],
                "Expected typed context parameter",
            ));
        }
    };

    // Determine the context type string
    let type_str = quote! { #ctx_type }.to_string();
    let is_ref = type_str.starts_with('&');

    // Extract table dependencies from function body or use explicit override
    let table_dependencies: Vec<String> = if let Some(explicit_tables) = attrs.tables {
        // Use explicitly specified tables
        explicit_tables
    } else {
        // Extract from SQL strings in the function body
        let mut extractor = SqlStringExtractor::new();
        extractor.visit_block(fn_block);

        let tables = extract_tables_from_sql(&extractor.sql_strings);
        let mut sorted: Vec<String> = tables.into_iter().collect();
        sorted.sort(); // Sort for deterministic output
        sorted
    };

    // Get remaining params for args struct
    let arg_params: Vec<_> = params.iter().skip(1).cloned().collect();

    // Build args struct fields
    let args_fields: Vec<TokenStream2> = arg_params
        .iter()
        .filter_map(|p| {
            if let FnArg::Typed(pat_type) = p {
                if let Pat::Ident(pat_ident) = &*pat_type.pat {
                    let name = &pat_ident.ident;
                    let ty = &pat_type.ty;
                    return Some(quote! { pub #name: #ty });
                }
            }
            None
        })
        .collect();

    // Build destructuring for function call
    let arg_names: Vec<TokenStream2> = arg_params
        .iter()
        .filter_map(|p| {
            if let FnArg::Typed(pat_type) = p {
                if let Pat::Ident(pat_ident) = &*pat_type.pat {
                    let name = &pat_ident.ident;
                    return Some(quote! { #name });
                }
            }
            None
        })
        .collect();

    // Get return type
    let output_type = match &input.sig.output {
        ReturnType::Default => quote! { () },
        ReturnType::Type(_, ty) => {
            // Extract T from Result<T> or Result<T, E>
            if let Type::Path(type_path) = &**ty {
                if let Some(segment) = type_path.path.segments.last() {
                    if segment.ident == "Result" {
                        if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                            if let Some(syn::GenericArgument::Type(t)) = args.args.first() {
                                quote! { #t }
                            } else {
                                quote! { #ty }
                            }
                        } else {
                            quote! { #ty }
                        }
                    } else {
                        quote! { #ty }
                    }
                } else {
                    quote! { #ty }
                }
            } else {
                quote! { #ty }
            }
        }
    };

    // Generate cache_ttl option
    let cache_ttl = match attrs.cache_ttl {
        Some(ttl) => quote! { Some(#ttl) },
        None => quote! { None },
    };

    // Generate timeout option
    let timeout = match attrs.timeout {
        Some(t) => quote! { Some(#t) },
        None => quote! { None },
    };

    let requires_auth = attrs.requires_auth;
    let is_public = attrs.is_public;

    let required_role = match &attrs.required_role {
        Some(role) => quote! { Some(#role) },
        None => quote! { None },
    };

    let rate_limit_requests = match attrs.rate_limit_requests {
        Some(n) => quote! { Some(#n) },
        None => quote! { None },
    };

    let rate_limit_per_secs = match attrs.rate_limit_per_secs {
        Some(n) => quote! { Some(#n) },
        None => quote! { None },
    };

    let rate_limit_key = match &attrs.rate_limit_key {
        Some(k) => quote! { Some(#k) },
        None => quote! { None },
    };

    let log_level = match &attrs.log_level {
        Some(l) => quote! { Some(#l) },
        None => quote! { None },
    };

    // Generate table_dependencies token
    let table_deps_tokens = if table_dependencies.is_empty() {
        quote! { &[] }
    } else {
        let table_strs: Vec<_> = table_dependencies.iter().map(|t| quote! { #t }).collect();
        quote! { &[#(#table_strs),*] }
    };

    // Check if we have a single custom args type (user-defined struct)
    // In this case, use it directly instead of wrapping it
    let single_custom_args_type: Option<&Type> = if arg_params.len() == 1 {
        if let FnArg::Typed(pat_type) = &arg_params[0] {
            // Check if it's a custom type (not a primitive)
            if let Type::Path(type_path) = &*pat_type.ty {
                if let Some(segment) = type_path.path.segments.last() {
                    // Use the user's type directly if it looks like a custom Args struct
                    let type_name = segment.ident.to_string();
                    if type_name.ends_with("Args") || type_name.contains("Args") {
                        Some(&*pat_type.ty)
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    // Generate the args struct (use unit type if no args, user type if single custom args)
    let (args_struct, args_type, execute_call) = if args_fields.is_empty() {
        (
            quote! {
                #vis struct #struct_name;
            },
            quote! { () },
            quote! { #fn_name(ctx).await },
        )
    } else if let Some(user_args_type) = single_custom_args_type {
        // Use the user's args type directly
        (
            quote! {
                #vis struct #struct_name;
            },
            quote! { #user_args_type },
            quote! { #fn_name(ctx, args).await },
        )
    } else {
        // Generate a wrapper struct for multiple args
        let args_struct_name = syn::Ident::new(&format!("{}Args", struct_name), fn_name.span());
        (
            quote! {
                #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
                #vis struct #args_struct_name {
                    #(#args_fields),*
                }

                #vis struct #struct_name;
            },
            quote! { #args_struct_name },
            quote! { #fn_name(ctx, #(args.#arg_names),*).await },
        )
    };

    // Generate the inner function - always take context by reference
    let inner_fn = if is_ref {
        // User already uses reference, keep the type as-is
        if arg_names.is_empty() {
            quote! {
                #(#fn_attrs)*
                #vis async fn #fn_name(#ctx_name: #ctx_type) -> forge::forge_core::Result<#output_type> #fn_block
            }
        } else {
            quote! {
                #(#fn_attrs)*
                #vis async fn #fn_name(#ctx_name: #ctx_type, #(#arg_params),*) -> forge::forge_core::Result<#output_type> #fn_block
            }
        }
    } else {
        // User uses value, convert to reference in the generated function
        if arg_names.is_empty() {
            quote! {
                #(#fn_attrs)*
                #vis async fn #fn_name(#ctx_name: &#ctx_type) -> forge::forge_core::Result<#output_type> #fn_block
            }
        } else {
            quote! {
                #(#fn_attrs)*
                #vis async fn #fn_name(#ctx_name: &#ctx_type, #(#arg_params),*) -> forge::forge_core::Result<#output_type> #fn_block
            }
        }
    };

    Ok(quote! {
        #args_struct

        #inner_fn

        impl forge::forge_core::ForgeQuery for #struct_name {
            type Args = #args_type;
            type Output = #output_type;

            fn info() -> forge::forge_core::FunctionInfo {
                forge::forge_core::FunctionInfo {
                    name: #fn_name_str,
                    description: None,
                    kind: forge::forge_core::FunctionKind::Query,
                    requires_auth: #requires_auth,
                    required_role: #required_role,
                    is_public: #is_public,
                    cache_ttl: #cache_ttl,
                    timeout: #timeout,
                    rate_limit_requests: #rate_limit_requests,
                    rate_limit_per_secs: #rate_limit_per_secs,
                    rate_limit_key: #rate_limit_key,
                    log_level: #log_level,
                    table_dependencies: #table_deps_tokens,
                }
            }

            fn execute(
                ctx: &forge::forge_core::QueryContext,
                args: Self::Args,
            ) -> std::pin::Pin<Box<dyn std::future::Future<Output = forge::forge_core::Result<Self::Output>> + Send + '_>> {
                Box::pin(async move {
                    #execute_call
                })
            }
        }
    })
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
        assert_eq!(to_pascal_case("get_user"), "GetUser");
        assert_eq!(to_pascal_case("list_all_projects"), "ListAllProjects");
        assert_eq!(to_pascal_case("simple"), "Simple");
    }

    #[test]
    fn test_parse_duration() {
        assert_eq!(parse_duration("30s"), Some(30));
        assert_eq!(parse_duration("5m"), Some(300));
        assert_eq!(parse_duration("1h"), Some(3600));
        assert_eq!(parse_duration("60"), Some(60));
    }
}
