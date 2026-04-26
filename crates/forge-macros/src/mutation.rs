use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{FnArg, ItemFn, Pat, ReturnType, Type, parse_macro_input};

use crate::utils::{
    has_attr_flag, parse_attr_value, parse_duration_secs, parse_size_bytes, to_pascal_case,
    validate_attr_keys,
};

const ALLOWED_MUTATION_KEYS: &[&str] = &[
    "name",
    "transactional",
    "public",
    "unscoped",
    "require_role",
    "timeout",
    "rate_limit",
    "log",
    "max_size",
];

/// Expand the #[forge::mutation] attribute.
///
/// This transforms an async function into a mutation handler that:
/// - Takes a MutationContext as the first parameter
/// - Returns a Result<T>
/// - Runs within a database transaction
/// - Generates a struct implementing ForgeMutation trait
pub fn expand_mutation(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    let attr_str = attr.to_string();

    if let Err(e) = validate_attr_keys(&attr_str, ALLOWED_MUTATION_KEYS, "mutation") {
        return e.to_compile_error().into();
    }

    let attrs = match parse_mutation_attrs(attr) {
        Ok(a) => a,
        Err(e) => return e.to_compile_error().into(),
    };

    expand_mutation_impl(input, attrs)
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

struct MutationAttrs {
    /// Override the wire name (default: function name).
    name: Option<String>,
    required_role: Option<String>,
    is_public: bool,
    is_unscoped: bool,
    timeout: Option<u64>,
    rate_limit_requests: Option<u32>,
    rate_limit_per_secs: Option<u64>,
    rate_limit_key: Option<String>,
    log_level: Option<String>,
    /// Defaults to `true`. Opt out with `transactional = false` for
    /// high-throughput mutations that don't need atomicity.
    transactional: bool,
    max_upload_size_bytes: Option<usize>,
}

impl Default for MutationAttrs {
    fn default() -> Self {
        Self {
            name: None,
            required_role: None,
            is_public: false,
            is_unscoped: false,
            timeout: None,
            rate_limit_requests: None,
            rate_limit_per_secs: None,
            rate_limit_key: None,
            log_level: None,
            transactional: true,
            max_upload_size_bytes: None,
        }
    }
}

fn parse_mutation_attrs(attr: TokenStream) -> Result<MutationAttrs, syn::Error> {
    let mut attrs = MutationAttrs::default();

    let attr_str = attr.to_string();

    if let Some(name) = parse_attr_value(&attr_str, "name") {
        attrs.name = Some(name);
    }

    // `transactional = false` opts out; bare `transactional` flag is a no-op
    // (the default is already true, but we accept it for clarity).
    if let Some(val) = parse_attr_value(&attr_str, "transactional")
        && val == "false"
    {
        attrs.transactional = false;
    }

    if has_attr_flag(&attr_str, "public") {
        attrs.is_public = true;
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

            if let Some(per_start) = rl_content.find("per")
                && let Some(quote_start) = rl_content[per_start..].find('"')
            {
                let after_quote = &rl_content[per_start + quote_start + 1..];
                if let Some(quote_end) = after_quote.find('"') {
                    let per_str = &after_quote[..quote_end];
                    attrs.rate_limit_per_secs = parse_duration_secs(per_str);
                }
            }

            if let Some(key_start) = rl_content.find("key")
                && let Some(quote_start) = rl_content[key_start..].find('"')
            {
                let after_quote = &rl_content[key_start + quote_start + 1..];
                if let Some(quote_end) = after_quote.find('"') {
                    let key = &after_quote[..quote_end];
                    attrs.rate_limit_key = Some(key.to_string());
                }
            }
        }
    }

    if let Some(log_start) = attr_str.find("log") {
        // Make sure it's not part of another word
        let before = if log_start > 0 {
            attr_str.chars().nth(log_start - 1)
        } else {
            None
        };
        if (before.is_none() || !before.unwrap().is_alphanumeric())
            && let Some(quote_start) = attr_str[log_start..].find('"')
        {
            let after_quote = &attr_str[log_start + quote_start + 1..];
            if let Some(quote_end) = after_quote.find('"') {
                let level = &after_quote[..quote_end];
                attrs.log_level = Some(level.to_string());
            }
        }
    }

    if let Some(size_str) = parse_attr_value(&attr_str, "max_size") {
        attrs.max_upload_size_bytes = parse_size_bytes(&size_str);
    }

    Ok(attrs)
}

