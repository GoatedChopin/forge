//! Migration runner with mesh-safe locking.
//!
//! Ensures only one node runs migrations at a time using PostgreSQL advisory locks.
//!
//! # Migration Types
//!
//! This runner handles two types of migrations:
//!
//! 1. **System migrations** (`__forge_vXXX`): Internal FORGE schema changes.
//!    These are versioned numerically and always run before user migrations.
//!
//! 2. **User migrations** (`XXXX_name.sql`): Application-specific schema changes.
//!    These are sorted alphabetically by name.

use forge_core::error::{ForgeError, Result};
use sqlx::{PgPool, Postgres};
use std::collections::HashSet;
use std::path::Path;
use tracing::{debug, info, warn};

use super::builtin::extract_version;

/// Lock ID for migration advisory lock (arbitrary but consistent).
/// Using a fixed value derived from "FORGE" ascii values.
const MIGRATION_LOCK_ID: i64 = 0x464F524745; // "FORGE" in hex

/// A single migration with up and optional down SQL.
#[derive(Debug, Clone)]
pub struct Migration {
    /// Unique name/identifier (e.g., "0001_forge_internal" or "0002_create_users").
    pub name: String,
    /// SQL to execute for upgrade (forward migration).
    pub up_sql: String,
    /// SQL to execute for rollback (optional).
    pub down_sql: Option<String>,
}

impl Migration {
    /// Create a migration with only up SQL (no rollback).
    pub fn new(name: impl Into<String>, sql: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            up_sql: sql.into(),
            down_sql: None,
        }
    }

    /// Create a migration with both up and down SQL.
    pub fn with_down(
        name: impl Into<String>,
        up_sql: impl Into<String>,
        down_sql: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            up_sql: up_sql.into(),
            down_sql: Some(down_sql.into()),
        }
    }

    /// Parse migration content that may contain -- @up and -- @down markers.
    pub fn parse(name: impl Into<String>, content: &str) -> Self {
        let name = name.into();
        let (up_sql, down_sql) = parse_migration_content(content);
        Self {
            name,
            up_sql,
            down_sql,
        }
    }
}

/// Parse migration content, splitting on -- @down marker.
/// Returns (up_sql, Option<down_sql>).
fn parse_migration_content(content: &str) -> (String, Option<String>) {
    // Look for -- @down marker (case insensitive, with optional whitespace)
    let down_marker_patterns = ["-- @down", "--@down", "-- @DOWN", "--@DOWN"];

    for pattern in down_marker_patterns {
        if let Some(idx) = content.find(pattern) {
            let up_part = &content[..idx];
            let down_part = &content[idx + pattern.len()..];

            // Clean up the up part (remove -- @up marker if present)
            let up_sql = up_part
                .replace("-- @up", "")
                .replace("--@up", "")
                .replace("-- @UP", "")
                .replace("--@UP", "")
                .trim()
                .to_string();

            let down_sql = down_part.trim().to_string();

            if down_sql.is_empty() {
                return (up_sql, None);
            }
            return (up_sql, Some(down_sql));
        }
    }

    // No @down marker found - treat entire content as up SQL
    let up_sql = content
        .replace("-- @up", "")
        .replace("--@up", "")
        .replace("-- @UP", "")
        .replace("--@UP", "")
        .trim()
        .to_string();

    (up_sql, None)
}

/// Migration runner that handles both built-in and user migrations.
pub struct MigrationRunner {
    pool: PgPool,
}

impl MigrationRunner {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Run all pending migrations with mesh-safe locking.
    ///
    /// This acquires an exclusive advisory lock before running migrations,
    /// ensuring only one node in the cluster runs migrations at a time.
    pub async fn run(&self, user_migrations: Vec<Migration>) -> Result<()> {
        // Acquire exclusive lock (blocks until acquired) on a dedicated connection.
        let mut lock_conn = self.acquire_lock_connection().await?;

        let result = self.run_migrations_inner(user_migrations).await;

        // Always release lock, even on error
        if let Err(e) = self.release_lock_connection(&mut lock_conn).await {
            warn!("Failed to release migration lock: {}", e);
        }

        result
    }

