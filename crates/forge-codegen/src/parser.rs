//! Rust source code parser for extracting FORGE schema definitions.
//!
//! Parses Rust source files using `syn` to extract model, enum, and function
//! definitions without requiring compilation.
//!
//! Key design decisions:
//! - Context arguments are detected structurally (type ends with "Context"),
//!   not by string-searching the entire token stream.
//! - Unparseable inner types become `RustType::Custom(original_string)` instead
//!   of silently falling back to `String`.
//! - `NaiveTime` correctly maps to `RustType::LocalTime`.

use std::path::{Path, PathBuf};

use forge_core::schema::{
    EnumDef, EnumVariant, FieldDef, FunctionArg, FunctionDef, FunctionKind, RustType,
    SchemaRegistry, TableDef,
};
use forge_core::util::to_snake_case;
use quote::ToTokens;
use syn::{Attribute, Expr, Fields, FnArg, Lit, Meta, Pat, ReturnType};

use crate::Error;

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// Parse all Rust source files in a directory and extract schema definitions.
pub fn parse_project(src_dir: &Path) -> Result<SchemaRegistry, Error> {
    let registry = SchemaRegistry::new();

    let mut files = Vec::new();
    collect_rs_files(src_dir, &mut files);
    files.sort();

    for path in &files {
        let content = std::fs::read_to_string(path)?;
        if let Err(e) = parse_file(&content, &registry) {
            tracing::debug!(file = ?path, error = %e, "Failed to parse file");
        }
    }

    Ok(registry)
}

/// Parse a single Rust source file and extract schema definitions.
fn parse_file(content: &str, registry: &SchemaRegistry) -> Result<(), Error> {
    let file = syn::parse_file(content).map_err(|e| Error::Parse {
        file: String::new(),
        message: e.to_string(),
    })?;

    for item in file.items {
        match item {
            syn::Item::Struct(item_struct) => {
                if has_forge_attr(&item_struct.attrs, "model") {
                    if let Some(table) = parse_model(&item_struct) {
                        registry.register_table(table);
                    }
                } else if has_serde_derive(&item_struct.attrs)
                    && let Some(table) = parse_dto_struct(&item_struct)
                {
                    registry.register_table(table);
                }
            }
            syn::Item::Enum(item_enum) => {
                if (has_forge_enum_attr(&item_enum.attrs) || has_serde_derive(&item_enum.attrs))
                    && let Some(enum_def) = parse_enum(&item_enum)
                {
                    registry.register_enum(enum_def);
                }
            }
            syn::Item::Fn(item_fn) => {
                if let Some(func) = parse_function(&item_fn) {
                    registry.register_function(func);
                }
            }
            _ => {}
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Attribute detection
// ---------------------------------------------------------------------------

/// Check if attributes contain `#[forge::name]` or `#[name]`.
fn has_forge_attr(attrs: &[Attribute], name: &str) -> bool {
    attrs.iter().any(|attr| {
        let path = attr.path();
        path.is_ident(name)
            || matches!(
                (path.segments.first(), path.segments.get(1), path.segments.get(2)),
                (Some(first), Some(second), None)
                    if first.ident == "forge" && second.ident == name
            )
    })
}

/// Check if attributes contain `#[forge_enum]`, `#[enum_type]`, or `#[forge::enum_type]`.
fn has_forge_enum_attr(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        let path = attr.path();
        path.is_ident("forge_enum")
            || path.is_ident("enum_type")
            || matches!(
                (path.segments.first(), path.segments.get(1), path.segments.get(2)),
                (Some(first), Some(second), None)
                    if first.ident == "forge"
                        && (second.ident == "enum_type" || second.ident == "forge_enum")
            )
    })
}

/// Check if attributes contain `#[derive(...Serialize...)]` or `#[derive(...Deserialize...)]`.
fn has_serde_derive(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        if !attr.path().is_ident("derive") {
            return false;
        }
        let tokens = attr.meta.to_token_stream().to_string();
        tokens.contains("Serialize") || tokens.contains("Deserialize")
    })
}

