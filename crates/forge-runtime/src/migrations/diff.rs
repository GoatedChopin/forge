use forge_core::schema::{FieldDef, TableDef};

/// Represents the difference between two schemas.
///
/// The diff algorithm compares the Rust schema (source of truth) against the
/// database schema (current state) to produce a set of changes. The comparison
/// is done at two levels:
///
/// 1. **Table level**: Find tables that exist in Rust but not DB (CREATE),
///    or exist in DB but not Rust (DROP, except forge_ internal tables).
///
/// 2. **Column level**: For tables in both, compare fields:
///    - Field in Rust but not DB → ADD COLUMN
///    - Field in DB but not Rust → DROP COLUMN
///    - Field in both but different type → ALTER COLUMN TYPE
///
/// The algorithm is intentionally simple and doesn't handle:
/// - Column renames (seen as DROP + ADD)
/// - Index changes (handled separately)
/// - Complex type migrations (require manual migration)
///
/// This is by design: automatic migrations are for development convenience.
/// Production deployments should use explicit migration files.
#[derive(Debug, Clone)]
pub struct SchemaDiff {
    /// Changes to be applied.
    pub entries: Vec<DiffEntry>,
}

impl SchemaDiff {
    /// Create an empty diff.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Compare a Rust schema to a database schema.
    pub fn from_comparison(rust_tables: &[TableDef], db_tables: &[DatabaseTable]) -> Self {
        let mut entries = Vec::new();

        // Find tables to add
        for rust_table in rust_tables {
            let db_table = db_tables.iter().find(|t| t.name == rust_table.name);

            match db_table {
                None => {
                    // Table doesn't exist, create it
                    // Note: Actual SQL should come from migrations, this is just for diff detection
                    entries.push(DiffEntry {
                        action: DiffAction::CreateTable,
                        table_name: rust_table.name.clone(),
                        details: format!("Create table {}", rust_table.name),
                        sql: format!("-- Create table {} (see migrations)", rust_table.name),
                    });
                }
                Some(db) => {
                    // Compare columns
                    for rust_field in &rust_table.fields {
                        let db_column =
                            db.columns.iter().find(|c| c.name == rust_field.column_name);

                        match db_column {
                            None => {
                                // Column doesn't exist, add it
                                entries.push(DiffEntry {
                                    action: DiffAction::AddColumn,
                                    table_name: rust_table.name.clone(),
                                    details: format!("Add column {}", rust_field.column_name),
                                    sql: Self::add_column_sql(&rust_table.name, rust_field),
                                });
                            }
                            Some(db_col) => {
                                // Check if column type changed
                                let rust_type = rust_field.sql_type.to_sql();
                                if db_col.data_type != rust_type {
                                    entries.push(DiffEntry {
                                        action: DiffAction::AlterColumn,
                                        table_name: rust_table.name.clone(),
                                        details: format!(
                                            "Change column {} type from {} to {}",
                                            rust_field.column_name, db_col.data_type, rust_type
                                        ),
                                        sql: format!(
                                            "ALTER TABLE {} ALTER COLUMN {} TYPE {};",
                                            rust_table.name, rust_field.column_name, rust_type
                                        ),
                                    });
                                }
                            }
                        }
                    }

                    // Find columns to drop (exist in DB but not in Rust)
                    for db_col in &db.columns {
                        let exists_in_rust = rust_table
                            .fields
                            .iter()
                            .any(|f| f.column_name == db_col.name);

                        if !exists_in_rust {
                            entries.push(DiffEntry {
                                action: DiffAction::DropColumn,
                                table_name: rust_table.name.clone(),
                                details: format!("Drop column {}", db_col.name),
                                sql: format!(
                                    "ALTER TABLE {} DROP COLUMN {};",
                                    rust_table.name, db_col.name
                                ),
                            });
                        }
                    }
                }
            }
        }

        // Find tables to drop (exist in DB but not in Rust)
        for db_table in db_tables {
            let exists_in_rust = rust_tables.iter().any(|t| t.name == db_table.name);

            if !exists_in_rust && !db_table.name.starts_with("forge_") {
                entries.push(DiffEntry {
                    action: DiffAction::DropTable,
                    table_name: db_table.name.clone(),
                    details: format!("Drop table {}", db_table.name),
                    sql: format!("DROP TABLE {};", db_table.name),
                });
            }
        }

        Self { entries }
    }

