use anyhow::Result;
use clap::Parser;
use console::style;
use sqlx::Row;
use sqlx::postgres::PgPoolOptions;
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;

/// Validate project configuration and dependencies.
///
/// Checks that the project is correctly configured and all required
/// files are in place with valid content.
#[derive(Parser)]
pub struct CheckCommand {
    /// Path to forge.toml (default: ./forge.toml)
    #[arg(short, long, default_value = "forge.toml")]
    pub config: String,

    /// Skip database connectivity check
    #[arg(long)]
    pub no_db: bool,
}

struct CheckResult {
    passed: bool,
    warnings: Vec<String>,
    errors: Vec<String>,
}

impl CheckResult {
    fn new() -> Self {
        Self {
            passed: true,
            warnings: Vec::new(),
            errors: Vec::new(),
        }
    }

    fn pass(&mut self, msg: &str) {
        println!("  {} {}", style("✓").green(), msg);
    }

    fn warn(&mut self, msg: &str, fix: &str) {
        println!("  {} {}", style("⚠").yellow(), msg);
        self.warnings.push(fix.to_string());
    }

    fn fail(&mut self, msg: &str, fix: &str) {
        println!("  {} {}", style("✗").red(), msg);
        self.errors.push(fix.to_string());
        self.passed = false;
    }

    fn info(&mut self, msg: &str) {
        println!("    {} {}", style("→").dim(), msg);
    }
}

impl CheckCommand {
    /// Execute the check command.
    pub async fn execute(self) -> Result<()> {
        println!();
        println!(
            "{} Checking FORGE project configuration...",
            style("🔍").cyan()
        );
        println!();

        let mut result = CheckResult::new();

        // Check 1: forge.toml
        self.check_forge_toml(&mut result)?;

        // Check 2: Cargo.toml
        self.check_cargo_toml(&mut result)?;

        // Check 3: Directory structure
        self.check_directory_structure(&mut result);

        // Check 4: Migration files
        self.check_migrations(&mut result)?;

        // Check 5: Functions
        self.check_functions(&mut result)?;

        // Check 6: Schema
        self.check_schema(&mut result)?;

        // Check 7: Code quality (cargo fmt, clippy)
        self.check_rust_linting(&mut result).await;

        // Check 8: Environment
        let database_url = self.check_environment(&mut result);

        // Check 9: Database connectivity
        if !self.no_db {
            if let Some(url) = database_url {
                self.check_database(&mut result, &url).await;
            }
        }

        // Check 10: Frontend (if exists)
        self.check_frontend(&mut result)?;

        // Check 11: Frontend linting (if frontend exists)
        self.check_frontend_linting(&mut result).await;

        // Summary
        println!();
        if result.passed && result.warnings.is_empty() {
            println!(
                "{} All checks passed! Ready for development.",
                style("✅").green()
            );
            println!();
            println!("Next steps:");
            println!("  {} Start development", style("forge dev").cyan());
        } else if result.passed {
            println!(
                "{} Checks passed with {} warning(s)",
                style("⚠").yellow(),
                result.warnings.len()
            );
            println!();
            println!("Suggestions:");
            for warning in &result.warnings {
                println!("  {} {}", style("→").dim(), warning);
            }
        } else {
            println!(
                "{} {} error(s) found. Fix the issues and run 'forge check' again.",
                style("❌").red(),
                result.errors.len()
            );
            println!();
            println!("To fix:");
            for error in &result.errors {
                println!("  {} {}", style("→").dim(), error);
            }
            return Err(anyhow::anyhow!("Project check failed"));
        }

        println!();
        Ok(())
    }