// ---------------------------------------------------------------------------
// Struct/model parsing
// ---------------------------------------------------------------------------

/// Parse a DTO struct (with Serialize/Deserialize) into a TableDef.
fn parse_dto_struct(item: &syn::ItemStruct) -> Option<TableDef> {
    let struct_name = item.ident.to_string();
    let mut table = TableDef::new(&struct_name, &struct_name);
    table.is_dto = true;
    table.doc = get_doc_comment(&item.attrs);

    if let Fields::Named(fields) = &item.fields {
        for field in &fields.named {
            if let Some(field_name) = &field.ident {
                table
                    .fields
                    .push(parse_field(field_name.to_string(), &field.ty, &field.attrs));
            }
        }
    }

    Some(table)
}

/// Parse a struct with `#[model]` attribute into a TableDef.
fn parse_model(item: &syn::ItemStruct) -> Option<TableDef> {
    let struct_name = item.ident.to_string();
    let table_name = get_table_name_from_attrs(&item.attrs).unwrap_or_else(|| {
        let snake = to_snake_case(&struct_name);
        pluralize(&snake)
    });

    let mut table = TableDef::new(&table_name, &struct_name);
    table.doc = get_doc_comment(&item.attrs);

    if let Fields::Named(fields) = &item.fields {
        for field in &fields.named {
            if let Some(field_name) = &field.ident {
                table
                    .fields
                    .push(parse_field(field_name.to_string(), &field.ty, &field.attrs));
            }
        }
    }

    Some(table)
}

fn parse_field(name: String, ty: &syn::Type, attrs: &[Attribute]) -> FieldDef {
    let rust_type = type_to_rust_type(ty);
    let mut field = FieldDef::new(&name, rust_type);
    field.column_name = to_snake_case(&name);
    field.doc = get_doc_comment(attrs);
    field
}

// ---------------------------------------------------------------------------
// Enum parsing
// ---------------------------------------------------------------------------

fn parse_enum(item: &syn::ItemEnum) -> Option<EnumDef> {
    let enum_name = item.ident.to_string();
    let mut enum_def = EnumDef::new(&enum_name);
    enum_def.doc = get_doc_comment(&item.attrs);

    for variant in &item.variants {
        let variant_name = variant.ident.to_string();
        let mut enum_variant = EnumVariant::new(&variant_name);
        enum_variant.doc = get_doc_comment(&variant.attrs);

        if let Some((_, Expr::Lit(lit))) = &variant.discriminant
            && let Lit::Int(int_lit) = &lit.lit
            && let Ok(value) = int_lit.base10_parse::<i32>()
        {
            enum_variant.int_value = Some(value);
        }

        enum_def.variants.push(enum_variant);
    }

    Some(enum_def)
}

// ---------------------------------------------------------------------------
// Function parsing
// ---------------------------------------------------------------------------

/// Parse a function with a forge decorator attribute.
fn parse_function(item: &syn::ItemFn) -> Option<FunctionDef> {
    let kind = get_function_kind(&item.attrs)?;
    let func_name = item.sig.ident.to_string();

    let return_type = match &item.sig.output {
        ReturnType::Default => RustType::Custom("()".to_string()),
        ReturnType::Type(_, ty) => extract_result_type(ty),
    };

    let mut func = FunctionDef::new(&func_name, kind, return_type);
    func.doc = get_doc_comment(&item.attrs);
    func.is_async = item.sig.asyncness.is_some();

    // Parse arguments, skipping the context parameter.
    let mut is_first = true;
    for arg in &item.sig.inputs {
        if let FnArg::Typed(pat_type) = arg {
            if is_first {
                is_first = false;
                if is_context_type(&pat_type.ty) {
                    continue;
                }
            }

            if let Pat::Ident(pat_ident) = &*pat_type.pat {
                let arg_name = pat_ident.ident.to_string();
                let arg_type = type_to_rust_type(&pat_type.ty);
                func.args.push(FunctionArg::new(arg_name, arg_type));
            }
        }
    }

    Some(func)
}

