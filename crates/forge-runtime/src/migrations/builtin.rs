//! Built-in FORGE schema migrations.
//!
//! These migrations create all internal tables required by the FORGE runtime.
//! They use a version-based naming scheme (`__forge_vXXX`) to avoid conflicts
//! with user migrations.
//!
//! # Migration Naming
//!
//! - System migrations: `__forge_v001`, `__forge_v002`, etc.
//! - User migrations: `0001_xxx`, `0002_xxx`, etc.
//!
//! System migrations are always applied before user migrations, regardless of
//! naming. This allows new forge features to be added without conflicting with
//! existing user migration numbering.

use super::runner::Migration;

/// System migration prefix. All forge internal migrations use this prefix.
pub const SYSTEM_MIGRATION_PREFIX: &str = "__forge_v";

/// Legacy migration name from older forge versions.
/// If found, it will be considered equivalent to v001.
pub const LEGACY_MIGRATION_NAME: &str = "0000_forge_internal";

/// System migration v001: Initial FORGE schema.
/// Creates all core tables for jobs, workflows, crons, observability, etc.
const V001_INITIAL: &str = include_str!("../../migrations/system/v001_initial.sql");

/// System migration v002: Daemons and webhooks.
/// Adds tables for daemon tracking and webhook idempotency.
const V002_DAEMON_WEBHOOK: &str = include_str!("../../migrations/system/v002_daemon_webhook.sql");

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
            description: "Initial FORGE schema with jobs, workflows, crons, and observability",
        },
        SystemMigration {
            version: 2,
            sql: V002_DAEMON_WEBHOOK,
            description: "Daemon tracking and webhook idempotency tables",
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
    name.starts_with(SYSTEM_MIGRATION_PREFIX) || name == LEGACY_MIGRATION_NAME
}

/// Extract version number from a system migration name.
/// Returns None if not a valid system migration name.
pub fn extract_version(name: &str) -> Option<u32> {
    if name == LEGACY_MIGRATION_NAME {
        return Some(1);
    }
    if let Some(suffix) = name.strip_prefix(SYSTEM_MIGRATION_PREFIX) {
        return suffix.parse().ok();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_system_migrations() {
        let migrations = get_system_migrations();
        assert!(!migrations.is_empty());
        assert_eq!(migrations[0].version, 1);
        assert_eq!(migrations[0].name(), "__forge_v001");
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
        assert!(sql.contains("CREATE TABLE IF NOT EXISTS forge_nodes"));
        assert!(sql.contains("CREATE TABLE IF NOT EXISTS forge_leaders"));
        assert!(sql.contains("CREATE TABLE IF NOT EXISTS forge_jobs"));
        assert!(sql.contains("CREATE TABLE IF NOT EXISTS forge_cron_runs"));
        assert!(sql.contains("CREATE TABLE IF NOT EXISTS forge_workflow_runs"));
        assert!(sql.contains("CREATE TABLE IF NOT EXISTS forge_workflow_steps"));
        assert!(sql.contains("CREATE TABLE IF NOT EXISTS forge_metrics"));
        assert!(sql.contains("CREATE TABLE IF NOT EXISTS forge_logs"));
        assert!(sql.contains("CREATE TABLE IF NOT EXISTS forge_traces"));
        assert!(sql.contains("CREATE TABLE IF NOT EXISTS forge_sessions"));
        assert!(sql.contains("CREATE TABLE IF NOT EXISTS forge_subscriptions"));
        assert!(sql.contains("CREATE TABLE IF NOT EXISTS forge_alert_rules"));
        assert!(sql.contains("CREATE TABLE IF NOT EXISTS forge_alerts"));
    }

    #[test]
    fn test_is_system_migration() {
        assert!(is_system_migration("__forge_v001"));
        assert!(is_system_migration("__forge_v002"));
        assert!(is_system_migration("__forge_v100"));
        assert!(is_system_migration("0000_forge_internal")); // Legacy
        assert!(!is_system_migration("0001_create_users"));
        assert!(!is_system_migration("user_migration"));
    }

    #[test]
    fn test_extract_version() {
        assert_eq!(extract_version("__forge_v001"), Some(1));
        assert_eq!(extract_version("__forge_v002"), Some(2));
        assert_eq!(extract_version("__forge_v100"), Some(100));
        assert_eq!(extract_version("0000_forge_internal"), Some(1)); // Legacy
        assert_eq!(extract_version("0001_create_users"), None);
        assert_eq!(extract_version("invalid"), None);
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
