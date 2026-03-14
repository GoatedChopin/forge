//! FORGE binding generation system.
//!
//! Generates type-safe frontend bindings from Rust source code.
//! Supports two targets: SvelteKit (TypeScript) and Dioxus (Rust).
//!
//! # Architecture
//!
//! The system has three layers:
//!
//! 1. **Parser** (`parser.rs`) — Extracts schema definitions from Rust AST.
//! 2. **Binding IR** (`binding.rs`) — Pre-computes all function binding facts once.
//! 3. **Emitters** (`emit.rs`, `typescript/`, `dioxus/`) — Generate target code
//!    using shared type mapping functions, eliminating duplication.
//!
//! The key design principle: every type mapping and utility function exists in
//! exactly one place (`emit.rs`), so edge cases can't diverge between targets.

mod binding;
pub mod dioxus;
mod emit;
pub mod parser;
pub mod typescript;

pub use dioxus::DioxusGenerator;
pub use parser::parse_project;
pub use typescript::{Error, GenerateOptions, RUNES_SVELTE_TS, TypeScriptGenerator};