/// Check if a type is a Forge context type by examining the base type name.
///
/// Handles references (`&Context`, `&mut Context`) and qualified paths
/// (`forge::QueryContext`). Only matches types whose final segment ends
/// with "Context" — won't match `ContextManager` or `NoContextHere`.
fn is_context_type(ty: &syn::Type) -> bool {
    // Get the type string, stripping whitespace for uniform matching.
    let type_str = ty.to_token_stream().to_string().replace(' ', "");

    // Strip leading references: &, &mut
    let base = type_str.trim_start_matches('&').trim_start_matches("mut");

    // Get the final path segment (after any :: qualifiers).
    let final_segment = base.rsplit("::").next().unwrap_or(base);

    final_segment.ends_with("Context")
}

fn get_function_kind(attrs: &[Attribute]) -> Option<FunctionKind> {
    for attr in attrs {
        let path = attr.path();
        let segments: Vec<_> = path.segments.iter().map(|s| s.ident.to_string()).collect();

        let kind_str = match segments.as_slice() {
            [forge, kind] if forge == "forge" => Some(kind.as_str()),
            [kind] => Some(kind.as_str()),
            _ => None,
        };

        if let Some(kind) = kind_str {
            match kind {
                "query" => return Some(FunctionKind::Query),
                "mutation" => return Some(FunctionKind::Mutation),
                "job" => return Some(FunctionKind::Job),
                "cron" => return Some(FunctionKind::Cron),
                "workflow" => return Some(FunctionKind::Workflow),
                _ => {}
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Type conversion
// ---------------------------------------------------------------------------

/// Extract the inner type from `Result<T, E>`.
fn extract_result_type(ty: &syn::Type) -> RustType {
    let type_str = quote::quote!(#ty).to_string().replace(' ', "");

    if let Some(rest) = type_str.strip_prefix("Result<") {
        // Find the inner type (T) before the comma or closing bracket.
        let mut depth = 0;
        let mut end_idx = 0;
        for (i, c) in rest.chars().enumerate() {
            match c {
                '<' => depth += 1,
                '>' => {
                    if depth == 0 {
                        end_idx = i;
                        break;
                    }
                    depth -= 1;
                }
                ',' if depth == 0 => {
                    end_idx = i;
                    break;
                }
                _ => {}
            }
        }
        let inner = &rest[..end_idx];
        return match syn::parse_str::<syn::Type>(inner) {
            Ok(inner_ty) => type_to_rust_type(&inner_ty),
            Err(_) => {
                tracing::warn!(
                    "Could not parse Result inner type '{}', treating as custom type",
                    inner
                );
                RustType::Custom(inner.to_string())
            }
        };
    }

    type_to_rust_type(ty)
}

/// Convert a `syn::Type` to `RustType`.
fn type_to_rust_type(ty: &syn::Type) -> RustType {
    let type_str = quote::quote!(#ty).to_string().replace(' ', "");

    match type_str.as_str() {
        "String" | "&str" => RustType::String,
        "i32" => RustType::I32,
        "i64" => RustType::I64,
        "f32" => RustType::F32,
        "f64" => RustType::F64,
        "bool" => RustType::Bool,
        "Uuid" | "uuid::Uuid" => RustType::Uuid,
        "DateTime<Utc>" | "chrono::DateTime<Utc>" | "chrono::DateTime<chrono::Utc>" => {
            RustType::Instant
        }
        "NaiveDate" | "chrono::NaiveDate" => RustType::LocalDate,
        "NaiveTime" | "chrono::NaiveTime" => RustType::LocalTime,
        "serde_json::Value" | "Value" => RustType::Json,
        "Vec<u8>" => RustType::Bytes,
        _ => parse_generic_or_custom(&type_str),
    }
}

/// Handle generic types (`Option<T>`, `Vec<T>`) and custom types.
fn parse_generic_or_custom(type_str: &str) -> RustType {
    // Option<T>
    if let Some(inner) = type_str
        .strip_prefix("Option<")
        .and_then(|s| s.strip_suffix('>'))
    {
        let inner_type = parse_inner_type(inner);
        return RustType::Option(Box::new(inner_type));
    }

    // Vec<T>
    if let Some(inner) = type_str
        .strip_prefix("Vec<")
        .and_then(|s| s.strip_suffix('>'))
    {
        if inner == "u8" {
            return RustType::Bytes;
        }
        let inner_type = parse_inner_type(inner);
        return RustType::Vec(Box::new(inner_type));
    }

    // Everything else is a custom type.
    RustType::Custom(type_str.to_string())
}

/// Parse an inner type string, falling back to Custom on failure.
fn parse_inner_type(inner: &str) -> RustType {
    match syn::parse_str::<syn::Type>(inner) {
        Ok(inner_ty) => type_to_rust_type(&inner_ty),
        Err(_) => {
            tracing::warn!(
                "Could not parse inner type '{}', treating as custom type",
                inner
            );
            RustType::Custom(inner.to_string())
        }
    }
}

// ---------------------------------------------------------------------------
// Attribute value helpers
// ---------------------------------------------------------------------------

/// Get `#[table(name = "...")]` value from attributes.
fn get_table_name_from_attrs(attrs: &[Attribute]) -> Option<String> {
    for attr in attrs {
        if attr.path().is_ident("table")
            && let Meta::List(list) = &attr.meta
        {
            let tokens = list.tokens.to_string();
            if let Some(value) = extract_name_value(&tokens) {
                return Some(value);
            }
        }
    }
    None
}

/// Get string value from attribute like `#[attr = "value"]`.
fn get_attribute_string_value(attr: &Attribute) -> Option<String> {
    if let Meta::NameValue(nv) = &attr.meta
        && let Expr::Lit(lit) = &nv.value
        && let Lit::Str(s) = &lit.lit
    {
        return Some(s.value());
    }
    None
}

/// Get documentation comment from attributes.
fn get_doc_comment(attrs: &[Attribute]) -> Option<String> {
    let docs: Vec<String> = attrs
        .iter()
        .filter_map(|attr| {
            if attr.path().is_ident("doc") {
                get_attribute_string_value(attr)
            } else {
                None
            }
        })
        .collect();

    if docs.is_empty() {
        None
    } else {
        Some(
            docs.into_iter()
                .map(|s| s.trim().to_string())
                .collect::<Vec<_>>()
                .join("\n"),
        )
    }
}

/// Extract name value from `name = "value"` format.
fn extract_name_value(s: &str) -> Option<String> {
    if let Some((_, value)) = s.split_once('=') {
        let value = value.trim();
        if let Some(stripped) = value.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
            return Some(stripped.to_string());
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Pluralization
// ---------------------------------------------------------------------------

/// Simple English pluralization for table names.
fn pluralize(s: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_model_source() {
        let source = r#"
            #[model]
            struct User {
                #[id]
                id: Uuid,
                email: String,
                name: Option<String>,
                #[indexed]
                created_at: DateTime<Utc>,
            }
        "#;

        let registry = SchemaRegistry::new();
        parse_file(source, &registry).expect("model source should parse");

        let table = registry
            .get_table("users")
            .expect("users table should be registered");
        assert_eq!(table.struct_name, "User");
        assert_eq!(table.fields.len(), 4);
    }

    #[test]
    fn test_parse_enum_source() {
        let source = r#"
            #[forge_enum]
            enum ProjectStatus {
                Draft,
                Active,
                Completed,
            }
        "#;

        let registry = SchemaRegistry::new();
        parse_file(source, &registry).expect("enum source should parse");

        let enum_def = registry
            .get_enum("ProjectStatus")
            .expect("ProjectStatus enum should be registered");
        assert_eq!(enum_def.variants.len(), 3);
    }

    #[test]
    fn test_to_snake_case() {
        assert_eq!(to_snake_case("UserProfile"), "user_profile");
        assert_eq!(to_snake_case("ID"), "i_d");
        assert_eq!(to_snake_case("createdAt"), "created_at");
    }

    #[test]
    fn test_pluralize() {
        assert_eq!(pluralize("user"), "users");
        assert_eq!(pluralize("category"), "categories");
        assert_eq!(pluralize("box"), "boxes");
        assert_eq!(pluralize("address"), "addresses");
    }

    #[test]
    fn test_parse_query_function() {
        let source = r#"
            #[query]
            async fn get_user(ctx: QueryContext, id: Uuid) -> Result<User> {
                todo!()
            }
        "#;

        let registry = SchemaRegistry::new();
        parse_file(source, &registry).expect("query function should parse");

        let func = registry
            .get_function("get_user")
            .expect("get_user function should be registered");
        assert_eq!(func.name, "get_user");
        assert_eq!(func.kind, FunctionKind::Query);
        assert!(func.is_async);
    }

    #[test]
    fn test_parse_mutation_function() {
        let source = r#"
            #[mutation]
            async fn create_user(ctx: MutationContext, name: String, email: String) -> Result<User> {
                todo!()
            }
        "#;

        let registry = SchemaRegistry::new();
        parse_file(source, &registry).expect("mutation function should parse");

        let func = registry
            .get_function("create_user")
            .expect("create_user function should be registered");
        assert_eq!(func.name, "create_user");
        assert_eq!(func.kind, FunctionKind::Mutation);
        assert_eq!(func.args.len(), 2);
        assert_eq!(
            func.args.first().expect("name arg should exist").name,
            "name"
        );
        assert_eq!(
            func.args.get(1).expect("email arg should exist").name,
            "email"
        );
    }

    #[test]
    fn test_context_detection_structural() {
        // A type ending with "Context" should be detected.
        let source = r#"
            #[query]
            async fn test(ctx: forge::QueryContext, id: Uuid) -> Result<User> {
                todo!()
            }
        "#;
        let registry = SchemaRegistry::new();
        parse_file(source, &registry).expect("context query should parse");
        let func = registry
            .get_function("test")
            .expect("test function should be registered");
        assert_eq!(func.args.len(), 1); // Only `id`, context was skipped.
        assert_eq!(func.args.first().expect("id arg should exist").name, "id");
    }

    #[test]
    fn test_context_detection_does_not_match_other_types() {
        // A type NOT ending with "Context" should not be skipped.
        let source = r#"
            #[query]
            async fn test(data: ContextManager, id: Uuid) -> Result<User> {
                todo!()
            }
        "#;
        let registry = SchemaRegistry::new();
        parse_file(source, &registry).expect("non-context query should parse");
        let func = registry
            .get_function("test")
            .expect("test function should be registered");
        // "ContextManager" ends with "Manager", not "Context", so both args kept.
        assert_eq!(func.args.len(), 2);
    }

    #[test]
    fn test_naive_time_maps_to_local_time() {
        let source = r#"
            #[derive(Serialize, Deserialize)]
            struct Schedule {
                start_time: NaiveTime,
            }
        "#;
        let registry = SchemaRegistry::new();
        parse_file(source, &registry).expect("schedule DTO should parse");
        let table = registry
            .get_table("Schedule")
            .expect("Schedule table should be registered");
        assert_eq!(
            table
                .fields
                .first()
                .expect("start_time field should exist")
                .rust_type,
            RustType::LocalTime
        );
    }
}
