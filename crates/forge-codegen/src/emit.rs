//! Single source of truth for type mapping and code generation utilities.
//!
//! Every type conversion — RustType to TypeScript, RustType to Dioxus Rust —
//! lives here. Generators call these functions instead of implementing their own.
//! This makes it impossible for type mappings to diverge between generators.

use forge_core::schema::RustType;

/// Position context for type mapping.
/// Some types map differently depending on usage context.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Position {
    /// Function argument or struct field.
    Arg,
    /// Function return type.
    Return,
}

// ---------------------------------------------------------------------------
// TypeScript type mapping
// ---------------------------------------------------------------------------

/// Convert a RustType to its TypeScript representation.
pub fn ts_type(rust_type: &RustType, pos: Position) -> String {
    match rust_type {
        RustType::String
        | RustType::Uuid
        | RustType::Instant
        | RustType::LocalDate
        | RustType::LocalTime => "string".into(),

        RustType::I32 | RustType::I64 | RustType::F32 | RustType::F64 => "number".into(),

        RustType::Bool => "boolean".into(),
        RustType::Json => "unknown".into(),
        RustType::Upload => "File | Blob".into(),

        RustType::Bytes => match pos {
            Position::Return => "Blob".into(),
            Position::Arg => "Uint8Array".into(),
        },

        RustType::Option(inner) => format!("{} | null", ts_type(inner, pos)),
        RustType::Vec(inner) => format!("{}[]", ts_type(inner, pos)),

        RustType::Custom(name) => ts_custom(name, pos),
    }
}

fn ts_custom(name: &str, pos: Position) -> String {
    match name {
        "()" => "void".into(),
        "Upload" => "File | Blob".into(),
        "Bytes" => match pos {
            Position::Return => "Blob".into(),
            Position::Arg => "Uint8Array".into(),
        },

        // Unparsed generic types that leaked through as Custom.
        _ if name.starts_with("Vec<") => {
            let inner = name
                .strip_prefix("Vec<")
                .and_then(|s| s.strip_suffix('>'))
                .unwrap_or("unknown");
            format!("{}[]", ts_type(&RustType::Custom(inner.to_string()), pos))
        }

        _ if name.starts_with("HashMap<") || name.starts_with("std::collections::HashMap<") => {
            ts_hashmap(name)
        }

        _ => name.to_string(),
    }
}

fn ts_hashmap(name: &str) -> String {
    let inner = name
        .strip_prefix("HashMap<")
        .or_else(|| name.strip_prefix("std::collections::HashMap<"))
        .and_then(|s| s.strip_suffix('>'));

    let Some(inner) = inner else {
        return "Record<string, unknown>".into();
    };

    let mut parts = inner.splitn(2, ',').map(|s| s.trim());
    let _key_type = parts.next();
    if let Some(value) = parts.next() {
        let value_type = match value {
            "String" | "&str" | "str" => "string",
            "i32" | "i64" | "u32" | "u64" | "f32" | "f64" => "number",
            "bool" => "boolean",
            other => other,
        };
        format!("Record<string, {}>", value_type)
    } else {
        "Record<string, unknown>".into()
    }
}

// ---------------------------------------------------------------------------
// Dioxus (Rust frontend) type mapping
// ---------------------------------------------------------------------------

/// Convert a RustType to its Dioxus/Rust representation for generated frontend code.
pub fn dioxus_type(rust_type: &RustType) -> String {
    match rust_type {
        RustType::String | RustType::Uuid => "String".into(),
        RustType::I32 => "i32".into(),
        RustType::I64 => "i64".into(),
        RustType::F32 => "f32".into(),
        RustType::F64 => "f64".into(),
        RustType::Bool => "bool".into(),
        RustType::Instant | RustType::LocalDate | RustType::LocalTime => "String".into(),
        RustType::Upload => "ForgeUpload".into(),
        RustType::Json => "JsonValue".into(),
        RustType::Bytes => "Vec<u8>".into(),
        RustType::Option(inner) => format!("Option<{}>", dioxus_type(inner)),
        RustType::Vec(inner) => format!("Vec<{}>", dioxus_type(inner)),
        RustType::Custom(name) => dioxus_custom(name),
    }
}