    fn check_forge_toml(&self, result: &mut CheckResult) -> Result<()> {
        let config_path = Path::new(&self.config);

        if !config_path.exists() {
            result.fail(
                "forge.toml not found",
                "Create a new project with: forge new my-app --demo",
            );
            return Ok(());
        }

        let content = std::fs::read_to_string(config_path)?;
        let config: toml::Value = match toml::from_str(&content) {
            Ok(c) => {
                result.pass("forge.toml is valid TOML");
                c
            }
            Err(e) => {
                result.fail(
                    &format!("forge.toml parse error: {}", e),
                    "Fix the TOML syntax errors in forge.toml",
                );
                return Ok(());
            }
        };

        // Check [project] section
        if let Some(project) = config.get("project") {
            if project.get("name").is_some() {
                result.pass("[project] section configured");
            } else {
                result.warn(
                    "[project].name missing",
                    "Add name = \"your-app\" to [project] section",
                );
            }
        } else {
            result.fail(
                "[project] section missing",
                "Add [project] section with name to forge.toml",
            );
        }

        // Check [database] section
        if let Some(db) = config.get("database") {
            if let Some(url) = db.get("url") {
                if let Some(url_str) = url.as_str() {
                    if url_str.starts_with("${") || url_str.starts_with("postgres://") {
                        result.pass("[database] section configured");
                    } else {
                        result.warn(
                            "[database].url format looks incorrect",
                            "Use postgres://user:pass@host:port/db or ${DATABASE_URL}",
                        );
                    }
                }
            } else if db.get("embedded").and_then(|v| v.as_bool()) == Some(true) {
                result.pass("[database] configured for embedded postgres");
            } else {
                result.warn(
                    "[database].url not set",
                    "Add url = \"${DATABASE_URL}\" or embedded = true to [database]",
                );
            }
        } else {
            result.fail(
                "[database] section missing",
                "Add [database] section with url to forge.toml",
            );
        }

        // Check [gateway] section
        if let Some(gateway) = config.get("gateway") {
            if let Some(port) = gateway.get("port") {
                if let Some(p) = port.as_integer() {
                    if (1..=65535).contains(&p) {
                        result.pass(&format!("[gateway] configured (port {})", p));
                    } else {
                        result.fail(
                            &format!("[gateway].port {} is out of range", p),
                            "Use a port between 1 and 65535",
                        );
                    }
                }
            }
        }

        Ok(())
    }

    fn check_cargo_toml(&self, result: &mut CheckResult) -> Result<()> {
        let cargo_path = Path::new("Cargo.toml");

        if !cargo_path.exists() {
            result.fail(
                "Cargo.toml not found",
                "This doesn't appear to be a Rust project",
            );
            return Ok(());
        }

        let content = std::fs::read_to_string(cargo_path)?;
        let cargo: toml::Value = match toml::from_str(&content) {
            Ok(c) => c,
            Err(e) => {
                result.fail(
                    &format!("Cargo.toml parse error: {}", e),
                    "Fix the TOML syntax errors in Cargo.toml",
                );
                return Ok(());
            }
        };

        // Check for forge/forgex dependency
        let has_forge_dep = cargo
            .get("dependencies")
            .and_then(|deps| deps.get("forge").or_else(|| deps.get("forgex")))
            .is_some();

        if has_forge_dep {
            result.pass("forge dependency found in Cargo.toml");
        } else {
            result.fail(
                "forge dependency not found",
                "Add forge = { version = \"0.0.3\", package = \"forgex\" } to [dependencies]",
            );
        }

        Ok(())
    }

    fn check_directory_structure(&self, result: &mut CheckResult) {
        let dirs = [
            ("src/", "Source directory"),
            ("src/schema/", "Schema directory"),
            ("src/functions/", "Functions directory"),
            ("migrations/", "Migrations directory"),
        ];

        for (dir, name) in dirs {
            if Path::new(dir).exists() {
                result.pass(&format!("{} exists", name));
            } else {
                result.fail(
                    &format!("{} missing", name),
                    &format!("Create {} directory", dir),
                );
            }
        }
    }

