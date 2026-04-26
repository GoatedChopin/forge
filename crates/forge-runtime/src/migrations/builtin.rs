//! Built-in FORGE schema migrations.
//!
//! These migrations create all internal tables required by the FORGE runtime.
//! They use a version-based naming scheme (`__forge_vXXX`) to avoid conflicts
//! with user migrations.
//!
//! # Migration Naming
//!
//! - System migrations: `__forge_v001` (single migration, pre-1.0)
//! - User migrations: `0001_xxx`, `0002_xxx`, etc.
//!
//! System migrations are always applied before user migrations, regardless of
//! naming. This allows new forge features to be added without conflicting with
//! existing user migration numbering.

use super::runner::Migration;

/// System migration prefix. All forge internal migrations use this prefix.
pub const SYSTEM_MIGRATION_PREFIX: &str = "__forge_v";

/// System migration v001: Initial FORGE schema.
/// Creates all core tables for jobs, workflows, crons, observability, daemons, webhooks, etc.
const V001_INITIAL: &str = include_str!("../../migrations/system/v001_initial.sql");

/// System migration v002: Add owner_subject to forge_cron_runs.
/// Mirrors forge_jobs.owner_subject for per-tenant audit trails.
const V002_CRON_OWNER_SUBJECT: &str =
    include_str!("../../migrations/system/v002_cron_owner_subject.sql");

/// System migration v003: Add token_family column for refresh token chain reuse detection.
const V003_TOKEN_FAMILIES: &str = include_str!("../../migrations/system/v003_token_families.sql");

/// System migration v004: Add compensation_state column to forge_workflow_runs.
const V004_WORKFLOW_COMPENSATION_STATE: &str =
    include_str!("../../migrations/system/v004_workflow_compensation_state.sql");

/// A system migration with a version number.
#[derive(Debug, Clone)]
pub struct SystemMigration {
    /// Version number (1, 2, 3, ...)
    pub version: u32,
    /// The SQL to execute
    pub sql: &'static str,
    /// Description of what this migration does
    pub description: &'static str,
}

impl SystemMigration {
    /// Get the migration name used in the database (e.g., `__forge_v001`).
    pub fn name(&self) -> String {
        format!("{}{:03}", SYSTEM_MIGRATION_PREFIX, self.version)
    }

    /// Convert to a Migration struct.
    pub fn to_migration(&self) -> Migration {
        Migration::new(self.name(), self.sql)
    }
}

/// Get all built-in FORGE system migrations in version order.
///
/// These are applied in order before any user migrations.
pub fn get_system_migrations() -> Vec<SystemMigration> {
    vec![
        SystemMigration {
            version: 1,
            sql: V001_INITIAL,
            description: "Initial FORGE schema with jobs, workflows, crons, daemons, webhooks, and auth",
        },
        SystemMigration {
            version: 2,
            sql: V002_CRON_OWNER_SUBJECT,
            description: "Add owner_subject to forge_cron_runs for per-tenant audit trails",
        },
        SystemMigration {
            version: 3,
            sql: V003_TOKEN_FAMILIES,
            description: "Add token_family to forge_refresh_tokens for chain reuse detection",
        },
        SystemMigration {
            version: 4,
            sql: V004_WORKFLOW_COMPENSATION_STATE,
            description: "Add compensation_state to forge_workflow_runs for crash-safe saga compensation",
        },
    ]
}

/// Get system migrations as Migration structs.
pub fn get_builtin_migrations() -> Vec<Migration> {
    get_system_migrations()
        .into_iter()
        .map(|m| m.to_migration())
        .collect()
}

/// Get all system migrations SQL concatenated.
///
/// Use for test setup before running user migrations.
/// In production, use [`get_builtin_migrations`] for versioned application.
pub fn get_all_system_sql() -> String {
    get_system_migrations()
        .into_iter()
        .map(|m| m.sql)
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Check if a migration name is a system migration.
pub fn is_system_migration(name: &str) -> bool {
    name.starts_with(SYSTEM_MIGRATION_PREFIX)
}

/// Extract version number from a system migration name.
/// Returns None if not a valid system migration name.
pub fn extract_version(name: &str) -> Option<u32> {
    name.strip_prefix(SYSTEM_MIGRATION_PREFIX)
        .and_then(|suffix| suffix.parse().ok())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_get_system_migrations() {
        let migrations = get_system_migrations();
        assert!(!migrations.is_empty());
        assert_eq!(migrations[0].version, 1);
        assert_eq!(migrations[0].name(), "__forge_v001");
        assert_eq!(migrations[1].version, 2);
        assert_eq!(migrations[1].name(), "__forge_v002");
    }

    #[test]
    fn test_migration_sql_not_empty() {
        let migrations = get_system_migrations();
        for m in migrations {
            assert!(!m.sql.is_empty(), "Migration v{} has empty SQL", m.version);
        }
    }

    #[test]
    fn test_migration_sql_contains_tables() {
        let migrations = get_system_migrations();
        let sql = migrations[0].sql;

        // Verify all core tables are defined
        assert!(sql.contains("forge_nodes"));
        assert!(sql.contains("forge_leaders"));
        assert!(sql.contains("forge_jobs"));
        assert!(sql.contains("forge_cron_runs"));
        assert!(sql.contains("forge_workflow_runs"));
        assert!(sql.contains("forge_workflow_steps"));
        assert!(sql.contains("forge_sessions"));
        assert!(sql.contains("forge_subscriptions"));
        assert!(sql.contains("forge_daemons"));
        assert!(sql.contains("forge_webhook_events"));
        assert!(sql.contains("forge_refresh_tokens"));
        assert!(sql.contains("forge_oauth_clients"));
        assert!(sql.contains("forge_oauth_codes"));

        // Signals tables
        assert!(sql.contains("forge_signals_events"));
        assert!(sql.contains("forge_signals_sessions"));
        assert!(sql.contains("forge_signals_users"));
    }

    #[test]
    fn test_is_system_migration() {
        assert!(is_system_migration("__forge_v001"));
        assert!(is_system_migration("__forge_v002"));
        assert!(is_system_migration("__forge_v100"));
        assert!(!is_system_migration("0001_create_users"));
        assert!(!is_system_migration("user_migration"));
    }

    #[test]
    fn test_extract_version() {
        assert_eq!(extract_version("__forge_v001"), Some(1));
        assert_eq!(extract_version("__forge_v002"), Some(2));
        assert_eq!(extract_version("__forge_v100"), Some(100));
        assert_eq!(extract_version("0001_create_users"), None);
        assert_eq!(extract_version("invalid"), None);
    }

    #[test]
    fn test_system_migrations_version_ordering() {
        let migrations = get_system_migrations();
        for window in migrations.windows(2) {
            assert!(
                window[0].version < window[1].version,
                "Migrations must be in ascending version order: v{} >= v{}",
                window[0].version,
                window[1].version,
            );
        }
    }

    #[test]
    fn test_system_migration_to_migration() {
        let sys = SystemMigration {
            version: 1,
            sql: "SELECT 1;",
            description: "Test",
        };
        let m = sys.to_migration();
        assert_eq!(m.name, "__forge_v001");
        assert_eq!(m.up_sql, "SELECT 1;");
    }
}
