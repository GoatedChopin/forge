use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::visit::Visit;
use syn::{FnArg, ItemFn, Pat, ReturnType, Type, parse_macro_input};

use crate::sql_extractor::{
    SqlStringExtractor, extract_columns_from_sql, extract_tables_from_sql,
    sql_references_identity_scope,
};
use crate::utils::{
    has_attr_flag, parse_attr_value, parse_duration_secs, parse_tables_attr, reject_reserved_keys,
    to_pascal_case, validate_attr_keys,
};

const ALLOWED_QUERY_KEYS: &[&str] = &[
    "name",
    "description",
    "public",
    "unscoped",
    "consistent",
    "require_role",
    "cache",
    "timeout",
    "rate_limit",
    "log",
    "tables",
    // Reserved for future Forge releases. Parsed here so apps fail loudly
    // (via `reject_reserved_keys` below) until behavior lands.
    "debounce_ms",
    "max_debounce_ms",
    "reexecute_timeout",
    "max_rows",
    "max_bytes",
];

/// Attribute keys whose names are reserved for upcoming reactor and
/// result-guardrail features. Using one today is a hard compile error
/// to surface that the feature isn't actually wired up yet.
const RESERVED_QUERY_KEYS: &[&str] = &[
    "debounce_ms",
    "max_debounce_ms",
    "reexecute_timeout",
    "max_rows",
    "max_bytes",
];

