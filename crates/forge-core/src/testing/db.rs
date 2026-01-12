//! Database provisioning for tests.
//!
//! Provides PostgreSQL access for integration tests. Database configuration
//! options:
//! 1. Pass a URL directly via `from_url()`
//! 2. Use `from_env()` to explicitly read from TEST_DATABASE_URL
//! 3. Use `embedded()` for automatic embedded PostgreSQL (requires `embedded-db` feature)
//!
//! This design prevents accidental use of production databases. The .env file
//! DATABASE_URL is NEVER automatically read.

use sqlx::PgPool;
use std::path::Path;
use tracing::{debug, info};

use crate::error::{ForgeError, Result};

#[cfg(feature = "embedded-db")]
use tokio::sync::OnceCell;

#[cfg(feature = "embedded-db")]
static EMBEDDED_PG: OnceCell<postgresql_embedded::PostgreSQL> = OnceCell::const_new();

/// Database access for tests.
///
/// Test database configuration is intentionally explicit to prevent
/// accidental use of production databases.
///
/// # Examples
///
/// ```ignore
/// // Option 1: Embedded Postgres (requires embedded-db feature)
/// // Run with: cargo test --features embedded-db
/// let db = TestDatabase::embedded().await?;
///
/// // Option 2: Explicit URL
/// let db = TestDatabase::from_url("postgres://localhost/test_db").await?;
///
/// // Option 3: From TEST_DATABASE_URL env var
/// let db = TestDatabase::from_env().await?;
/// ```
pub struct TestDatabase {
    pool: PgPool,
    url: String,
}

impl TestDatabase {
    /// Connect to database at the given URL.
    ///
    /// Use this for explicit database configuration in tests.
    pub async fn from_url(url: &str) -> Result<Self> {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(10)
            .connect(url)
            .await
            .map_err(ForgeError::Sql)?;

        Ok(Self {
            pool,
            url: url.to_string(),
        })
    }

    /// Connect using TEST_DATABASE_URL environment variable.
    ///
    /// Note: This reads TEST_DATABASE_URL (not DATABASE_URL) to prevent
    /// accidental use of production databases in tests.
    pub async fn from_env() -> Result<Self> {
        let url = std::env::var("TEST_DATABASE_URL").map_err(|_| {
            ForgeError::Database(
                "TEST_DATABASE_URL not set. Set it explicitly for database tests.".to_string(),
            )
        })?;
        Self::from_url(&url).await
    }

    /// Start an embedded PostgreSQL instance.
    ///
    /// Downloads and starts a real PostgreSQL instance automatically.
    /// Requires the `embedded-db` feature: `cargo test --features embedded-db`
    #[cfg(feature = "embedded-db")]
    pub async fn embedded() -> Result<Self> {
        let pg = EMBEDDED_PG
            .get_or_try_init(|| async {
                let mut pg = postgresql_embedded::PostgreSQL::default();
                pg.setup().await.map_err(|e| {
                    ForgeError::Database(format!("Failed to setup embedded Postgres: {}", e))
                })?;
                pg.start().await.map_err(|e| {
                    ForgeError::Database(format!("Failed to start embedded Postgres: {}", e))
                })?;
                Ok::<_, ForgeError>(pg)
            })
            .await?;

        let url = pg.settings().url("postgres");
        Self::from_url(&url).await
    }

    /// Get the connection pool.
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Get the database URL.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Run raw SQL to set up test data or schema.
    pub async fn execute(&self, sql: &str) -> Result<()> {
        sqlx::query(sql)
            .execute(&self.pool)
            .await
            .map_err(ForgeError::Sql)?;
        Ok(())
    }

    /// Creates a dedicated database for a single test, providing full isolation.
    ///
    /// Each call creates a new database with a unique name. Use this when tests
    /// modify data and could interfere with each other.
    pub async fn isolated(&self, test_name: &str) -> Result<IsolatedTestDb> {
        let base_url = self.url.clone();
        // UUID suffix prevents collisions when tests run in parallel
        let db_name = format!(
            "forge_test_{}_{}",
            sanitize_db_name(test_name),
            uuid::Uuid::new_v4().simple()
        );

        // Connect to default database to create the test database
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect(&base_url)
            .await
            .map_err(ForgeError::Sql)?;

        // Double-quoted identifier handles special characters in generated name
        sqlx::query(&format!("CREATE DATABASE \"{}\"", db_name))
            .execute(&pool)
            .await
            .map_err(ForgeError::Sql)?;

        // Build URL for the new database by replacing the database name component
        let test_url = replace_db_name(&base_url, &db_name);

        let test_pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(5)
            .connect(&test_url)
            .await
            .map_err(ForgeError::Sql)?;

        Ok(IsolatedTestDb {
            pool: test_pool,
            db_name,
            base_url,
        })
    }
}

/// A test database that exists for the lifetime of a single test.
///
/// The database is automatically created on construction. Cleanup happens
/// when `cleanup()` is called or when the database is reused in subsequent
/// test runs (orphaned databases are cleaned up automatically).
pub struct IsolatedTestDb {
    pool: PgPool,
    db_name: String,
    base_url: String,
}

impl IsolatedTestDb {
    /// Get the connection pool for this isolated database.
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Get the database name.
    pub fn db_name(&self) -> &str {
        &self.db_name
    }

