use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{FnArg, ItemFn, Pat, ReturnType, Type, parse_macro_input};

use crate::utils::{has_attr_flag, parse_duration_secs, to_pascal_case};

/// Expand the #[forge::mcp_tool] attribute.
pub fn expand_mcp_tool(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    let attrs = parse_mcp_tool_attrs(attr);

    expand_mcp_tool_impl(input, attrs)
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

#[derive(Default)]
struct McpToolAttrs {
    name: Option<String>,
    title: Option<String>,
    description: Option<String>,
    required_role: Option<String>,
    is_public: bool,
    timeout: Option<u64>,
    rate_limit_requests: Option<u32>,
    rate_limit_per_secs: Option<u64>,
    rate_limit_key: Option<String>,
    read_only_hint: Option<bool>,
    destructive_hint: Option<bool>,
    idempotent_hint: Option<bool>,
    open_world_hint: Option<bool>,
}

fn parse_mcp_tool_attrs(attr: TokenStream) -> McpToolAttrs {
    let mut attrs = McpToolAttrs::default();
    let attr_str = attr.to_string();

    if has_attr_flag(&attr_str, "public") {
        attrs.is_public = true;
    }

    if has_attr_flag(&attr_str, "read_only") {
        attrs.read_only_hint = Some(true);
    }
    if has_attr_flag(&attr_str, "destructive") {
        attrs.destructive_hint = Some(true);
    }
    if has_attr_flag(&attr_str, "idempotent") {
        attrs.idempotent_hint = Some(true);
    }
    if has_attr_flag(&attr_str, "open_world") {
        attrs.open_world_hint = Some(true);
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

    if let Some(name_start) = attr_str.find("name")
        && let Some(eq_pos) = attr_str[name_start..].find('=')
    {
        let after_eq = &attr_str[name_start + eq_pos + 1..];
        if let Some(quote_start) = after_eq.find('"') {
            let after_quote = &after_eq[quote_start + 1..];
            if let Some(quote_end) = after_quote.find('"') {
                attrs.name = Some(after_quote[..quote_end].to_string());
            }
        }
    }

    if let Some(title_start) = attr_str.find("title")
        && let Some(eq_pos) = attr_str[title_start..].find('=')
    {
        let after_eq = &attr_str[title_start + eq_pos + 1..];
        if let Some(quote_start) = after_eq.find('"') {
            let after_quote = &after_eq[quote_start + 1..];
            if let Some(quote_end) = after_quote.find('"') {
                attrs.title = Some(after_quote[..quote_end].to_string());
            }
        }
    }

    if let Some(desc_start) = attr_str.find("description")
        && let Some(eq_pos) = attr_str[desc_start..].find('=')
    {
        let after_eq = &attr_str[desc_start + eq_pos + 1..];
        if let Some(quote_start) = after_eq.find('"') {
            let after_quote = &after_eq[quote_start + 1..];
            if let Some(quote_end) = after_quote.find('"') {
                attrs.description = Some(after_quote[..quote_end].to_string());
            }
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

    attrs
}

fn validate_tool_name(name: &str) -> syn::Result<()> {
    if name.is_empty() || name.len() > 128 {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "MCP tool names must be 1-128 characters long",
        ));
    }

    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
    {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "MCP tool names may only contain ASCII letters, numbers, '_', '-', and '.'",
        ));
    }

    Ok(())
}

fn is_schema_field_attr(attr: &syn::Attribute) -> bool {
    let path = attr.path();
    path.is_ident("schemars") || path.is_ident("serde") || path.is_ident("doc")
}

fn tool_type_stem(fn_name: &str) -> &str {
    fn_name
        .strip_suffix("_mcp_tool")
        .or_else(|| fn_name.strip_suffix("_tool"))
        .filter(|stem| !stem.is_empty())
        .unwrap_or(fn_name)
}