    fn add_column_sql(table_name: &str, field: &FieldDef) -> String {
        let mut sql = format!(
            "ALTER TABLE {} ADD COLUMN {} {}",
            table_name,
            field.column_name,
            field.sql_type.to_sql()
        );

        if !field.nullable {
            // For non-nullable columns, provide a sensible default
            let default_val = match field.sql_type {
                forge_core::schema::SqlType::Varchar(_) | forge_core::schema::SqlType::Text => "''",
                forge_core::schema::SqlType::Integer | forge_core::schema::SqlType::BigInt => "0",
                forge_core::schema::SqlType::Boolean => "false",
                forge_core::schema::SqlType::Timestamptz => "NOW()",
                _ => "NULL",
            };
            sql.push_str(&format!(" NOT NULL DEFAULT {}", default_val));
        }

        sql.push(';');
        sql
    }

    /// Check if there are any changes.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Get all SQL statements.
    pub fn to_sql(&self) -> Vec<String> {
        self.entries.iter().map(|e| e.sql.clone()).collect()
    }
}

impl Default for SchemaDiff {
    fn default() -> Self {
        Self::new()
    }
}

/// A single diff entry.
#[derive(Debug, Clone)]
pub struct DiffEntry {
    /// Type of action.
    pub action: DiffAction,
    /// Affected table name.
    pub table_name: String,
    /// Human-readable description.
    pub details: String,
    /// SQL to apply.
    pub sql: String,
}

/// Type of schema change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffAction {
    CreateTable,
    DropTable,
    AddColumn,
    DropColumn,
    AlterColumn,
    AddIndex,
    DropIndex,
    CreateEnum,
    AlterEnum,
}

/// Representation of a database table (from introspection).
#[derive(Debug, Clone)]
pub struct DatabaseTable {
    pub name: String,
    pub columns: Vec<DatabaseColumn>,
}

/// Representation of a database column (from introspection).
#[derive(Debug, Clone)]
pub struct DatabaseColumn {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub default: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_core::schema::RustType;
    use forge_core::schema::{FieldDef, TableDef};

    #[test]
    fn test_empty_diff() {
        let diff = SchemaDiff::new();
        assert!(diff.is_empty());
    }

    #[test]
    fn test_create_table_diff() {
        let mut table = TableDef::new("users", "User");
        table.fields.push(FieldDef::new("id", RustType::Uuid));

        let diff = SchemaDiff::from_comparison(&[table], &[]);

        assert_eq!(diff.entries.len(), 1);
        assert_eq!(diff.entries[0].action, DiffAction::CreateTable);
    }

    #[test]
    fn test_add_column_diff() {
        let mut rust_table = TableDef::new("users", "User");
        rust_table.fields.push(FieldDef::new("id", RustType::Uuid));
        rust_table
            .fields
            .push(FieldDef::new("email", RustType::String));

        let db_table = DatabaseTable {
            name: "users".to_string(),
            columns: vec![DatabaseColumn {
                name: "id".to_string(),
                data_type: "UUID".to_string(),
                nullable: false,
                default: None,
            }],
        };

        let diff = SchemaDiff::from_comparison(&[rust_table], &[db_table]);

        assert_eq!(diff.entries.len(), 1);
        assert_eq!(diff.entries[0].action, DiffAction::AddColumn);
        assert!(diff.entries[0].details.contains("email"));
    }
}