    fn check_migrations(&self, result: &mut CheckResult) -> Result<()> {
        let migrations_dir = Path::new("migrations");
        if !migrations_dir.exists() {
            return Ok(());
        }

        let mut migration_count = 0;
        let mut valid_count = 0;
        let mut issues = Vec::new();

        for entry in std::fs::read_dir(migrations_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().is_some_and(|ext| ext == "sql") {
                migration_count += 1;
                let filename = path.file_name().unwrap().to_string_lossy();

                // Check naming convention: NNNN_name.sql
                let name_valid = filename
                    .split('_')
                    .next()
                    .map(|prefix| prefix.chars().all(|c| c.is_ascii_digit()))
                    .unwrap_or(false);

                if !name_valid {
                    issues.push(format!("{} - should be NNNN_name.sql", filename));
                    continue;
                }

                // Check for @up marker
                let content = std::fs::read_to_string(&path)?;
                if content.contains("-- @up") {
                    valid_count += 1;
                } else {
                    issues.push(format!("{} - missing '-- @up' marker", filename));
                }
            }
        }

        if migration_count == 0 {
            result.warn(
                "No migration files found",
                "Create migrations/0001_initial.sql with schema",
            );
        } else if issues.is_empty() {
            result.pass(&format!("{} migration file(s) valid", valid_count));
        } else {
            result.warn(
                &format!(
                    "{}/{} migrations have issues",
                    issues.len(),
                    migration_count
                ),
                "Fix migration file naming or add '-- @up' marker",
            );
            for issue in issues.iter().take(3) {
                result.info(issue);
            }
            if issues.len() > 3 {
                result.info(&format!("... and {} more", issues.len() - 3));
            }
        }

        Ok(())
    }

    fn check_functions(&self, result: &mut CheckResult) -> Result<()> {
        let functions_dir = Path::new("src/functions");
        if !functions_dir.exists() {
            return Ok(());
        }

        let mod_file = functions_dir.join("mod.rs");
        if !mod_file.exists() {
            result.fail(
                "src/functions/mod.rs not found",
                "Create mod.rs to export your functions",
            );
            return Ok(());
        }

        // Count function files and check for forge macros
        let mut function_count = 0;
        let mut macro_count = 0;

        for entry in std::fs::read_dir(functions_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().is_some_and(|ext| ext == "rs") {
                let filename = path.file_name().unwrap().to_string_lossy();
                if filename == "mod.rs" {
                    continue;
                }

                function_count += 1;
                let content = std::fs::read_to_string(&path)?;

                // Check for any forge macro
                if content.contains("#[forge::query")
                    || content.contains("#[forge::mutation")
                    || content.contains("#[forge::action")
                    || content.contains("#[forge::job")
                    || content.contains("#[forge::cron")
                    || content.contains("#[forge::workflow")
                {
                    macro_count += 1;
                }
            }
        }

        if function_count == 0 {
            result.warn(
                "No function files found",
                "Add functions with forge add query/mutation/action",
            );
        } else if macro_count == function_count {
            result.pass(&format!(
                "{} function file(s) with forge macros",
                macro_count
            ));
        } else {
            result.warn(
                &format!("{}/{} files have forge macros", macro_count, function_count),
                "Ensure all function files use #[forge::*] macros",
            );
        }

        Ok(())
    }

    fn check_schema(&self, result: &mut CheckResult) -> Result<()> {
        let schema_dir = Path::new("src/schema");
        if !schema_dir.exists() {
            return Ok(());
        }

        let mod_file = schema_dir.join("mod.rs");
        if !mod_file.exists() {
            result.fail(
                "src/schema/mod.rs not found",
                "Create mod.rs to export your models",
            );
            return Ok(());
        }

        // Count model files and check for forge::model macro
        let mut model_count = 0;
        let mut macro_count = 0;

        for entry in std::fs::read_dir(schema_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().is_some_and(|ext| ext == "rs") {
                let filename = path.file_name().unwrap().to_string_lossy();
                if filename == "mod.rs" {
                    continue;
                }

                model_count += 1;
                let content = std::fs::read_to_string(&path)?;

                if content.contains("#[forge::model") {
                    macro_count += 1;
                }
            }
        }

        if model_count == 0 {
            result.warn("No schema files found", "Add models with forge add model");
        } else if macro_count == model_count {
            result.pass(&format!(
                "{} model file(s) with #[forge::model]",
                macro_count
            ));
        } else {
            result.warn(
                &format!("{}/{} files have #[forge::model]", macro_count, model_count),
                "Ensure all model files use #[forge::model] macro",
            );
        }

        Ok(())
    }