fn expand_mcp_tool_impl(input: ItemFn, attrs: McpToolAttrs) -> syn::Result<TokenStream2> {
    let fn_name = &input.sig.ident;
    let fn_name_str = attrs.name.unwrap_or_else(|| fn_name.to_string());
    validate_tool_name(&fn_name_str)?;

    let fn_name_value = fn_name.to_string();
    let struct_name = syn::Ident::new(
        &format!("{}McpTool", to_pascal_case(tool_type_stem(&fn_name_value))),
        fn_name.span(),
    );
    let vis = &input.vis;
    let asyncness = &input.sig.asyncness;
    let fn_block = &input.block;
    let fn_attrs = &input.attrs;

    if asyncness.is_none() {
        return Err(syn::Error::new_spanned(
            &input.sig,
            "MCP tool functions must be async",
        ));
    }

    let params: Vec<_> = input.sig.inputs.iter().collect();
    if params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.sig,
            "MCP tool functions must have at least a McpToolContext parameter",
        ));
    }

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

    let type_str = quote! { #ctx_type }.to_string();
    let is_ref = type_str.starts_with('&');

    let arg_params: Vec<_> = params.iter().skip(1).cloned().collect();

    let args_fields: Vec<TokenStream2> = arg_params
        .iter()
        .filter_map(|p| {
            if let FnArg::Typed(pat_type) = p
                && let Pat::Ident(pat_ident) = &*pat_type.pat
            {
                let name = &pat_ident.ident;
                let ty = &pat_type.ty;
                let field_attrs: Vec<_> = pat_type
                    .attrs
                    .iter()
                    .filter(|attr| is_schema_field_attr(attr))
                    .collect();
                return Some(quote! {
                    #(#field_attrs)*
                    pub #name: #ty
                });
            }
            None
        })
        .collect();

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

    let timeout = match attrs.timeout {
        Some(t) => quote! { Some(#t) },
        None => quote! { None },
    };

    let required_role = match &attrs.required_role {
        Some(role) => quote! { Some(#role) },
        None => quote! { None },
    };

    let title = match &attrs.title {
        Some(t) => quote! { Some(#t) },
        None => quote! { None },
    };

    let description = match &attrs.description {
        Some(d) => quote! { Some(#d) },
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

    let read_only_hint = match attrs.read_only_hint {
        Some(v) => quote! { Some(#v) },
        None => quote! { None },
    };
    let destructive_hint = match attrs.destructive_hint {
        Some(v) => quote! { Some(#v) },
        None => quote! { None },
    };
    let idempotent_hint = match attrs.idempotent_hint {
        Some(v) => quote! { Some(#v) },
        None => quote! { None },
    };
    let open_world_hint = match attrs.open_world_hint {
        Some(v) => quote! { Some(#v) },
        None => quote! { None },
    };

    let is_public = attrs.is_public;

    let single_custom_args_type: Option<&Type> = if arg_params.len() == 1 {
        if let FnArg::Typed(pat_type) = &arg_params[0] {
            if let Type::Path(type_path) = &*pat_type.ty {
                if let Some(segment) = type_path.path.segments.last() {
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

    let (args_struct, args_type, execute_call) = if arg_params.is_empty() {
        let args_struct_name = syn::Ident::new(&format!("{}Args", struct_name), fn_name.span());
        (
            quote! {
                #[derive(Debug, Clone, serde::Deserialize, forge::forge_core::schemars::JsonSchema)]
                #vis struct #args_struct_name {}

                #vis struct #struct_name;
            },
            quote! { #args_struct_name },
            quote! { #fn_name(ctx).await },
        )
    } else if let Some(user_args_type) = single_custom_args_type {
        (
            quote! {
                #vis struct #struct_name;
            },
            quote! { #user_args_type },
            quote! { #fn_name(ctx, args).await },
        )
    } else {
        let args_struct_name = syn::Ident::new(&format!("{}Args", struct_name), fn_name.span());
        (
            quote! {
                #[derive(Debug, Clone, serde::Deserialize, forge::forge_core::schemars::JsonSchema)]
                #vis struct #args_struct_name {
                    #(#args_fields),*
                }

                #vis struct #struct_name;
            },
            quote! { #args_struct_name },
            quote! { #fn_name(ctx, #(args.#arg_names),*).await },
        )
    };

    let inner_fn = if is_ref {
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
    } else if arg_names.is_empty() {
        quote! {
            #(#fn_attrs)*
            #vis async fn #fn_name(#ctx_name: &#ctx_type) -> forge::forge_core::Result<#output_type> #fn_block
        }
    } else {
        quote! {
            #(#fn_attrs)*
            #vis async fn #fn_name(#ctx_name: &#ctx_type, #(#arg_params),*) -> forge::forge_core::Result<#output_type> #fn_block
        }
    };

    Ok(quote! {
        #args_struct

        #inner_fn

        impl forge::forge_core::ForgeMcpTool for #struct_name {
            type Args = #args_type;
            type Output = #output_type;

            fn info() -> forge::forge_core::McpToolInfo {
                forge::forge_core::McpToolInfo {
                    name: #fn_name_str,
                    title: #title,
                    description: #description,
                    required_role: #required_role,
                    is_public: #is_public,
                    timeout: #timeout,
                    rate_limit_requests: #rate_limit_requests,
                    rate_limit_per_secs: #rate_limit_per_secs,
                    rate_limit_key: #rate_limit_key,
                    annotations: forge::forge_core::McpToolAnnotations {
                        title: #title,
                        read_only_hint: #read_only_hint,
                        destructive_hint: #destructive_hint,
                        idempotent_hint: #idempotent_hint,
                        open_world_hint: #open_world_hint,
                    },
                    icons: &[],
                }
            }

            fn execute(
                ctx: &forge::forge_core::McpToolContext,
                args: Self::Args,
            ) -> std::pin::Pin<Box<dyn std::future::Future<Output = forge::forge_core::Result<Self::Output>> + Send + '_>> {
                Box::pin(async move {
                    #execute_call
                })
            }
        }

        forge::inventory::submit!(forge::AutoMcpTool(|registry| {
            registry.register::<#struct_name>();
        }));
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_tool_name_accepts_valid_names() {
        assert!(validate_tool_name("get_user").is_ok());
        assert!(validate_tool_name("admin.tools.list").is_ok());
        assert!(validate_tool_name("DATA_EXPORT_v2").is_ok());
    }

    #[test]
    fn test_validate_tool_name_rejects_invalid_names() {
        assert!(validate_tool_name("").is_err());
        assert!(validate_tool_name("with space").is_err());
        assert!(validate_tool_name("weird,comma").is_err());
    }

    #[test]
    fn test_generated_args_preserve_schema_field_attributes() {
        let input: ItemFn = syn::parse_quote! {
            pub async fn describe_weather(
                ctx: &McpToolContext,
                #[schemars(description = "City name or zip code", length(min = 1))]
                location: String,
                #[serde(default)]
                unit: Option<String>,
            ) -> forge::forge_core::Result<String> {
                Ok(format!("{}:{:?}", location, unit))
            }
        };

        let expanded =
            expand_mcp_tool_impl(input, McpToolAttrs::default()).expect("macro expansion succeeds");
        let tokens = expanded.to_string();

        assert!(tokens.contains("schemars"));
        assert!(tokens.contains("City name or zip code"));
        assert!(tokens.contains("serde"));
        assert!(tokens.contains("pub location : String"));
        assert!(tokens.contains("pub unit : Option < String >"));
    }

    #[test]
    fn test_tool_struct_name_strips_redundant_tool_suffix() {
        assert_eq!(tool_type_stem("export_project_tool"), "export_project");
        assert_eq!(tool_type_stem("sync_users_mcp_tool"), "sync_users");
        assert_eq!(tool_type_stem("lookup"), "lookup");
    }
}
