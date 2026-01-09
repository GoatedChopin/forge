use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{Data, DeriveInput, Fields, Meta, parse_macro_input, spanned::Spanned};

/// Expand the #[forge::model] macro.
///
/// Generates:
/// - Struct with Debug, Clone, Serialize, Deserialize derives
/// - ModelMeta trait implementation for TypeScript codegen
pub fn expand_model(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);

    match expand_model_impl(attr.into(), input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

fn expand_model_impl(_attr: TokenStream2, input: DeriveInput) -> syn::Result<TokenStream2> {
    let struct_name = &input.ident;
    let table_name = get_table_name(&input)?;
    let vis = &input.vis;

    // Extract fields
    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            _ => {
                return Err(syn::Error::new(
                    input.span(),
                    "Only named fields are supported",
                ));
            }
        },
        _ => return Err(syn::Error::new(input.span(), "Only structs are supported")),
    };

    // Generate field definitions for TableDef (used for TypeScript codegen)
    let field_tokens: Vec<TokenStream2> = fields
        .iter()
        .map(|field| {
            let field_name = field.ident.as_ref().unwrap();
            let field_type = &field.ty;
            let type_str = quote!(#field_type).to_string();
            let name = field_name.to_string();
            let column_name = to_snake_case(&name);

            quote! {
                {
                    let rust_type = forge::forge_core::schema::RustType::from_type_string(#type_str);
                    let mut field = forge::forge_core::schema::FieldDef::new(#name, rust_type);
                    field.column_name = #column_name.to_string();
                    field
                }
            }
        })
        .collect();

    // Generate the impl
    let expanded = quote! {
        #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
        #vis struct #struct_name {
            #fields
        }

        impl forge::forge_core::schema::ModelMeta for #struct_name {
            const TABLE_NAME: &'static str = #table_name;

            fn table_def() -> forge::forge_core::schema::TableDef {
                let mut table = forge::forge_core::schema::TableDef::new(#table_name, stringify!(#struct_name));
                table.fields = vec![
                    #(#field_tokens),*
                ];
                table
            }

            fn primary_key_field() -> &'static str {
                "id"
            }
        }
    };

    Ok(expanded)
}

fn get_table_name(input: &DeriveInput) -> syn::Result<String> {
    // Look for #[table(name = "...")]
    for attr in &input.attrs {
        if attr.path().is_ident("table") {
            let meta = attr.meta.clone();
            if let Meta::List(list) = meta {
                let tokens: TokenStream2 = list.tokens;
                let tokens_str = tokens.to_string();
                if tokens_str.starts_with("name") {
                    if let Some(value) = extract_string_value(&tokens_str) {
                        return Ok(value);
                    }
                }
            }
        }
    }

    // Default: convert struct name to snake_case plural
    let name = to_snake_case(&input.ident.to_string());
    Ok(pluralize(&name))
}

fn extract_string_value(s: &str) -> Option<String> {
    // Parse "name = \"value\"" pattern
    let parts: Vec<&str> = s.splitn(2, '=').collect();
    if parts.len() == 2 {
        let value = parts[1].trim();
        if let Some(stripped) = value.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
            return Some(stripped.to_string());
        }
    }
    None
}

fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(c.to_lowercase().next().unwrap());
        } else {
            result.push(c);
        }
    }
    result
}

fn pluralize(s: &str) -> String {
    // Simple English pluralization rules
    if s.ends_with('s')
        || s.ends_with("sh")
        || s.ends_with("ch")
        || s.ends_with('x')
        || s.ends_with('z')
    {
        format!("{}es", s)
    } else if let Some(stem) = s.strip_suffix('y') {
        if !s.ends_with("ay") && !s.ends_with("ey") && !s.ends_with("oy") && !s.ends_with("uy") {
            format!("{}ies", stem)
        } else {
            format!("{}s", s)
        }
    } else {
        format!("{}s", s)
    }
}
