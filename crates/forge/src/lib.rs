//! FORGE - The Rust Full-Stack Framework
//!
//! A batteries-included framework for building full-stack web applications
//! with Rust backend and Svelte 5 frontend.

mod runtime;

// Re-export forge_core for macro-generated code
#[doc(hidden)]
pub use forge_core;

// Re-export proc macros at crate root
pub use forge_macros::{action, cron, forge_enum, job, model, mutation, query, workflow};

// Re-export Migration type for programmatic migrations
pub use forge_runtime::migrations::Migration;

// Re-export testing utilities
pub use forge_core::testing;

/// Internal FORGE schema SQL.
///
/// This SQL creates all forge internal tables and functions required by
/// user migrations that use forge features like `forge_enable_reactivity`.
///
/// # Example
///
/// ```ignore
/// use forge::testing::TestDatabase;
/// use forge::FORGE_INTERNAL_SQL;
///
/// let base = TestDatabase::embedded().await?;
/// let db = base.isolated("my_test").await?;
///
/// // Run forge internals first (needed for forge_enable_reactivity, etc.)
/// db.run_sql(FORGE_INTERNAL_SQL).await?;
///
/// // Then run your migrations
/// db.migrate(Path::new("migrations")).await?;
/// ```
pub const FORGE_INTERNAL_SQL: &str =
    include_str!("../../forge-runtime/migrations/system/v001_initial.sql");

pub use runtime::prelude;
pub use runtime::{Forge, ForgeBuilder};
