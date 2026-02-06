//! Schema definition and registration system.
//!
//! This module provides types for defining database schemas declaratively in Rust.
//! Schemas are registered at compile time via proc macros and used for:
//!
//! - TypeScript client generation
//! - Migration diffing
//! - Runtime introspection
//!
//! # Key Types
//!
//! - [`TableDef`] - Table schema with fields and indexes
//! - [`FieldDef`] - Column definition with Rust and SQL types
//! - [`FunctionDef`] - Query/mutation signatures for codegen
//! - [`SchemaRegistry`] - Global registry populated by macros
//!
//! # Example
//!
//! ```ignore
//! #[derive(sqlx::FromRow, Serialize, Deserialize)]
//! #[forge::model]
//! pub struct User {
//!     pub id: Uuid,
//!     pub email: String,
//!     pub created_at: DateTime<Utc>,
//! }
//! ```

mod field;
mod function;
mod model;
mod registry;
mod types;

pub use field::FieldDef;
pub use function::{FunctionArg, FunctionDef, FunctionKind};
pub use model::{IndexDef, ModelMeta, RelationType, TableDef};
pub use registry::{EnumDef, EnumVariant, SchemaRegistry};
pub use types::{RustType, SqlType};
