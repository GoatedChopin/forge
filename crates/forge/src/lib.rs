//! FORGE - The Rust Full-Stack Framework
//!
//! A batteries-included framework for building full-stack web applications
//! with a Rust backend and generated SvelteKit or Dioxus frontends.

mod runtime;

// Re-export forge_core for macro-generated code
#[doc(hidden)]
pub use forge_core;

// Re-export proc macros at crate root
pub use forge_macros::{
    cron, daemon, forge_enum, job, mcp_tool, model, mutation, query, webhook, workflow,
};

// Re-export Migration type for programmatic migrations
pub use forge_runtime::migrations::Migration;

// Re-export testing utilities
pub use forge_core::testing;

// Re-export testing assertion macros
pub use forge_core::{
    assert_err, assert_err_variant, assert_http_called, assert_http_not_called,
    assert_job_dispatched, assert_job_not_dispatched, assert_ok, assert_workflow_not_started,
    assert_workflow_started,
};

/// All internal FORGE schema SQL concatenated.
///
/// For tests: apply before user migrations. In production, migration runner handles versioning.
pub fn get_internal_sql() -> String {
    forge_runtime::migrations::get_all_system_sql()
}

pub use runtime::prelude;
pub use runtime::{Forge, ForgeBuilder};
