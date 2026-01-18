//! FORGE - The Rust Full-Stack Framework
//!
//! A batteries-included framework for building full-stack web applications
//! with Rust backend and Svelte 5 frontend.

mod runtime;

// Re-export forge_core for macro-generated code
#[doc(hidden)]
pub use forge_core;

// Re-export proc macros at crate root
pub use forge_macros::{cron, forge_enum, job, model, mutation, query, workflow};

// Re-export Migration type for programmatic migrations
pub use forge_runtime::migrations::Migration;

// Re-export testing utilities
pub use forge_core::testing;

/// All internal FORGE schema SQL concatenated.
///
/// For tests: apply before user migrations. In production, migration runner handles versioning.
pub fn get_internal_sql() -> String {
    forge_runtime::migrations::get_all_system_sql()
}

pub use runtime::prelude;
pub use runtime::{Forge, ForgeBuilder};