fn expand_mutation_impl(input: ItemFn, attrs: MutationAttrs) -> syn::Result<TokenStream2> {
    let fn_name = &input.sig.ident;
    let fn_name_str = fn_name.to_string();
    let rpc_name = attrs.name.as_deref().unwrap_or(&fn_name_str).to_string();
    let module_name = syn::Ident::new(&format!("__forge_handler_{}", fn_name_str), fn_name.span());
    let struct_name = syn::Ident::new(
        &format!("{}Mutation", to_pascal_case(&fn_name_str)),
        fn_name.span(),
    );

    let vis = &input.vis;
    let asyncness = &input.sig.asyncness;
    let fn_block = &input.block;
    let fn_attrs = &input.attrs;

    // dispatch_job / start_workflow require a transaction so the outbox flush is
    // atomic with the database write. Explicitly opting out of transactions with
    // `transactional = false` while calling these is always a bug.
    let block_str = quote! { #fn_block }.to_string();
    let has_dispatch = block_str.contains("dispatch_job") || block_str.contains("start_workflow");
    if has_dispatch && !attrs.transactional {
        return Err(syn::Error::new_spanned(
            &input.sig.ident,
            "Mutations that call `dispatch_job()` or `start_workflow()` cannot use \
             `transactional = false` — jobs dispatched outside a transaction may \
             execute even when the database write is rolled back on error.",
        ));
    }

    // Validate async
    if asyncness.is_none() {
        return Err(syn::Error::new_spanned(
            &input.sig,
            "Mutation functions must be async",
        ));
    }

    // Extract parameters (skip first which should be &MutationContext)
    let params: Vec<_> = input.sig.inputs.iter().collect();
    if params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.sig,
            "Mutation functions must have at least a MutationContext parameter",
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

    // Determine the context type string (e.g., MutationContext)
    let type_str = quote! { #ctx_type }.to_string();
    let is_ref = type_str.starts_with('&');

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

    // Generate timeout option
    let timeout = match attrs.timeout {
        Some(t) => quote! { Some(#t) },
        None => quote! { None },
    };
    let http_timeout = timeout.clone();

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

    let max_upload_size_bytes = match attrs.max_upload_size_bytes {
        Some(n) => quote! { Some(#n) },
        None => quote! { None },
    };

    let transactional = attrs.transactional;
    let is_public = attrs.is_public;

    // Check if we have a single custom args type (user-defined struct)
    // In this case, use it directly instead of wrapping it
    let single_custom_args_type: Option<&Type> = if arg_params.len() == 1 {
        if let FnArg::Typed(pat_type) = &arg_params[0] {
            // Check if it's a custom type (not a primitive)
            if let Type::Path(type_path) = &*pat_type.ty {
                if let Some(segment) = type_path.path.segments.last() {
                    // Use the user's type directly if it looks like a custom Args/Input struct
                    let type_name = segment.ident.to_string();
                    if type_name.ends_with("Args")
                        || type_name.contains("Args")
                        || type_name.ends_with("Input")
                        || type_name.contains("Input")
                    {
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

            impl forge::forge_core::ForgeMutation for #struct_name {
                type Args = #args_type;
                type Output = #output_type;

                fn info() -> forge::forge_core::FunctionInfo {
                    forge::forge_core::FunctionInfo {
                        name: #rpc_name,
                        description: None,
                        kind: forge::forge_core::FunctionKind::Mutation,
                        required_role: #required_role,
                        is_public: #is_public,
                        cache_ttl: None,
                        timeout: #timeout,
                        http_timeout: #http_timeout,
                        rate_limit_requests: #rate_limit_requests,
                        rate_limit_per_secs: #rate_limit_per_secs,
                        rate_limit_key: #rate_limit_key,
                        log_level: #log_level,
                        table_dependencies: &[],
                        selected_columns: &[],
                        transactional: #transactional,
                        consistent: false,
                        max_upload_size_bytes: #max_upload_size_bytes,
                    }
                }

                fn execute(
                    ctx: &forge::forge_core::MutationContext,
                    args: Self::Args,
                ) -> std::pin::Pin<Box<dyn std::future::Future<Output = forge::forge_core::Result<Self::Output>> + Send + '_>> {
                    Box::pin(async move {
                        #execute_call
                    })
                }
            }

            forge::inventory::submit!(forge::AutoMutation(|registry| {
                registry.register_mutation::<#struct_name>();
            }));
        }
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]
mod tests {
    use super::*;

    // Note: proc_macro::TokenStream cannot be created outside the compiler bridge,
    // so we test parse_mutation_attrs indirectly through the has_attr_flag/parse_attr_value
    // utilities and test expand_mutation_impl directly with syn::ItemFn + MutationAttrs.

    // --- MutationAttrs default ---

    #[test]
    fn default_attrs_transactional_is_true() {
        let attrs = MutationAttrs::default();
        assert!(attrs.transactional, "transactional defaults to true");
        assert!(!attrs.is_public);
        assert!(!attrs.is_unscoped);
        assert!(attrs.required_role.is_none());
        assert!(attrs.timeout.is_none());
        assert!(attrs.rate_limit_requests.is_none());
        assert!(attrs.rate_limit_per_secs.is_none());
        assert!(attrs.rate_limit_key.is_none());
        assert!(attrs.log_level.is_none());
        assert!(attrs.max_upload_size_bytes.is_none());
    }

    // --- Validation: transactional requirement ---

    #[test]
    fn rejects_dispatch_job_with_transactional_false() {
        let input: ItemFn = syn::parse_str(
            r#"
            pub async fn create_user(ctx: &MutationContext, name: String) -> Result<User> {
                ctx.dispatch_job("send_email", json!({})).await?;
                Ok(User { name })
            }
            "#,
        )
        .unwrap();

        let attrs = MutationAttrs {
            transactional: false,
            ..MutationAttrs::default()
        };
        let result = expand_mutation_impl(input, attrs);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("transactional"),
            "Error should mention transactional: {err_msg}"
        );
    }

    #[test]
    fn rejects_start_workflow_with_transactional_false() {
        let input: ItemFn = syn::parse_str(
            r#"
            pub async fn begin_onboarding(ctx: &MutationContext) -> Result<()> {
                ctx.start_workflow("onboarding", json!({})).await?;
                Ok(())
            }
            "#,
        )
        .unwrap();

        let attrs = MutationAttrs {
            transactional: false,
            ..MutationAttrs::default()
        };
        let result = expand_mutation_impl(input, attrs);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("transactional"));
    }

    #[test]
    fn accepts_dispatch_job_with_default_transactional() {
        let input: ItemFn = syn::parse_str(
            r#"
            pub async fn create_user(ctx: &MutationContext, name: String) -> Result<User> {
                ctx.dispatch_job("send_email", json!({})).await?;
                Ok(User { name })
            }
            "#,
        )
        .unwrap();

        // Default is transactional = true, so dispatch_job is fine.
        let attrs = MutationAttrs::default();
        let result = expand_mutation_impl(input, attrs);
        assert!(
            result.is_ok(),
            "Should accept dispatch_job with default transactional=true"
        );
    }

    #[test]
    fn accepts_dispatch_job_with_transactional() {
        let input: ItemFn = syn::parse_str(
            r#"
            pub async fn create_user(ctx: &MutationContext, name: String) -> Result<User> {
                ctx.dispatch_job("send_email", json!({})).await?;
                Ok(User { name })
            }
            "#,
        )
        .unwrap();

        let attrs = MutationAttrs {
            transactional: true,
            ..Default::default()
        };
        let result = expand_mutation_impl(input, attrs);
        assert!(
            result.is_ok(),
            "Should accept dispatch_job with transactional"
        );
    }

    #[test]
    fn accepts_mutation_without_dispatch() {
        let input: ItemFn = syn::parse_str(
            r#"
            pub async fn update_name(ctx: &MutationContext, name: String) -> Result<()> {
                Ok(())
            }
            "#,
        )
        .unwrap();

        let attrs = MutationAttrs::default();
        let result = expand_mutation_impl(input, attrs);
        assert!(
            result.is_ok(),
            "Simple mutation without dispatch should work"
        );
    }

    // --- Validation: async requirement ---

    #[test]
    fn rejects_non_async_mutation() {
        let input: ItemFn = syn::parse_str(
            r#"
            pub fn create_user(ctx: &MutationContext) -> Result<()> {
                Ok(())
            }
            "#,
        )
        .unwrap();

        let attrs = MutationAttrs::default();
        let result = expand_mutation_impl(input, attrs);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("async"),
            "Error should mention async: {err_msg}"
        );
    }

    // --- Validation: context parameter ---

    #[test]
    fn rejects_mutation_without_parameters() {
        let input: ItemFn = syn::parse_str(
            r#"
            pub async fn create_user() -> Result<()> {
                Ok(())
            }
            "#,
        )
        .unwrap();

        let attrs = MutationAttrs::default();
        let result = expand_mutation_impl(input, attrs);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("MutationContext"),
            "Error should mention context param: {err_msg}"
        );
    }

    // --- Generated output structure ---

    #[test]
    fn generates_struct_for_no_arg_mutation() {
        let input: ItemFn = syn::parse_str(
            r#"
            pub async fn reset_all(ctx: &MutationContext) -> Result<()> {
                Ok(())
            }
            "#,
        )
        .unwrap();

        let attrs = MutationAttrs::default();
        let output = expand_mutation_impl(input, attrs).expect("should expand");
        let output_str = output.to_string();
        assert!(
            output_str.contains("ResetAllMutation"),
            "Should generate PascalCase struct name"
        );
        assert!(
            output_str.contains("ForgeMutation"),
            "Should implement ForgeMutation trait"
        );
        assert!(
            output_str.contains("inventory"),
            "Should register via inventory"
        );
    }

    #[test]
    fn generates_info_with_attributes() {
        let input: ItemFn = syn::parse_str(
            r#"
            pub async fn create_item(ctx: &MutationContext) -> Result<()> {
                Ok(())
            }
            "#,
        )
        .unwrap();

        let attrs = MutationAttrs {
            is_public: true,
            transactional: true,
            required_role: Some("admin".into()),
            ..Default::default()
        };
        let output = expand_mutation_impl(input, attrs).expect("should expand");
        let output_str = output.to_string();
        assert!(output_str.contains("is_public : true"));
        assert!(output_str.contains("transactional : true"));
        assert!(
            output_str.contains(r#"Some ("admin")"#) || output_str.contains(r#"Some("admin")"#)
        );
    }
}