    async fn run_migrations_inner(&self, user_migrations: Vec<Migration>) -> Result<()> {
        // Ensure migration tracking table exists
        self.ensure_migrations_table().await?;

        // Get already-applied migrations
        let applied = self.get_applied_migrations().await?;
        debug!("Already applied migrations: {:?}", applied);

        // Calculate the highest system version already applied
        let max_applied_version = self.get_max_system_version(&applied);
        debug!("Max applied system version: {:?}", max_applied_version);

        // Run built-in FORGE system migrations first (in version order)
        let system_migrations = super::builtin::get_system_migrations();
        for sys_migration in system_migrations {
            // Skip if this version is already applied
            if let Some(max_ver) = max_applied_version
                && sys_migration.version <= max_ver
            {
                debug!(
                    "Skipping system migration v{} (already at v{})",
                    sys_migration.version, max_ver
                );
                continue;
            }

            let migration = sys_migration.to_migration();
            info!(
                "Applying system migration: {} ({})",
                migration.name, sys_migration.description
            );
            self.apply_migration(&migration).await?;
        }

        // Then run user migrations (sorted by name)
        for migration in user_migrations {
            if !applied.contains(&migration.name) {
                self.apply_migration(&migration).await?;
            }
        }

        Ok(())
    }

    /// Get the maximum system migration version that has been applied.
    fn get_max_system_version(&self, applied: &HashSet<String>) -> Option<u32> {
        applied
            .iter()
            .filter_map(|name| extract_version(name))
            .max()
    }

    async fn acquire_lock_connection(&self) -> Result<sqlx::pool::PoolConnection<Postgres>> {
        debug!("Acquiring migration lock...");
        let mut conn = self.pool.acquire().await.map_err(|e| {
            ForgeError::Database(format!("Failed to acquire lock connection: {}", e))
        })?;

        sqlx::query_scalar!("SELECT pg_advisory_lock($1)", MIGRATION_LOCK_ID)
            .fetch_one(&mut *conn)
            .await
            .map_err(|e| {
                ForgeError::Database(format!("Failed to acquire migration lock: {}", e))
            })?;
        debug!("Migration lock acquired");
        Ok(conn)
    }

    async fn release_lock_connection(
        &self,
        conn: &mut sqlx::pool::PoolConnection<Postgres>,
    ) -> Result<()> {
        sqlx::query_scalar!("SELECT pg_advisory_unlock($1)", MIGRATION_LOCK_ID)
            .fetch_one(&mut **conn)
            .await
            .map_err(|e| {
                ForgeError::Database(format!("Failed to release migration lock: {}", e))
            })?;
        debug!("Migration lock released");
        Ok(())
    }

    async fn ensure_migrations_table(&self) -> Result<()> {
        // Create table if not exists
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS forge_migrations (
                id SERIAL PRIMARY KEY,
                name VARCHAR(255) UNIQUE NOT NULL,
                applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                down_sql TEXT
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| ForgeError::Database(format!("Failed to create migrations table: {}", e)))?;

        Ok(())
    }

    async fn get_applied_migrations(&self) -> Result<HashSet<String>> {
        let rows = sqlx::query!("SELECT name FROM forge_migrations")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| {
                ForgeError::Database(format!("Failed to get applied migrations: {}", e))
            })?;