    fn check_environment(&self, result: &mut CheckResult) -> Option<String> {
        // Check .env file
        let env_path = Path::new(".env");
        if !env_path.exists() {
            result.warn(
                ".env file not found",
                "Create .env with DATABASE_URL (or use forge dev for embedded postgres)",
            );
            return None;
        }

        result.pass(".env file found");

        // Check DATABASE_URL
        let mut database_url = std::env::var("DATABASE_URL").ok();

        if database_url.is_none() {
            if let Ok(content) = std::fs::read_to_string(env_path) {
                for line in content.lines() {
                    let line = line.trim();
                    if line.starts_with("DATABASE_URL=") {
                        let url = line.trim_start_matches("DATABASE_URL=").trim();
                        if !url.is_empty() {
                            database_url = Some(url.to_string());
                        }
                        break;
                    }
                }
            }
        }

        if let Some(ref url) = database_url {
            result.pass("DATABASE_URL is set");
            let masked = mask_database_url(url);
            result.info(&masked);

            // Warn about localhost in production-like settings
            if url.contains("localhost") || url.contains("127.0.0.1") {
                result.info("Using localhost - fine for development");
            }
        } else {
            result.warn(
                "DATABASE_URL not set",
                "Set DATABASE_URL in .env or use forge dev --no-pg",
            );
        }

        database_url
    }

    async fn check_database(&self, result: &mut CheckResult, url: &str) {
        println!();
        println!("  {} Checking database connection...", style("⋯").cyan());

        match check_database_connection(url).await {
            Ok(version) => {
                result.pass("Database connection successful");
                result.info(&format!("PostgreSQL {}", version));
            }
            Err(e) => {
                result.fail(
                    &format!("Database connection failed: {}", e),
                    "Check DATABASE_URL and ensure PostgreSQL is running",
                );
            }
        }
    }

    fn check_frontend(&self, result: &mut CheckResult) -> Result<()> {
        let frontend_dir = Path::new("frontend");
        if !frontend_dir.exists() {
            result.info("No frontend/ directory (backend-only project)");
            return Ok(());
        }

        println!();
        result.pass("frontend/ directory exists");

        // Check package.json
        let package_json = frontend_dir.join("package.json");
        if !package_json.exists() {
            result.fail(
                "frontend/package.json not found",
                "Run 'cd frontend && bun init' to initialize",
            );
            return Ok(());
        }

        let content = std::fs::read_to_string(&package_json)?;
        let package: serde_json::Value = match serde_json::from_str(&content) {
            Ok(p) => p,
            Err(e) => {
                result.fail(
                    &format!("package.json parse error: {}", e),
                    "Fix JSON syntax in package.json",
                );
                return Ok(());
            }
        };

        // Check for svelte dependency
        let has_svelte = package
            .get("devDependencies")
            .or_else(|| package.get("dependencies"))
            .and_then(|deps| deps.get("svelte"))
            .is_some();

        if has_svelte {
            result.pass("Svelte dependency found");
        } else {
            result.warn(
                "Svelte not found in dependencies",
                "This might not be a FORGE frontend project",
            );
        }

        // Check for .forge/svelte runtime
        let runtime_dir = frontend_dir.join(".forge").join("svelte");
        if runtime_dir.exists() {
            result.pass("FORGE runtime generated");
        } else {
            result.warn(
                ".forge/svelte runtime not found",
                "Run 'forge generate' to create TypeScript types",
            );
        }

        // Check node_modules
        let node_modules = frontend_dir.join("node_modules");
        if node_modules.exists() {
            result.pass("Frontend dependencies installed");
        } else {
            result.warn(
                "Frontend dependencies not installed",
                "Run 'cd frontend && bun install'",
            );
        }

        Ok(())
    }