    /// Run raw SQL to set up test data or schema.
    pub async fn execute(&self, sql: &str) -> Result<()> {
        sqlx::query(sql)
            .execute(&self.pool)
            .await
            .map_err(ForgeError::Sql)?;
        Ok(())
    }

    /// Run multi-statement SQL for setup.
    ///
    /// This handles SQL with multiple statements separated by semicolons,
    /// including PL/pgSQL functions with dollar-quoted strings.
    pub async fn run_sql(&self, sql: &str) -> Result<()> {
        let statements = split_sql_statements(sql);
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
                .map_err(|e| ForgeError::Database(format!("Failed to execute SQL: {}", e)))?;
        }
        Ok(())
    }

    /// Cleanup the test database by dropping it.
    ///
    /// Call this at the end of your test if you want immediate cleanup.
    /// Otherwise, orphaned databases will be cleaned up on subsequent test runs.
    pub async fn cleanup(self) -> Result<()> {
        // Close all connections first
        self.pool.close().await;

        // Connect to default database to drop the test database
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect(&self.base_url)
            .await
            .map_err(ForgeError::Sql)?;

        // Force disconnect other connections and drop
        let _ = sqlx::query(&format!(
            "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '{}'",
            self.db_name
        ))
        .execute(&pool)
        .await;

        sqlx::query(&format!("DROP DATABASE IF EXISTS \"{}\"", self.db_name))
            .execute(&pool)
            .await
            .map_err(ForgeError::Sql)?;

        Ok(())
    }

    /// Run migrations from a directory.
    ///
    /// Loads all `.sql` files from the directory, sorts them alphabetically,
    /// and executes them in order. This is intended for test setup.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let base = TestDatabase::embedded().await?;
    /// let db = base.isolated("my_test").await?;
    /// db.migrate(Path::new("migrations")).await?;
    /// ```
    pub async fn migrate(&self, migrations_dir: &Path) -> Result<()> {
        if !migrations_dir.exists() {
            debug!("Migrations directory does not exist: {:?}", migrations_dir);
            return Ok(());
        }

        let mut migrations = Vec::new();

        let entries = std::fs::read_dir(migrations_dir).map_err(ForgeError::Io)?;

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
                migrations.push((name, content));
            }
        }

        // Sort by name (which includes the numeric prefix)
        migrations.sort_by(|a, b| a.0.cmp(&b.0));

        debug!("Running {} migrations for test", migrations.len());

        for (name, content) in migrations {
            info!("Applying test migration: {}", name);

            // Parse content to extract up SQL (everything before -- @down marker)
            let up_sql = parse_up_sql(&content);

            // Split into individual statements and execute
            let statements = split_sql_statements(&up_sql);
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
                        ForgeError::Database(format!("Failed to apply migration '{}': {}", name, e))
                    })?;
            }
        }

        Ok(())
    }
}

/// Sanitize a test name for use in a database name.
fn sanitize_db_name(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .take(32)
        .collect()
}

/// Replace the database name in a connection URL.
fn replace_db_name(url: &str, new_db: &str) -> String {
    // Handle both postgres://.../ and postgres://...? formats
    if let Some(idx) = url.rfind('/') {
        let base = &url[..=idx];
        // Check if there are query params
        if let Some(query_idx) = url[idx + 1..].find('?') {
            let query = &url[idx + 1 + query_idx..];
            format!("{}{}{}", base, new_db, query)
        } else {
            format!("{}{}", base, new_db)
        }
    } else {
        format!("{}/{}", url, new_db)
    }
}

/// Parse migration content, extracting only the up SQL (before -- @down marker).
fn parse_up_sql(content: &str) -> String {
    let down_marker_patterns = ["-- @down", "--@down", "-- @DOWN", "--@DOWN"];

    for pattern in down_marker_patterns {
        if let Some(idx) = content.find(pattern) {
            let up_part = &content[..idx];
            return up_part
                .replace("-- @up", "")
                .replace("--@up", "")
                .replace("-- @UP", "")
                .replace("--@UP", "")
                .trim()
                .to_string();
        }
    }

    // No @down marker found - treat entire content as up SQL
    content
        .replace("-- @up", "")
        .replace("--@up", "")
        .replace("-- @UP", "")
        .replace("--@UP", "")
        .trim()
        .to_string()
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
                    potential_tag.push(chars.next().unwrap());
                    current.push('$');
                    break;
                } else if next_c.is_alphanumeric() || next_c == '_' {
                    potential_tag.push(chars.next().unwrap());
                    current.push(potential_tag.chars().last().unwrap());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_db_name() {
        assert_eq!(sanitize_db_name("my_test"), "my_test");
        assert_eq!(sanitize_db_name("my-test"), "my_test");
        assert_eq!(sanitize_db_name("my test"), "my_test");
        assert_eq!(sanitize_db_name("test::function"), "test__function");
    }

    #[test]
    fn test_replace_db_name() {
        assert_eq!(
            replace_db_name("postgres://localhost/olddb", "newdb"),
            "postgres://localhost/newdb"
        );
        assert_eq!(
            replace_db_name("postgres://user:pass@localhost:5432/olddb", "newdb"),
            "postgres://user:pass@localhost:5432/newdb"
        );
        assert_eq!(
            replace_db_name("postgres://localhost/olddb?sslmode=disable", "newdb"),
            "postgres://localhost/newdb?sslmode=disable"
        );
    }
}
