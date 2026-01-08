use serde::{Deserialize, Serialize};

use super::types::{RustType, SqlType};

/// Definition of a model field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDef {
    /// Field name in Rust (snake_case).
    pub name: String,

    /// Column name in SQL (may differ from field name).
    pub column_name: String,

    /// Rust type.
    pub rust_type: RustType,

    /// SQL type.
    pub sql_type: SqlType,

    /// Whether the field is nullable.
    pub nullable: bool,

    /// Documentation comment.
    pub doc: Option<String>,
}

impl FieldDef {
    /// Create a new field definition.
    pub fn new(name: &str, rust_type: RustType) -> Self {
        let sql_type = rust_type.to_sql_type();
        let nullable = rust_type.is_nullable();
        let column_name = to_snake_case(name);

        Self {
            name: name.to_string(),
            column_name,
            rust_type,
            sql_type,
            nullable,
            doc: None,
        }
    }

    /// Generate TypeScript field.
    pub fn to_typescript(&self) -> String {
        let ts_type = self.rust_type.to_typescript();
        let optional = if self.nullable { "?" } else { "" };
        format!("  {}{}: {};", to_camel_case(&self.name), optional, ts_type)
    }
}

/// Convert a string to snake_case.
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

/// Convert a string to camelCase.
fn to_camel_case(s: &str) -> String {
    let mut result = String::new();
    let mut capitalize_next = false;
    for c in s.chars() {
        if c == '_' {
            capitalize_next = true;
        } else if capitalize_next {
            result.push(c.to_uppercase().next().unwrap());
            capitalize_next = false;
        } else {
            result.push(c);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_field_def_basic() {
        let field = FieldDef::new("email", RustType::String);
        assert_eq!(field.name, "email");
        assert_eq!(field.column_name, "email");
        assert!(!field.nullable);
    }

    #[test]
    fn test_field_def_nullable() {
        let field = FieldDef::new("avatar_url", RustType::Option(Box::new(RustType::String)));
        assert!(field.nullable);
    }

    #[test]
    fn test_to_snake_case() {
        assert_eq!(to_snake_case("createdAt"), "created_at");
        assert_eq!(to_snake_case("userId"), "user_id");
        assert_eq!(to_snake_case("HTTPServer"), "h_t_t_p_server");
    }

    #[test]
    fn test_to_camel_case() {
        assert_eq!(to_camel_case("created_at"), "createdAt");
        assert_eq!(to_camel_case("user_id"), "userId");
    }
}
