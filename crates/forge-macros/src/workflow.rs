use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::visit::Visit;
use syn::{ExprAwait, ExprCall, ItemFn, Lit, parse_macro_input};

/// Minimum sleep duration (in seconds) that triggers the tokio::sleep warning.
/// Sleeps shorter than this are allowed since they're typically used for polling/retry loops.
const TOKIO_SLEEP_THRESHOLD_SECS: u64 = 100;

/// Detects tokio::sleep calls with durations exceeding the threshold.
/// Returns the span of the first violation found, if any.
struct TokioSleepDetector {
    violation_span: Option<proc_macro2::Span>,
}

impl TokioSleepDetector {
    fn new() -> Self {
        Self {
            violation_span: None,
        }
    }

    /// Try to extract a duration in seconds from common patterns.
    fn extract_duration_secs(
        args: &syn::punctuated::Punctuated<syn::Expr, syn::token::Comma>,
    ) -> Option<u64> {
        if args.len() != 1 {
            return None;
        }

        if let syn::Expr::Call(call) = &args[0] {
            if let syn::Expr::Path(path) = &*call.func {
                let path_str: String = path
                    .path
                    .segments
                    .iter()
                    .map(|s| s.ident.to_string())
                    .collect::<Vec<_>>()
                    .join("::");

                if path_str.ends_with("from_secs") {
                    if let Some(syn::Expr::Lit(lit)) = call.args.first() {
                        if let Lit::Int(int_lit) = &lit.lit {
                            return int_lit.base10_parse::<u64>().ok();
                        }
                    }
                } else if path_str.ends_with("from_millis") {
                    if let Some(syn::Expr::Lit(lit)) = call.args.first() {
                        if let Lit::Int(int_lit) = &lit.lit {
                            return int_lit.base10_parse::<u64>().ok().map(|ms| ms / 1000);
                        }
                    }
                } else if path_str.ends_with("from_days") {
                    if let Some(syn::Expr::Lit(lit)) = call.args.first() {
                        if let Lit::Int(int_lit) = &lit.lit {
                            return int_lit.base10_parse::<u64>().ok().map(|d| d * 86400);
                        }
                    }
                }
            }
        }
        None
    }

    fn check_sleep_call(
        &mut self,
        path_str: &str,
        args: &syn::punctuated::Punctuated<syn::Expr, syn::token::Comma>,
        span: proc_macro2::Span,
    ) {
        if self.violation_span.is_some() {
            return;
        }

        let is_tokio_sleep =
            path_str.contains("tokio") && path_str.contains("sleep") || path_str == "sleep";

        if !is_tokio_sleep {
            return;
        }

        match Self::extract_duration_secs(args) {
            Some(secs) if secs <= TOKIO_SLEEP_THRESHOLD_SECS => {}
            _ => self.violation_span = Some(span),
        }
    }
}

impl<'ast> Visit<'ast> for TokioSleepDetector {
    fn visit_expr_call(&mut self, node: &'ast ExprCall) {
        if let syn::Expr::Path(path) = &*node.func {
            let path_str: String = path
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect::<Vec<_>>()
                .join("::");

            let span = path
                .path
                .segments
                .last()
                .map(|s| s.ident.span())
                .unwrap_or_else(proc_macro2::Span::call_site);

            self.check_sleep_call(&path_str, &node.args, span);
        }
        syn::visit::visit_expr_call(self, node);
    }

    fn visit_expr_await(&mut self, node: &'ast ExprAwait) {
        // Check for tokio::time::sleep(...).await pattern
        if let syn::Expr::Call(call) = &*node.base {
            if let syn::Expr::Path(path) = &*call.func {
                let path_str: String = path
                    .path
                    .segments
                    .iter()
                    .map(|s| s.ident.to_string())
                    .collect::<Vec<_>>()
                    .join("::");

                let span = path
                    .path
                    .segments
                    .last()
                    .map(|s| s.ident.span())
                    .unwrap_or_else(proc_macro2::Span::call_site);

                self.check_sleep_call(&path_str, &call.args, span);
            }
        }
        syn::visit::visit_expr_await(self, node);
    }
}

/// Workflow attributes.
#[derive(Debug, Default)]
struct WorkflowAttrs {
    version: Option<u32>,
    timeout: Option<String>,
    deprecated: bool,
    is_public: bool,
    required_role: Option<String>,
}