        Ok(rows.into_iter().map(|row| row.name).collect())
    }

    async fn apply_migration(&self, migration: &Migration) -> Result<()> {
        info!("Applying migration: {}", migration.name);

        // Split migration into individual statements, respecting dollar-quoted strings
        let statements = split_sql_statements(&migration.up_sql);

        for statement in statements {
            let statement = statement.trim();

            // Skip empty statements or comment-only blocks
            if statement.is_empty()
                || statement.lines().all(|l| {
                    let l = l.trim();
                    l.is_empty() || l.starts_with("--")
                })
            {
                continue;
            }

            sqlx::query(statement)
                .execute(&self.pool)
                .await
                .map_err(|e| {
                    ForgeError::Database(format!(
                        "Failed to apply migration '{}': {}",
                        migration.name, e
                    ))
                })?;
        }

        // Record it as applied (with down_sql for potential rollback)
        sqlx::query!(
            "INSERT INTO forge_migrations (name, down_sql) VALUES ($1, $2)",
            &migration.name,
            migration.down_sql as _,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| {
            ForgeError::Database(format!(
                "Failed to record migration '{}': {}",
                migration.name, e
            ))
        })?;

        info!("Migration applied: {}", migration.name);
        Ok(())
    }

    /// Rollback N migrations (most recent first).
    pub async fn rollback(&self, count: usize) -> Result<Vec<String>> {
        if count == 0 {
            return Ok(Vec::new());
        }

        // Acquire exclusive lock on a dedicated connection.
        let mut lock_conn = self.acquire_lock_connection().await?;

        let result = self.rollback_inner(count).await;

        // Always release lock
        if let Err(e) = self.release_lock_connection(&mut lock_conn).await {
            warn!("Failed to release migration lock: {}", e);
        }

        result
    }

    async fn rollback_inner(&self, count: usize) -> Result<Vec<String>> {
        self.ensure_migrations_table().await?;

        // Get the N most recent migrations with their down_sql
        let rows = sqlx::query!(
            "SELECT id, name, down_sql FROM forge_migrations ORDER BY id DESC LIMIT $1",
            count as i32
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ForgeError::Database(format!("Failed to get migrations: {}", e)))?;

        if rows.is_empty() {
            info!("No migrations to rollback");
            return Ok(Vec::new());
        }

        let mut rolled_back = Vec::new();

        for row in rows {
            let id = row.id;
            let name = row.name;
            let down_sql = row.down_sql;
            info!("Rolling back migration: {}", name);

            if let Some(down) = down_sql {
                // Execute down SQL
                let statements = split_sql_statements(&down);
                for statement in statements {
                    let statement = statement.trim();
                    if statement.is_empty()
                        || statement.lines().all(|l| {
                            let l = l.trim();
                            l.is_empty() || l.starts_with("--")
                        })
                    {
                        continue;
                    }

                    sqlx::query(statement)
                        .execute(&self.pool)
                        .await
                        .map_err(|e| {
                            ForgeError::Database(format!(
                                "Failed to rollback migration '{}': {}",
                                name, e
                            ))
                        })?;
                }
            } else {
                warn!("Migration '{}' has no down SQL, removing record only", name);
            }

            // Remove from migrations table
            sqlx::query!("DELETE FROM forge_migrations WHERE id = $1", id)
                .execute(&self.pool)
                .await
                .map_err(|e| {
                    ForgeError::Database(format!(
                        "Failed to remove migration record '{}': {}",
                        name, e
                    ))
                })?;

            info!("Rolled back migration: {}", name);
            rolled_back.push(name);
        }

        Ok(rolled_back)
    }

    /// Get the status of all migrations.
    pub async fn status(&self, available: &[Migration]) -> Result<MigrationStatus> {
        self.ensure_migrations_table().await?;

        let applied = self.get_applied_migrations().await?;

        let applied_list: Vec<AppliedMigration> = {
            let rows = sqlx::query!(
                "SELECT name, applied_at, down_sql FROM forge_migrations ORDER BY id ASC"
            )
            .fetch_all(&self.pool)
            .await
            .map_err(|e| ForgeError::Database(format!("Failed to get migrations: {}", e)))?;

            rows.into_iter()
                .map(|row| AppliedMigration {
                    name: row.name,
                    applied_at: row.applied_at,
                    has_down: row.down_sql.is_some(),
                })
                .collect()
        };

        let pending: Vec<String> = available
            .iter()
            .filter(|m| !applied.contains(&m.name))
            .map(|m| m.name.clone())
            .collect();

        Ok(MigrationStatus {
            applied: applied_list,
            pending,
        })
    }
}

/// Information about an applied migration.
#[derive(Debug, Clone)]
pub struct AppliedMigration {
    pub name: String,
    pub applied_at: chrono::DateTime<chrono::Utc>,
    pub has_down: bool,
}

/// Status of migrations.
#[derive(Debug, Clone)]
pub struct MigrationStatus {
    pub applied: Vec<AppliedMigration>,
    pub pending: Vec<String>,
}