    async fn check_rust_linting(&self, result: &mut CheckResult) {
        println!();

        // Check cargo fmt
        let fmt_result = Command::new("cargo")
            .args(["fmt", "--check"])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .status()
            .await;

        match fmt_result {
            Ok(status) if status.success() => {
                result.pass("cargo fmt check passed");
            }
            Ok(_) => {
                result.fail(
                    "Code formatting issues found",
                    "Run 'cargo fmt' to fix formatting",
                );
            }
            Err(_) => {
                result.warn(
                    "Could not run cargo fmt",
                    "Ensure rustfmt is installed: rustup component add rustfmt",
                );
            }
        }

        // Check cargo clippy
        let clippy_result = Command::new("cargo")
            .args(["clippy", "--", "-D", "warnings"])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .status()
            .await;

        match clippy_result {
            Ok(status) if status.success() => {
                result.pass("cargo clippy check passed");
            }
            Ok(_) => {
                result.fail(
                    "Clippy warnings found",
                    "Run 'cargo clippy' to see warnings",
                );
            }
            Err(_) => {
                result.warn(
                    "Could not run cargo clippy",
                    "Ensure clippy is installed: rustup component add clippy",
                );
            }
        }
    }

    async fn check_frontend_linting(&self, result: &mut CheckResult) {
        let frontend_dir = Path::new("frontend");
        if !frontend_dir.exists() {
            return;
        }

        // Check if node_modules exists (linting needs deps)
        if !frontend_dir.join("node_modules").exists() {
            return;
        }

        println!();

        // Check ESLint
        let eslint_result = Command::new("bun")
            .args(["run", "lint"])
            .current_dir(frontend_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .status()
            .await;

        match eslint_result {
            Ok(status) if status.success() => {
                result.pass("ESLint check passed");
            }
            Ok(_) => {
                result.fail(
                    "ESLint errors found",
                    "Run 'cd frontend && bun run lint' to see errors",
                );
            }
            Err(_) => {
                result.warn(
                    "Could not run ESLint",
                    "Check frontend/package.json has 'lint' script",
                );
            }
        }

        // Check Prettier
        let prettier_result = Command::new("bun")
            .args(["run", "format:check"])
            .current_dir(frontend_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .status()
            .await;

        match prettier_result {
            Ok(status) if status.success() => {
                result.pass("Prettier check passed");
            }
            Ok(_) => {
                result.fail(
                    "Prettier formatting issues found",
                    "Run 'cd frontend && bun run format' to fix",
                );
            }
            Err(_) => {
                result.warn(
                    "Could not run Prettier check",
                    "Check frontend/package.json has 'format:check' script",
                );
            }
        }
    }
}

fn mask_database_url(url: &str) -> String {
    if let Some(at_pos) = url.find('@') {
        if let Some(colon_pos) = url[..at_pos].rfind(':') {
            let protocol_end = url.find("://").map(|p| p + 3).unwrap_or(0);
            if colon_pos > protocol_end {
                let before_password = &url[..colon_pos + 1];
                let after_password = &url[at_pos..];
                return format!("{}****{}", before_password, after_password);
            }
        }
    }
    url.to_string()
}

async fn check_database_connection(url: &str) -> Result<String> {
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect(url)
        .await?;

    let row = sqlx::query("SELECT version()").fetch_one(&pool).await?;
    let version_str: String = row.get(0);

    let version = version_str
        .split_whitespace()
        .nth(1)
        .unwrap_or(&version_str)
        .to_string();

    pool.close().await;
    Ok(version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mask_database_url() {
        assert_eq!(
            mask_database_url("postgres://user:secret@localhost/db"),
            "postgres://user:****@localhost/db"
        );
        assert_eq!(
            mask_database_url("postgres://localhost/db"),
            "postgres://localhost/db"
        );
    }

    #[test]
    fn test_check_result() {
        let result = CheckResult::new();
        assert!(result.passed);
        assert!(result.warnings.is_empty());
        assert!(result.errors.is_empty());
    }
}
