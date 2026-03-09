use anyhow::Result;
use clap::Parser;
use console::style;
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;

use super::runtime_generator::{MismatchKind, verify_checksums};
use super::ui;

/// Validate project configuration and dependencies.
///
/// Checks that the project is correctly configured and all required
/// files are in place with valid content.
#[derive(Parser)]
pub struct CheckCommand {
    /// Path to forge.toml (default: ./forge.toml)
    #[arg(short, long, default_value = "forge.toml")]
    pub config: String,
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
        println!("  {} {}", ui::ok(), msg);
    }

    fn warn(&mut self, msg: &str, fix: &str) {
        println!("  {} {}", ui::warn(), msg);
        self.warnings.push(fix.to_string());
    }

    fn fail(&mut self, msg: &str, fix: &str) {
        println!("  {} {}", ui::error(), msg);
        self.errors.push(fix.to_string());
        self.passed = false;
    }

    fn info(&mut self, msg: &str) {
        println!("    {} {}", ui::info(), msg);
    }

    fn section(&mut self, title: &str) {
        println!();
        println!("  {} {}", ui::step(), style(title).bold());
    }
}

impl CheckCommand {
    /// Execute the check command.
    pub async fn execute(self) -> Result<()> {
        ui::section("FORGE Project Check");
        println!(
            "  {} Scanning project configuration and dependencies",
            ui::tool()
        );

        let mut result = CheckResult::new();

        result.section("Configuration");
        self.check_forge_toml(&mut result)?;
        self.check_cargo_toml(&mut result)?;

        result.section("Project Structure");
        self.check_directory_structure(&mut result);

        result.section("Migrations");
        self.check_migrations(&mut result)?;

        result.section("Functions");
        self.check_functions(&mut result)?;

        result.section("Schema");
        self.check_schema(&mut result)?;

        result.section("SQLx Cache");
        self.check_sqlx_cache(&mut result)?;

        result.section("Rust Tooling");
        self.check_rust_linting(&mut result).await;

        result.section("Frontend");
        self.check_frontend(&mut result)?;

        result.section("Generated Files");
        self.check_generated_files(&mut result)?;

        result.section("Frontend Tooling");
        self.check_frontend_linting(&mut result).await;

        // Summary
        println!();
        if result.passed && result.warnings.is_empty() {
            println!("{} All checks passed! Ready for development.", ui::ok());
            println!();
            println!("Next steps:");
            println!("  {} Start development", style("forge dev").cyan());
        } else if result.passed {
            println!(
                "{} Checks passed with {} warning(s)",
                ui::warn(),
                result.warnings.len()
            );
            println!();
            println!("Suggestions:");
            for warning in &result.warnings {
                println!("  {} {}", ui::step(), warning);
            }
        } else {
            println!(
                "{} {} error(s) found. Fix the issues and run 'forge check' again.",
                ui::error(),
                result.errors.len()
            );
            println!();
            println!("To fix:");
            for error in &result.errors {
                println!("  {} {}", ui::step(), error);
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
        let content = forge_core::config::substitute_env_vars(&content);
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
            if let Some(url) = db.get("url").and_then(|v| v.as_str()) {
                if url.starts_with("${") || url.starts_with("postgres://") {
                    result.pass("[database] configured");
                } else {
                    result.warn(
                        "[database].url format looks incorrect",
                        "Use postgres://user:pass@host:port/db or ${DATABASE_URL}",
                    );
                }
            } else {
                result.warn(
                    "[database].url not set",
                    "Add url = \"${DATABASE_URL}\" to [database]",
                );
            }
        } else {
            result.fail(
                "[database] section missing",
                "Add [database] section with url to forge.toml",
            );
        }

        // Check [gateway] section
        if let Some(gateway) = config.get("gateway")
            && let Some(port) = gateway.get("port")
            && let Some(p) = port.as_integer()
        {
            if (1..=65535).contains(&p) {
                result.pass(&format!("[gateway] configured (port {})", p));
            } else {
                result.fail(
                    &format!("[gateway].port {} is out of range", p),
                    "Use a port between 1 and 65535",
                );
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
                let Some(file_name) = path.file_name() else {
                    continue;
                };
                let filename = file_name.to_string_lossy();

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
                let Some(file_name) = path.file_name() else {
                    continue;
                };
                if file_name == "mod.rs" {
                    continue;
                }

                function_count += 1;
                let content = std::fs::read_to_string(&path)?;

                // Check for any forge macro
                if content.contains("#[forge::query")
                    || content.contains("#[forge::mutation")
                    || content.contains("#[forge::webhook")
                    || content.contains("#[forge::daemon")
                    || content.contains("#[forge::mcp_tool")
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
                "Create handlers in src/functions/ with #[forge::*] macros, then run forge generate",
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

        // Count model files and check for forge::model or standard derive patterns
        let mut model_count = 0;
        let mut forge_model_count = 0;
        let mut derive_count = 0;

        for entry in std::fs::read_dir(schema_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().is_some_and(|ext| ext == "rs") {
                let Some(file_name) = path.file_name() else {
                    continue;
                };
                if file_name == "mod.rs" {
                    continue;
                }

                model_count += 1;
                let content = std::fs::read_to_string(&path)?;

                if content.contains("#[forge::model") {
                    forge_model_count += 1;
                } else if content.contains("Serialize") || content.contains("FromRow") {
                    derive_count += 1;
                }
            }
        }

        let recognized = forge_model_count + derive_count;

        if model_count == 0 {
            result.warn(
                "No schema files found",
                "Create models in src/schema/, then run forge generate",
            );
        } else if recognized == model_count {
            if forge_model_count > 0 {
                result.pass(&format!(
                    "{} model file(s) with #[forge::model]",
                    forge_model_count
                ));
            }
            if derive_count > 0 {
                result.pass(&format!(
                    "{} model file(s) with standard derives (Serialize, FromRow)",
                    derive_count
                ));
            }
        } else {
            result.warn(
                &format!(
                    "{}/{} schema files have model definitions",
                    recognized, model_count
                ),
                "Add #[forge::model] or #[derive(Serialize, Deserialize, sqlx::FromRow)] to model structs",
            );
        }

        Ok(())
    }

    fn check_sqlx_cache(&self, result: &mut CheckResult) -> Result<()> {
        let sqlx_dir = Path::new(".sqlx");

        if !sqlx_dir.exists() {
            result.fail(
                ".sqlx/ directory missing",
                "Run 'forge migrate prepare' to generate the offline query cache",
            );
            return Ok(());
        }

        // Count query-*.json files
        let query_files: Vec<_> = std::fs::read_dir(sqlx_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("query-")
            })
            .collect();

        if query_files.is_empty() {
            result.fail(
                ".sqlx/ has no cached queries",
                "Run 'forge migrate prepare' to populate the offline cache",
            );
            return Ok(());
        }

        result.pass(&format!(".sqlx/ cache with {} query file(s)", query_files.len()));

        // Warn if migrations are newer than cache
        let migrations_dir = Path::new("migrations");
        if migrations_dir.exists() {
            let cache_mtime = query_files
                .iter()
                .filter_map(|e| e.metadata().ok())
                .filter_map(|m| m.modified().ok())
                .min();

            let migration_mtime = std::fs::read_dir(migrations_dir)?
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.path()
                        .extension()
                        .is_some_and(|ext| ext == "sql")
                })
                .filter_map(|e| e.metadata().ok())
                .filter_map(|m| m.modified().ok())
                .max();

            if let (Some(oldest_cache), Some(newest_migration)) = (cache_mtime, migration_mtime) {
                if newest_migration > oldest_cache {
                    result.warn(
                        "Migrations are newer than .sqlx/ cache",
                        "Run 'forge migrate prepare' to refresh the cache",
                    );
                }
            }
        }

        // Check sqlx.toml
        let sqlx_toml = Path::new("sqlx.toml");
        if sqlx_toml.exists() {
            let content = std::fs::read_to_string(sqlx_toml)?;
            if content.contains("offline = true") {
                result.pass("sqlx.toml configured with offline = true");
            } else {
                result.warn(
                    "sqlx.toml missing offline = true",
                    "Add [common] offline = true to sqlx.toml",
                );
            }
        } else {
            result.warn(
                "sqlx.toml not found",
                "Create sqlx.toml with [common] offline = true",
            );
        }

        Ok(())
    }