/// Split SQL into individual statements, respecting dollar-quoted strings.
/// This handles PL/pgSQL functions that contain semicolons inside $$ delimiters.
fn split_sql_statements(sql: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut current = String::new();
    let mut in_dollar_quote = false;
    let mut dollar_tag = String::new();
    let mut chars = sql.chars().peekable();

    while let Some(c) = chars.next() {
        current.push(c);

        // Check for dollar-quoting start/end
        if c == '$' {
            // Look for a dollar-quote tag like $$ or $tag$
            let mut potential_tag = String::from("$");

            // Collect characters until we hit another $ or non-identifier char
            while let Some(&next_c) = chars.peek() {
                if next_c == '$' {
                    // Safe: peek confirmed the char exists
                    potential_tag.push(chars.next().expect("peeked char"));
                    current.push('$');
                    break;
                } else if next_c.is_alphanumeric() || next_c == '_' {
                    let c = chars.next().expect("peeked char");
                    potential_tag.push(c);
                    current.push(c);
                } else {
                    break;
                }
            }

            // Check if this is a valid dollar-quote delimiter (ends with $)
            if potential_tag.len() >= 2 && potential_tag.ends_with('$') {
                if in_dollar_quote && potential_tag == dollar_tag {
                    // End of dollar-quoted string
                    in_dollar_quote = false;
                    dollar_tag.clear();
                } else if !in_dollar_quote {
                    // Start of dollar-quoted string
                    in_dollar_quote = true;
                    dollar_tag = potential_tag;
                }
            }
        }

        // Split on semicolon only if not inside a dollar-quoted string
        if c == ';' && !in_dollar_quote {
            let stmt = current.trim().trim_end_matches(';').trim().to_string();
            if !stmt.is_empty() {
                statements.push(stmt);
            }
            current.clear();
        }
    }

    // Don't forget the last statement (might not end with ;)
    let stmt = current.trim().trim_end_matches(';').trim().to_string();
    if !stmt.is_empty() {
        statements.push(stmt);
    }

    statements
}