fn parse_workflow_attrs(attr: TokenStream) -> WorkflowAttrs {
    let mut result = WorkflowAttrs::default();
    let attr_str = attr.to_string();

    // Parse version = N
    if let Some(version_start) = attr_str.find("version") {
        if let Some(eq_pos) = attr_str[version_start..].find('=') {
            let remaining = &attr_str[version_start + eq_pos + 1..];
            if let Ok(v) = remaining
                .split(&[',', ')'])
                .next()
                .unwrap_or("")
                .trim()
                .parse::<u32>()
            {
                result.version = Some(v);
            }
        }
    }

    // Parse timeout = "Xh" or timeout = "Xm" etc
    if let Some(timeout_start) = attr_str.find("timeout") {
        if let Some(quote_start) = attr_str[timeout_start..].find('"') {
            let remaining = &attr_str[timeout_start + quote_start + 1..];
            if let Some(quote_end) = remaining.find('"') {
                let timeout_str = &remaining[..quote_end];
                result.timeout = Some(timeout_str.to_string());
            }
        }
    }

    // Parse deprecated flag
    if attr_str.contains("deprecated") {
        result.deprecated = true;
    }

    // Parse public flag
    if attr_str.contains("public") {
        result.is_public = true;
    }

    // Parse require_role("admin")
    if let Some(role_start) = attr_str.find("require_role") {
        if let Some(paren_start) = attr_str[role_start..].find('(') {
            let remaining = &attr_str[role_start + paren_start + 1..];
            if let Some(paren_end) = remaining.find(')') {
                let role = remaining[..paren_end].trim().trim_matches('"');
                result.required_role = Some(role.to_string());
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
    } else if s.ends_with('d') {
        let n: u64 = s.trim_end_matches('d').parse().unwrap_or(1);
        let secs = n * 86400;
        quote! { std::time::Duration::from_secs(#secs) }
    } else {
        let n: u64 = s.parse().unwrap_or(86400);
        quote! { std::time::Duration::from_secs(#n) }
    }
}

pub fn workflow_impl(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    let attrs = parse_workflow_attrs(attr);

    let fn_name = &input.sig.ident;
    let fn_name_str = fn_name.to_string();
    let struct_name = format_ident!("{}Workflow", to_pascal_case(&fn_name.to_string()));

    let vis = &input.vis;
    let block = &input.block;

    // Detect tokio::sleep usage (only for long sleeps > 100s)
    let mut sleep_detector = TokioSleepDetector::new();
    sleep_detector.visit_block(block);
    if let Some(span) = sleep_detector.violation_span {
        return syn::Error::new(
            span,
            "Use `ctx.sleep()` instead of `tokio::sleep()` for long sleeps in workflows. \
             Workflows require durable sleep that survives process restarts. \
             Short sleeps (<100s) for polling are allowed with tokio::sleep.",
        )
        .to_compile_error()
        .into();
    }

    // Parse input type from function signature
    let mut input_type = quote! { () };
    let mut input_ident = format_ident!("_input");

    for (i, input_arg) in input.sig.inputs.iter().enumerate() {
        if i == 0 {
            continue; // Skip context
        }
        if let syn::FnArg::Typed(pat_type) = input_arg {
            if let syn::Pat::Ident(ident) = pat_type.pat.as_ref() {
                input_ident = ident.ident.clone();
            }
            let ty = &pat_type.ty;
            input_type = quote! { #ty };
        }
    }

    // Parse return type
    let output_type = match &input.sig.output {
        syn::ReturnType::Default => quote! { () },
        syn::ReturnType::Type(_, ty) => {
            if let syn::Type::Path(path) = ty.as_ref() {
                if let Some(segment) = path.path.segments.last() {
                    if segment.ident == "Result" {
                        if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                            if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                                quote! { #inner }
                            } else {
                                quote! { () }
                            }
                        } else {
                            quote! { () }
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

    let version = attrs.version.unwrap_or(1);
    let deprecated = attrs.deprecated;
    let is_public = attrs.is_public;
    let required_role = if let Some(ref role) = attrs.required_role {
        quote! { Some(#role) }
    } else {
        quote! { None }
    };

    let timeout = if let Some(ref t) = attrs.timeout {
        parse_duration(t)
    } else {
        quote! { std::time::Duration::from_secs(86400) } // 24 hours default
    };

    let fn_attrs = &input.attrs;

    let expanded = quote! {
        #(#fn_attrs)*
        #vis struct #struct_name;

        impl forge::forge_core::workflow::ForgeWorkflow for #struct_name {
            type Input = #input_type;
            type Output = #output_type;

            fn info() -> forge::forge_core::workflow::WorkflowInfo {
                forge::forge_core::workflow::WorkflowInfo {
                    name: #fn_name_str,
                    version: #version,
                    timeout: #timeout,
                    deprecated: #deprecated,
                    is_public: #is_public,
                    required_role: #required_role,
                }
            }

            fn execute(
                ctx: &forge::forge_core::workflow::WorkflowContext,
                #input_ident: Self::Input,
            ) -> std::pin::Pin<Box<dyn std::future::Future<Output = forge::forge_core::Result<Self::Output>> + Send + '_>> {
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
        assert_eq!(to_pascal_case("user_onboarding"), "UserOnboarding");
        assert_eq!(to_pascal_case("order_processing"), "OrderProcessing");
        assert_eq!(to_pascal_case("simple"), "Simple");
    }

    #[test]
    fn test_parse_duration_days() {
        let ts = parse_duration("7d");
        assert!(!ts.is_empty());
    }

    #[test]
    fn test_parse_duration_hours() {
        let ts = parse_duration("24h");
        assert!(!ts.is_empty());
    }
}