/// Expand the #[forge::query] attribute.
///
/// This transforms an async function into a query handler that:
/// - Takes a QueryContext as the first parameter
/// - Returns a Result<T>
/// - Generates a struct implementing ForgeQuery trait
pub fn expand_query(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    let attr_str = attr.to_string();

    if let Err(e) = reject_reserved_keys(&attr_str, RESERVED_QUERY_KEYS, "query") {
        return e.to_compile_error().into();
    }

    if let Err(e) = validate_attr_keys(&attr_str, ALLOWED_QUERY_KEYS, "query") {
        return e.to_compile_error().into();
    }

    let attrs = match parse_query_attrs(attr) {
        Ok(a) => a,
        Err(e) => return e.to_compile_error().into(),
    };

    expand_query_impl(input, attrs)
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

#[derive(Default)]
struct QueryAttrs {
    /// Override the wire name (default: function name).
    name: Option<String>,
    /// Human-readable description surfaced in metadata and docs.
    description: Option<String>,
    cache_ttl: Option<u64>,
    required_role: Option<String>,
    is_public: bool,
    is_unscoped: bool,
    consistent: bool,
    timeout: Option<u64>,
    rate_limit_requests: Option<u32>,
    rate_limit_per_secs: Option<u64>,
    rate_limit_key: Option<String>,
    log_level: Option<String>,
    /// Explicitly specified table dependencies (override for dynamic SQL).
    tables: Option<Vec<String>>,
}

fn parse_query_attrs(attr: TokenStream) -> Result<QueryAttrs, syn::Error> {
    let mut attrs = QueryAttrs::default();

    let attr_str = attr.to_string();

    if let Some(name) = parse_attr_value(&attr_str, "name") {
        attrs.name = Some(name);
    }

    if let Some(description) = parse_attr_value(&attr_str, "description") {
        attrs.description = Some(description);
    }

    if has_attr_flag(&attr_str, "public") {
        attrs.is_public = true;
    }

    if has_attr_flag(&attr_str, "consistent") {
        attrs.consistent = true;
    }

    if has_attr_flag(&attr_str, "unscoped") {
        attrs.is_unscoped = true;
    }

    if let Some(role_start) = attr_str.find("require_role")
        && let Some(paren_start) = attr_str[role_start..].find('(')
    {
        let remaining = &attr_str[role_start + paren_start + 1..];
        if let Some(paren_end) = remaining.find(')') {
            let role = remaining[..paren_end].trim().trim_matches('"');
            attrs.required_role = Some(role.to_string());
        }
    }

    if let Some(cache_start) = attr_str.find("cache")
        && let Some(quote_start) = attr_str[cache_start..].find('"')
    {
        let remaining = &attr_str[cache_start + quote_start + 1..];
        if let Some(quote_end) = remaining.find('"') {
            let ttl_str = &remaining[..quote_end];
            match parse_duration_secs(ttl_str) {
                Some(secs) => attrs.cache_ttl = Some(secs),
                None => {
                    return Err(syn::Error::new(
                        proc_macro2::Span::call_site(),
                        format!(
                            "invalid cache duration \"{ttl_str}\": use a duration string like \"30s\", \"5m\", or \"1h\""
                        ),
                    ));
                }
            }
        }
    }

    if let Some(timeout) = parse_attr_value(&attr_str, "timeout") {
        match parse_duration_secs(&timeout) {
            Some(secs) => attrs.timeout = Some(secs),
            None => {
                return Err(syn::Error::new(
                    proc_macro2::Span::call_site(),
                    format!(
                        "invalid timeout \"{timeout}\": use a duration string like \"30s\", \"5m\", or \"1h\""
                    ),
                ));
            }
        }
    }

    if let Some(rl_start) = attr_str.find("rate_limit")
        && let Some(paren_start) = attr_str[rl_start..].find('(')
    {
        let remaining = &attr_str[rl_start + paren_start + 1..];
        if let Some(paren_end) = remaining.find(')') {
            let rl_content = &remaining[..paren_end];

            if let Some(req_start) = rl_content.find("requests")
                && let Some(eq_pos) = rl_content[req_start..].find('=')
            {
                let after_eq = rl_content[req_start + eq_pos + 1..]
                    .split(',')
                    .next()
                    .unwrap_or("")
                    .trim();
                match after_eq.parse::<u32>() {
                    Ok(0) => {
                        return Err(syn::Error::new(
                            proc_macro2::Span::call_site(),
                            "rate_limit requests must be at least 1",
                        ));
                    }
                    Ok(n) => attrs.rate_limit_requests = Some(n),
                    Err(_) => {
                        return Err(syn::Error::new(
                            proc_macro2::Span::call_site(),
                            format!(
                                "invalid rate_limit requests value \"{after_eq}\": expected a positive integer"
                            ),
                        ));
                    }
                }
            }

            if let Some(per_start) = rl_content.find("per")
                && let Some(quote_start) = rl_content[per_start..].find('"')
            {
                let after_quote = &rl_content[per_start + quote_start + 1..];
                if let Some(quote_end) = after_quote.find('"') {
                    let per_str = &after_quote[..quote_end];
                    match parse_duration_secs(per_str) {
                        Some(secs) => attrs.rate_limit_per_secs = Some(secs),
                        None => {
                            return Err(syn::Error::new(
                                proc_macro2::Span::call_site(),
                                format!(
                                    "invalid rate_limit per duration \"{per_str}\": use a duration like \"1m\", \"30s\", or \"1h\""
                                ),
                            ));
                        }
                    }
                }
            }

            if let Some(key_start) = rl_content.find("key")
                && let Some(quote_start) = rl_content[key_start..].find('"')
            {
                let after_quote = &rl_content[key_start + quote_start + 1..];
                if let Some(quote_end) = after_quote.find('"') {
                    let key = &after_quote[..quote_end];
                    if !["user", "ip", "tenant", "global"].contains(&key)
                        && !key.starts_with("custom(")
                    {
                        return Err(syn::Error::new(
                            proc_macro2::Span::call_site(),
                            format!(
                                "invalid rate_limit key \"{key}\". Valid keys: \"user\", \"ip\", \"tenant\", \"global\", or \"custom(...)\""
                            ),
                        ));
                    }
                    attrs.rate_limit_key = Some(key.to_string());
                }
            }

            // Validate that required fields are present when rate_limit is used
            let has_any_rl = attrs.rate_limit_requests.is_some()
                || attrs.rate_limit_per_secs.is_some()
                || attrs.rate_limit_key.is_some();
            if has_any_rl && attrs.rate_limit_requests.is_none() {
                return Err(syn::Error::new(
                    proc_macro2::Span::call_site(),
                    "rate_limit requires `requests` field (e.g. rate_limit(requests = 100, per = \"1m\", key = \"user\"))",
                ));
            }
            if has_any_rl && attrs.rate_limit_per_secs.is_none() {
                return Err(syn::Error::new(
                    proc_macro2::Span::call_site(),
                    "rate_limit requires `per` field (e.g. rate_limit(requests = 100, per = \"1m\", key = \"user\"))",
                ));
            }
        }
    }

    if let Some(level) = parse_attr_value(&attr_str, "log") {
        attrs.log_level = Some(level);
    }

    if let Some(tables) = parse_tables_attr(&attr_str) {
        attrs.tables = Some(tables);
    }

    Ok(attrs)
}

fn expand_query_impl(input: ItemFn, attrs: QueryAttrs) -> syn::Result<TokenStream2> {
    let fn_name = &input.sig.ident;
    let fn_name_str = fn_name.to_string();
    let rpc_name = attrs.name.as_deref().unwrap_or(&fn_name_str).to_string();
    let module_name = syn::Ident::new(&format!("__forge_handler_{}", fn_name_str), fn_name.span());
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
        sorted.sort();
        sorted
    };

    // Extract selected columns from SQL
    let selected_columns: Vec<String> = {
        let mut extractor = SqlStringExtractor::new();
        extractor.visit_block(fn_block);
        let cols = extract_columns_from_sql(&extractor.sql_strings);
        let mut sorted: Vec<String> = cols.into_iter().collect();
        sorted.sort();
        sorted
    };

    // Compile-time scope check: private queries that touch tables must filter by user identity
    if !attrs.is_public && !attrs.is_unscoped && !table_dependencies.is_empty() {
        let mut scope_extractor = SqlStringExtractor::new();
        scope_extractor.visit_block(fn_block);
        if !sql_references_identity_scope(&scope_extractor.sql_strings) {
            let tables_str = table_dependencies.join(", ");
            return Err(syn::Error::new_spanned(
                &input.sig.ident,
                format!(
                    "Private query `{fn_name_str}` references table(s) [{tables_str}] but SQL \
                     does not filter by user_id or owner_id. Add a WHERE clause scoped to the \
                     authenticated user, or use #[query(unscoped)] if this is intentional."
                ),
            ));
        }
    }

    // Get remaining params for args struct
    let arg_params: Vec<_> = params.iter().skip(1).cloned().collect();

    // Build args struct fields
    let args_fields: Vec<TokenStream2> = arg_params
        .iter()
        .filter_map(|p| {
            if let FnArg::Typed(pat_type) = p
                && let Pat::Ident(pat_ident) = &*pat_type.pat
            {
                let name = &pat_ident.ident;
                let ty = &pat_type.ty;
                return Some(quote! { pub #name: #ty });
            }
            None
        })
        .collect();

    // Build destructuring for function call
    let arg_names: Vec<TokenStream2> = arg_params
        .iter()
        .filter_map(|p| {
            if let FnArg::Typed(pat_type) = p
                && let Pat::Ident(pat_ident) = &*pat_type.pat
            {
                let name = &pat_ident.ident;
                return Some(quote! { #name });
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

    // Generate timeout option as Duration so all handler Info structs agree.
    let timeout = match attrs.timeout {
        Some(t) => quote! { Some(::std::time::Duration::from_secs(#t)) },
        None => quote! { None },
    };
    let http_timeout = timeout.clone();

    let description = match &attrs.description {
        Some(d) => quote! { Some(#d) },
        None => quote! { None },
    };

    let is_public = attrs.is_public;
    let consistent = attrs.consistent;

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
        Some(k) => {
            let key_tokens = match k.as_str() {
                "user" => quote! { forge::forge_core::rate_limit::RateLimitKey::User },
                "ip" => quote! { forge::forge_core::rate_limit::RateLimitKey::Ip },
                "tenant" => quote! { forge::forge_core::rate_limit::RateLimitKey::Tenant },
                "user_action" => quote! { forge::forge_core::rate_limit::RateLimitKey::UserAction },
                "global" => quote! { forge::forge_core::rate_limit::RateLimitKey::Global },
                _ if k.starts_with("custom:") => {
                    let claim = k.trim_start_matches("custom:");
                    quote! { forge::forge_core::rate_limit::RateLimitKey::Custom(#claim.to_string()) }
                }
                _ => quote! { forge::forge_core::rate_limit::RateLimitKey::User },
            };
            quote! { Some(#key_tokens) }
        }
        None => quote! { None },
    };

    let log_level = match &attrs.log_level {
        Some(l) => {
            let level_tokens = match l.as_str() {
                "trace" => quote! { forge::forge_core::LogLevel::Trace },
                "debug" => quote! { forge::forge_core::LogLevel::Debug },
                "info" => quote! { forge::forge_core::LogLevel::Info },
                "warn" => quote! { forge::forge_core::LogLevel::Warn },
                "error" => quote! { forge::forge_core::LogLevel::Error },
                "off" => quote! { forge::forge_core::LogLevel::Off },
                _ => quote! { forge::forge_core::LogLevel::Trace },
            };
            quote! { Some(#level_tokens) }
        }
        None => quote! { None },
    };

    // Generate table_dependencies token
    let table_deps_tokens = if table_dependencies.is_empty() {
        quote! { &[] }
    } else {
        let table_strs: Vec<_> = table_dependencies.iter().map(|t| quote! { #t }).collect();
        quote! { &[#(#table_strs),*] }
    };

    // Generate selected_columns token
    let selected_cols_tokens = if selected_columns.is_empty() {
        quote! { &[] }
    } else {
        let col_strs: Vec<_> = selected_columns.iter().map(|c| quote! { #c }).collect();
        quote! { &[#(#col_strs),*] }
    };

    // A single non-primitive struct argument is passed through to the handler
    // as the args type directly. Primitives and collections get wrapped in a
    // generated #StructNameArgs struct so RPC payloads stay JSON-named.
    let single_custom_args_type: Option<&Type> = if arg_params.len() == 1 {
        if let FnArg::Typed(pat_type) = &arg_params[0] {
            if crate::utils::is_primitive_arg_type(&pat_type.ty) {
                None
            } else {
                Some(&*pat_type.ty)
            }
        } else {
            None
        }
    } else {
        None
    };

    // Generate handler struct definitions and execute call for the hidden module.
    // The struct and its args live in a private per-handler module; the original function
    // stays at the parent level and is called via super::.
    let (module_struct_defs, args_type, execute_call) = if args_fields.is_empty() {
        (
            quote! { pub struct #struct_name; },
            quote! { () },
            quote! { super::#fn_name(ctx).await },
        )
    } else if let Some(user_args_type) = single_custom_args_type {
        (
            quote! { pub struct #struct_name; },
            quote! { #user_args_type },
            quote! { super::#fn_name(ctx, args).await },
        )
    } else {
        let args_struct_name = syn::Ident::new(&format!("{}Args", struct_name), fn_name.span());
        (
            quote! {
                #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
                pub struct #args_struct_name {
                    #(#args_fields),*
                }
                pub struct #struct_name;
            },
            quote! { #args_struct_name },
            quote! { super::#fn_name(ctx, #(args.#arg_names),*).await },
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
        #inner_fn

        #[doc(hidden)]
        #[allow(non_snake_case)]
        mod #module_name {
            use super::*;

            #module_struct_defs

            impl forge::forge_core::__sealed::Sealed for #struct_name {}

            impl forge::forge_core::ForgeQuery for #struct_name {
                type Args = #args_type;
                type Output = #output_type;

                fn info() -> forge::forge_core::FunctionInfo {
                    forge::forge_core::FunctionInfo {
                        name: #rpc_name,
                        description: #description,
                        kind: forge::forge_core::FunctionKind::Query,
                        required_role: #required_role,
                        is_public: #is_public,
                        cache_ttl: #cache_ttl,
                        timeout: #timeout,
                        http_timeout: #http_timeout,
                        rate_limit_requests: #rate_limit_requests,
                        rate_limit_per_secs: #rate_limit_per_secs,
                        rate_limit_key: #rate_limit_key,
                        log_level: #log_level,
                        table_dependencies: #table_deps_tokens,
                        selected_columns: #selected_cols_tokens,
                        transactional: false,
                        consistent: #consistent,
                        max_upload_size_bytes: None,
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

            forge::inventory::submit!(forge::AutoQuery(|registry| {
                registry.register_query::<#struct_name>();
            }));
        }
    })
}

// Tests for to_pascal_case and parse_duration are in utils.rs (single source of truth).