/// Load user migrations from a directory.
///
/// Migrations should be named like:
/// - `0001_create_users.sql`
/// - `0002_add_posts.sql`
///
/// They are sorted alphabetically and executed in order.
pub fn load_migrations_from_dir(dir: &Path) -> Result<Vec<Migration>> {
    if !dir.exists() {
        debug!("Migrations directory does not exist: {:?}", dir);
        return Ok(Vec::new());
    }

    let mut migrations = Vec::new();

    let entries = std::fs::read_dir(dir).map_err(ForgeError::Io)?;

    for entry in entries {
        let entry = entry.map_err(ForgeError::Io)?;
        let path = entry.path();

        if path.extension().map(|e| e == "sql").unwrap_or(false) {
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or_else(|| ForgeError::Config("Invalid migration filename".into()))?
                .to_string();

            let content = std::fs::read_to_string(&path).map_err(ForgeError::Io)?;

            migrations.push(Migration::parse(name, &content));
        }
    }

    // Sort by name (which includes the numeric prefix)
    migrations.sort_by(|a, b| a.name.cmp(&b.name));

    debug!("Loaded {} user migrations", migrations.len());
    Ok(migrations)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_load_migrations_from_empty_dir() {
        let dir = TempDir::new().unwrap();
        let migrations = load_migrations_from_dir(dir.path()).unwrap();
        assert!(migrations.is_empty());
    }

    #[test]
    fn test_load_migrations_from_nonexistent_dir() {
        let migrations = load_migrations_from_dir(Path::new("/nonexistent/path")).unwrap();
        assert!(migrations.is_empty());
    }

    #[test]
    fn test_load_migrations_sorted() {
        let dir = TempDir::new().unwrap();

        // Create migrations out of order
        fs::write(dir.path().join("0002_second.sql"), "SELECT 2;").unwrap();
        fs::write(dir.path().join("0001_first.sql"), "SELECT 1;").unwrap();
        fs::write(dir.path().join("0003_third.sql"), "SELECT 3;").unwrap();

        let migrations = load_migrations_from_dir(dir.path()).unwrap();
        assert_eq!(migrations.len(), 3);
        assert_eq!(migrations[0].name, "0001_first");
        assert_eq!(migrations[1].name, "0002_second");
        assert_eq!(migrations[2].name, "0003_third");
    }

    #[test]
    fn test_load_migrations_ignores_non_sql() {
        let dir = TempDir::new().unwrap();

        fs::write(dir.path().join("0001_migration.sql"), "SELECT 1;").unwrap();
        fs::write(dir.path().join("readme.txt"), "Not a migration").unwrap();
        fs::write(dir.path().join("backup.sql.bak"), "Backup").unwrap();

        let migrations = load_migrations_from_dir(dir.path()).unwrap();
        assert_eq!(migrations.len(), 1);
        assert_eq!(migrations[0].name, "0001_migration");
    }

    #[test]
    fn test_migration_new() {
        let m = Migration::new("test", "SELECT 1");
        assert_eq!(m.name, "test");
        assert_eq!(m.up_sql, "SELECT 1");
        assert!(m.down_sql.is_none());
    }

    #[test]
    fn test_migration_with_down() {
        let m = Migration::with_down("test", "CREATE TABLE t()", "DROP TABLE t");
        assert_eq!(m.name, "test");
        assert_eq!(m.up_sql, "CREATE TABLE t()");
        assert_eq!(m.down_sql, Some("DROP TABLE t".to_string()));
    }

    #[test]
    fn test_migration_parse_up_only() {
        let content = "CREATE TABLE users (id INT);";
        let m = Migration::parse("0001_test", content);
        assert_eq!(m.name, "0001_test");
        assert_eq!(m.up_sql, "CREATE TABLE users (id INT);");
        assert!(m.down_sql.is_none());
    }

    #[test]
    fn test_migration_parse_with_markers() {
        let content = r#"
-- @up
CREATE TABLE users (
    id UUID PRIMARY KEY,
    email VARCHAR(255)
);

-- @down
DROP TABLE users;
"#;
        let m = Migration::parse("0001_users", content);
        assert_eq!(m.name, "0001_users");
        assert!(m.up_sql.contains("CREATE TABLE users"));
        assert!(!m.up_sql.contains("@up"));
        assert!(!m.up_sql.contains("DROP TABLE"));
        assert_eq!(m.down_sql, Some("DROP TABLE users;".to_string()));
    }

    #[test]
    fn test_migration_parse_complex() {
        let content = r#"
-- @up
CREATE TABLE posts (
    id UUID PRIMARY KEY,
    title TEXT NOT NULL
);
CREATE INDEX idx_posts_title ON posts(title);

-- @down
DROP INDEX idx_posts_title;
DROP TABLE posts;
"#;
        let m = Migration::parse("0002_posts", content);
        assert!(m.up_sql.contains("CREATE TABLE posts"));
        assert!(m.up_sql.contains("CREATE INDEX"));
        let down = m.down_sql.unwrap();
        assert!(down.contains("DROP INDEX"));
        assert!(down.contains("DROP TABLE posts"));
    }

    #[tokio::test]
    async fn test_get_max_system_version_prefers_highest_applied_version() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect_lazy("postgres://localhost/nonexistent")
            .expect("lazy pool must build");
        let runner = MigrationRunner::new(pool);

        let applied = HashSet::from([
            "__forge_v003".to_string(),
            "__forge_v001".to_string(),
            "0001_user_schema".to_string(),
        ]);

        assert_eq!(runner.get_max_system_version(&applied), Some(3));
    }

    #[test]
    fn test_split_simple_statements() {
        let sql = "SELECT 1; SELECT 2; SELECT 3;";
        let stmts = super::split_sql_statements(sql);
        assert_eq!(stmts.len(), 3);
        assert_eq!(stmts[0], "SELECT 1");
        assert_eq!(stmts[1], "SELECT 2");
        assert_eq!(stmts[2], "SELECT 3");
    }

    #[test]
    fn test_split_with_dollar_quoted_function() {
        let sql = r#"
CREATE FUNCTION test() RETURNS void AS $$
BEGIN
    SELECT 1;
    SELECT 2;
END;
$$ LANGUAGE plpgsql;

SELECT 3;
"#;
        let stmts = super::split_sql_statements(sql);
        assert_eq!(stmts.len(), 2);
        assert!(stmts[0].contains("CREATE FUNCTION"));
        assert!(stmts[0].contains("$$ LANGUAGE plpgsql"));
        assert!(stmts[1].contains("SELECT 3"));
    }

    #[test]
    fn test_split_preserves_dollar_quote_content() {
        let sql = r#"
CREATE FUNCTION notify() RETURNS trigger AS $$
DECLARE
    row_id TEXT;
BEGIN
    row_id := NEW.id::TEXT;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;
"#;
        let stmts = super::split_sql_statements(sql);
        assert_eq!(stmts.len(), 1);
        assert!(stmts[0].contains("row_id := NEW.id::TEXT"));
    }
}