    fn check_generated_files(&self, result: &mut CheckResult) -> Result<()> {
        let frontend_dir = Path::new("frontend");
        if !frontend_dir.exists() {
            return Ok(());
        }

        let forge_dir = frontend_dir.join(".forge");
        if !forge_dir.exists() {
            result.warn(
                "No .forge/ directory found",
                "Run 'forge generate' to create runtime files",
            );
            return Ok(());
        }

        println!();

        match verify_checksums(frontend_dir) {
            Ok(mismatches) if mismatches.is_empty() => {
                result.pass("Generated files integrity verified");
            }
            Ok(mismatches) => {
                for m in &mismatches {
                    match m.kind {
                        MismatchKind::Modified => {
                            result.fail(
                                &format!(".forge/{} has been modified", m.file),
                                "Run 'forge generate' to restore generated files",
                            );
                        }
                        MismatchKind::Missing => {
                            result.fail(
                                &format!(".forge/{} is missing", m.file),
                                "Run 'forge generate' to restore generated files",
                            );
                        }
                    }
                }
            }
            Err(_) => {
                result.warn(
                    "Could not verify generated file integrity",
                    "Run 'forge generate' to regenerate checksums",
                );
            }
        }

        Ok(())
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
            .stderr(Stdio::null())
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
            .stderr(Stdio::null())
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
        let eslint_result = Command::new("bunx")
            .args(["eslint", "."])
            .current_dir(frontend_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;

        match eslint_result {
            Ok(status) if status.success() => {
                result.pass("ESLint check passed");
            }
            Ok(_) => {
                result.fail(
                    "ESLint errors found",
                    "Run 'cd frontend && bunx eslint .' to see errors",
                );
            }
            Err(_) => {
                result.warn(
                    "Could not run ESLint",
                    "Ensure eslint is installed in frontend/",
                );
            }
        }

        // Check Prettier
        let prettier_result = Command::new("bunx")
            .args(["prettier", "--check", "."])
            .current_dir(frontend_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
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
                    "Ensure prettier is installed in frontend/",
                );
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn test_check_result() {
        let result = CheckResult::new();
        assert!(result.passed);
        assert!(result.warnings.is_empty());
        assert!(result.errors.is_empty());
    }
}
