use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{FnArg, ItemFn, Pat, ReturnType, Type, parse_macro_input};

use crate::utils::{has_attr_flag, parse_duration_secs, to_pascal_case};

/// Expand the #[forge::mutation] attribute.
///
/// This transforms an async function into a mutation handler that:
/// - Takes a MutationContext as the first parameter
/// - Returns a Result<T>
/// - Runs within a database transaction
/// - Generates a struct implementing ForgeMutation trait
pub fn expand_mutation(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    let attrs = parse_mutation_attrs(attr);

    expand_mutation_impl(input, attrs)
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

#[derive(Default)]
struct MutationAttrs {
    required_role: Option<String>,
    is_public: bool,
    timeout: Option<u64>,
    rate_limit_requests: Option<u32>,
    rate_limit_per_secs: Option<u64>,
    rate_limit_key: Option<String>,
    log_level: Option<String>,
    transactional: bool,
}

fn parse_mutation_attrs(attr: TokenStream) -> MutationAttrs {
    let mut attrs = MutationAttrs::default();

    let attr_str = attr.to_string();

    if has_attr_flag(&attr_str, "transactional") {
        attrs.transactional = true;
    }

    if has_attr_flag(&attr_str, "public") {
        attrs.is_public = true;
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

    if let Some(timeout_start) = attr_str.find("timeout")
        && let Some(eq_pos) = attr_str[timeout_start..].find('=')
    {
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

    attrs
}

fn expand_mutation_impl(input: ItemFn, attrs: MutationAttrs) -> syn::Result<TokenStream2> {
    let fn_name = &input.sig.ident;
    let fn_name_str = fn_name.to_string();
    let struct_name = syn::Ident::new(
        &format!("{}Mutation", to_pascal_case(&fn_name_str)),
        fn_name.span(),
    );

    let vis = &input.vis;
    let asyncness = &input.sig.asyncness;
    let fn_block = &input.block;
    let fn_attrs = &input.attrs;

    // Check for dispatch_job or start_workflow calls without transactional attribute
    let block_str = quote! { #fn_block }.to_string();
    let has_dispatch = block_str.contains("dispatch_job") || block_str.contains("start_workflow");
    if has_dispatch && !attrs.transactional {
        return Err(syn::Error::new_spanned(
            &input.sig.ident,
            "Mutations that call `dispatch_job()` or `start_workflow()` must use \
             #[forge::mutation(transactional)] to ensure atomicity. Without it, \
             jobs may be dispatched but database changes rolled back on error.",
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

        impl forge::forge_core::ForgeMutation for #struct_name {
            type Args = #args_type;
            type Output = #output_type;

            fn info() -> forge::forge_core::FunctionInfo {
                forge::forge_core::FunctionInfo {
                    name: #fn_name_str,
                    description: None,
                    kind: forge::forge_core::FunctionKind::Mutation,
                    required_role: #required_role,
                    is_public: #is_public,
                    cache_ttl: None,
                    timeout: #timeout,
                    rate_limit_requests: #rate_limit_requests,
                    rate_limit_per_secs: #rate_limit_per_secs,
                    rate_limit_key: #rate_limit_key,
                    log_level: #log_level,
                    table_dependencies: &[],
                    selected_columns: &[],
                    transactional: #transactional,
                    consistent: false,
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
    })
}