fn dioxus_custom(name: &str) -> String {
    match name {
        "Uuid" | "uuid::Uuid" => "String".into(),
        "DateTime<Utc>" | "NaiveDate" | "NaiveDateTime" | "Instant" | "LocalDate" | "LocalTime"
        | "Timestamp" => "String".into(),
        "Value" | "serde_json::Value" => "JsonValue".into(),
        "Bytes" => "Vec<u8>".into(),
        "Upload" => "ForgeUpload".into(),
        other => other.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Recursive type tree queries
// ---------------------------------------------------------------------------

/// Recursively walk a RustType tree, returning true if the predicate matches any node.
fn walk_type(rust_type: &RustType, predicate: &dyn Fn(&RustType) -> bool) -> bool {
    if predicate(rust_type) {
        return true;
    }
    match rust_type {
        RustType::Option(inner) | RustType::Vec(inner) => walk_type(inner, predicate),
        _ => false,
    }
}

/// Check if a RustType tree contains an Upload type anywhere.
pub fn contains_upload(rust_type: &RustType) -> bool {
    walk_type(rust_type, &|t| {
        matches!(t, RustType::Upload) || matches!(t, RustType::Custom(n) if n == "Upload")
    })
}

/// Check if a RustType tree contains a Json type anywhere.
pub fn contains_json(rust_type: &RustType) -> bool {
    walk_type(rust_type, &|t| {
        matches!(t, RustType::Json)
            || matches!(t, RustType::Custom(n) if n == "Value" || n == "serde_json::Value")
    })
}

/// Collect custom type names that need to be imported in TypeScript.
pub fn collect_type_imports(rust_type: &RustType, imports: &mut Vec<String>) {
    match rust_type {
        RustType::Custom(name) if is_importable_type(name) => {
            imports.push(name.clone());
        }
        RustType::Option(inner) | RustType::Vec(inner) => collect_type_imports(inner, imports),
        _ => {}
    }
}

/// A custom type name is importable if it represents a user-defined type
/// (not a built-in, container, or type that maps to a primitive).
fn is_importable_type(name: &str) -> bool {
    !matches!(
        name,
        "()" | "Upload" | "Bytes" | "Instant" | "LocalDate" | "LocalTime"
    ) && !name.starts_with("Vec<")
        && !name.starts_with("HashMap<")
        && !name.starts_with("std::collections::")
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- TypeScript mapping --

    #[test]
    fn ts_primitives() {
        assert_eq!(ts_type(&RustType::String, Position::Arg), "string");
        assert_eq!(ts_type(&RustType::I32, Position::Arg), "number");
        assert_eq!(ts_type(&RustType::Bool, Position::Arg), "boolean");
        assert_eq!(ts_type(&RustType::Uuid, Position::Arg), "string");
        assert_eq!(ts_type(&RustType::Json, Position::Arg), "unknown");
    }

    #[test]
    fn ts_temporal_types() {
        for ty in [RustType::Instant, RustType::LocalDate, RustType::LocalTime] {
            assert_eq!(ts_type(&ty, Position::Arg), "string");
        }
    }

    #[test]
    fn ts_bytes_depends_on_position() {
        assert_eq!(ts_type(&RustType::Bytes, Position::Arg), "Uint8Array");
        assert_eq!(ts_type(&RustType::Bytes, Position::Return), "Blob");
    }

    #[test]
    fn ts_option_and_vec() {
        assert_eq!(
            ts_type(&RustType::Option(Box::new(RustType::String)), Position::Arg),
            "string | null"
        );
        assert_eq!(
            ts_type(&RustType::Vec(Box::new(RustType::I32)), Position::Arg),
            "number[]"
        );
    }

    #[test]
    fn ts_custom_types() {
        assert_eq!(
            ts_type(&RustType::Custom("()".into()), Position::Arg),
            "void"
        );
        assert_eq!(
            ts_type(&RustType::Custom("User".into()), Position::Arg),
            "User"
        );
        assert_eq!(
            ts_type(&RustType::Custom("Upload".into()), Position::Arg),
            "File | Blob"
        );
    }

    #[test]
    fn ts_hashmap() {
        assert_eq!(
            ts_type(
                &RustType::Custom("HashMap<String, i32>".into()),
                Position::Arg
            ),
            "Record<string, number>"
        );
        assert_eq!(
            ts_type(
                &RustType::Custom("HashMap<String, User>".into()),
                Position::Arg
            ),
            "Record<string, User>"
        );
    }

    // -- Dioxus mapping --

    #[test]
    fn dioxus_primitives() {
        assert_eq!(dioxus_type(&RustType::String), "String");
        assert_eq!(dioxus_type(&RustType::I32), "i32");
        assert_eq!(dioxus_type(&RustType::Bool), "bool");
        assert_eq!(dioxus_type(&RustType::Upload), "ForgeUpload");
        assert_eq!(dioxus_type(&RustType::Json), "JsonValue");
        assert_eq!(dioxus_type(&RustType::Bytes), "Vec<u8>");
    }

    #[test]
    fn dioxus_custom_known_aliases() {
        assert_eq!(dioxus_type(&RustType::Custom("Timestamp".into())), "String");
        assert_eq!(dioxus_type(&RustType::Custom("Value".into())), "JsonValue");
        assert_eq!(
            dioxus_type(&RustType::Custom("Upload".into())),
            "ForgeUpload"
        );
    }

    // -- Type tree queries --

    #[test]
    fn upload_detection() {
        assert!(contains_upload(&RustType::Upload));
        assert!(contains_upload(&RustType::Option(Box::new(
            RustType::Upload
        ))));
        assert!(contains_upload(&RustType::Vec(Box::new(RustType::Upload))));
        assert!(contains_upload(&RustType::Custom("Upload".into())));
        assert!(!contains_upload(&RustType::String));
        assert!(!contains_upload(&RustType::Custom("User".into())));
    }

    #[test]
    fn json_detection() {
        assert!(contains_json(&RustType::Json));
        assert!(contains_json(&RustType::Option(Box::new(RustType::Json))));
        assert!(contains_json(&RustType::Custom("Value".into())));
        assert!(!contains_json(&RustType::String));
    }

    #[test]
    fn type_imports() {
        let mut imports = Vec::new();
        collect_type_imports(&RustType::Custom("User".into()), &mut imports);
        collect_type_imports(&RustType::Custom("()".into()), &mut imports);
        collect_type_imports(&RustType::Custom("Upload".into()), &mut imports);
        collect_type_imports(
            &RustType::Vec(Box::new(RustType::Custom("Project".into()))),
            &mut imports,
        );
        assert_eq!(imports, vec!["User", "Project"]);
    }
}
