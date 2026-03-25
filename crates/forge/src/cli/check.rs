use anyhow::Result;
use clap::Parser;
use console::style;
use std::path::Path;
use std::process::{Command as StdCommand, Stdio};
use tokio::process::Command as TokioCommand;

use super::frontend_codegen::BindingGeneratorInput;
use super::frontend_target::FrontendTarget;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SqlxCacheCheck {
    Missing,
    Empty,
    Ready(usize),
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

        result.section("Generated Bindings");
        self.check_generated_bindings(&mut result)?;

        result.section("Frontend Tooling");
        self.check_frontend_linting(&mut result).await;

        // Summary
        println!();
        if result.passed && result.warnings.is_empty() {
            println!("{} All checks passed! Ready for development.", ui::ok());
            println!();
            println!("Next steps:");
            println!(
                "  {} Start development",
                style("docker compose up --build").cyan()
            );
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
                "Create a new project with: forge new my-app --template with-svelte/minimal",
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
        let uses_compile_time_macros = project_uses_compile_time_sqlx_macros(Path::new("src"))?;
        let cache_status = inspect_sqlx_cache(sqlx_dir)?;

        match cache_status {
            SqlxCacheCheck::Missing => {
                if uses_compile_time_macros {
                    result.fail(
                        ".sqlx/ directory missing",
                        "Run 'forge migrate prepare' to generate the offline query cache",
                    );
                } else {
                    result.info("No .sqlx/ cache yet (no compile-time sqlx macros found)");
                }
                return Ok(());
            }
            SqlxCacheCheck::Empty => {
                if uses_compile_time_macros {
                    result.fail(
                        ".sqlx/ has no cached queries",
                        "Run 'forge migrate prepare' to populate the offline cache",
                    );
                } else {
                    result.pass(".sqlx/ directory present");
                }
                return Ok(());
            }
            SqlxCacheCheck::Ready(query_file_count) => {
                result.pass(&format!(
                    ".sqlx/ cache with {} query file(s)",
                    query_file_count
                ));
            }
        }

        let query_files: Vec<_> = std::fs::read_dir(sqlx_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("query-"))
            .collect();

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
                .filter(|e| e.path().extension().is_some_and(|ext| ext == "sql"))
                .filter_map(|e| e.metadata().ok())
                .filter_map(|m| m.modified().ok())
                .max();

            if let (Some(oldest_cache), Some(newest_migration)) = (cache_mtime, migration_mtime)
                && newest_migration > oldest_cache
            {
                result.warn(
                    "Migrations are newer than .sqlx/ cache",
                    "Run 'forge migrate prepare' to refresh the cache",
                );
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

    fn check_frontend(&self, result: &mut CheckResult) -> Result<()> {
        let frontend_dir = Path::new("frontend");
        if !frontend_dir.exists() {
            result.info("No frontend/ directory (backend-only project)");
            return Ok(());
        }

        println!();
        result.pass("frontend/ directory exists");
        let target = FrontendTarget::detect(frontend_dir).unwrap_or(FrontendTarget::SvelteKit);

        match target {
            FrontendTarget::SvelteKit => {
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

                if frontend_dir.join("node_modules").exists() {
                    result.pass("Frontend dependencies installed");
                } else {
                    result.warn(
                        "Frontend dependencies not installed",
                        "Run 'cd frontend && bun install'",
                    );
                }
            }
            FrontendTarget::Dioxus => {
                if frontend_dir.join("Cargo.toml").exists() {
                    result.pass("Dioxus Cargo.toml found");
                } else {
                    result.fail(
                        "frontend/Cargo.toml not found",
                        "Add a Dioxus frontend crate in frontend/",
                    );
                }

                if frontend_dir.join("Dioxus.toml").exists() {
                    result.pass("Dioxus.toml found");
                } else {
                    result.fail(
                        "frontend/Dioxus.toml not found",
                        "Create frontend/Dioxus.toml for dx build/serve",
                    );
                }
            }
        }

        Ok(())
    }

    fn check_generated_bindings(&self, result: &mut CheckResult) -> Result<()> {
        let frontend_dir = Path::new("frontend");
        if !frontend_dir.exists() {
            result.info("No frontend/ directory, skipping binding check");
            return Ok(());
        }

        let target = FrontendTarget::detect(frontend_dir).unwrap_or(FrontendTarget::SvelteKit);
        let output_dir = target.default_output_dir();
        let output_path = Path::new(output_dir);

        if !output_path.exists() {
            result.warn(
                "Generated bindings directory not found",
                &format!("Run 'forge generate' to create {}", output_dir),
            );
            return Ok(());
        }

        let src_path = Path::new("src");
        let registry = if src_path.exists() {
            match forge_codegen::parse_project(src_path) {
                Ok(r) => r,
                Err(e) => {
                    result.warn(
                        &format!("Could not parse source: {}", e),
                        "Fix source errors and re-run",
                    );
                    return Ok(());
                }
            }
        } else {
            forge_core::schema::SchemaRegistry::new()
        };

        let has_schema = !registry.all_tables().is_empty()
            || !registry.all_enums().is_empty()
            || !registry.all_functions().is_empty();

        let tmp_dir = frontend_dir.join(format!("forge-check-{}", std::process::id()));
        let tmp_output = tmp_dir.join("bindings");
        std::fs::create_dir_all(&tmp_output)?;
        let tmp_output_str = tmp_output.to_string_lossy().to_string();

        let gen_result = target.generate_bindings(&BindingGeneratorInput {
            output_dir: &tmp_output_str,
            output_path: &tmp_output,
            registry: &registry,
            has_schema,
            force: true,
        });

        let cleanup = || {
            let _ = std::fs::remove_dir_all(&tmp_dir);
        };

        if let Err(e) = gen_result {
            cleanup();
            result.warn(
                &format!("Could not regenerate bindings: {}", e),
                "Run 'forge generate' to check manually",
            );
            return Ok(());
        }

        if let Err(e) =
            format_generated_bindings_for_check(target, frontend_dir, output_path, &tmp_output)
        {
            cleanup();
            result.warn(
                &format!("Could not format regenerated bindings: {}", e),
                "Run 'forge generate --force' to restore generated bindings",
            );
            return Ok(());
        }

        let mut modified = Vec::new();
        let mut missing = Vec::new();

        if let Ok(entries) = std::fs::read_dir(&tmp_output) {
            for entry in entries.flatten() {
                if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                    continue;
                }
                let filename = entry.file_name();
                let Ok(expected) = std::fs::read(entry.path()) else {
                    continue;
                };
                let on_disk = output_path.join(&filename);

                if !on_disk.exists() {
                    missing.push(filename.to_string_lossy().to_string());
                    continue;
                }

                let Ok(actual) = std::fs::read(&on_disk) else {
                    missing.push(filename.to_string_lossy().to_string());
                    continue;
                };

                if actual != expected {
                    modified.push(filename.to_string_lossy().to_string());
                }
            }
        }

        cleanup();

        if modified.is_empty() && missing.is_empty() {
            result.pass("Generated bindings are up to date");
        } else {
            if !modified.is_empty() {
                result.warn(
                    &format!(
                        "{} binding file(s) modified: {}",
                        modified.len(),
                        modified.join(", ")
                    ),
                    "Run 'forge generate --force' to restore generated bindings",
                );
            }
            if !missing.is_empty() {
                result.warn(
                    &format!(
                        "{} binding file(s) missing: {}",
                        missing.len(),
                        missing.join(", ")
                    ),
                    "Run 'forge generate' to recreate missing bindings",
                );
            }
        }

        Ok(())
    }

    async fn check_rust_linting(&self, result: &mut CheckResult) {
        println!();

        // Check cargo fmt
        let fmt_result = TokioCommand::new("cargo")
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
        let clippy_output = TokioCommand::new("cargo")
            .args(["clippy", "--", "-D", "warnings"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await;

        match clippy_output {
            Ok(output) if output.status.success() => {
                result.pass("cargo clippy check passed");
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                result.fail(
                    "Clippy warnings found",
                    "Run 'cargo clippy' to see warnings",
                );
                if !stderr.is_empty() {
                    eprintln!("{}", stderr);
                }
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
        let target = FrontendTarget::detect(frontend_dir).unwrap_or(FrontendTarget::SvelteKit);

        println!();

        if target == FrontendTarget::Dioxus {
            // Use rustfmt directly to avoid cargo fmt dependency resolution issues
            let mut rs_files = Vec::new();
            if let Ok(entries) = std::fs::read_dir(frontend_dir.join("src")) {
                collect_rs_files(entries, &mut rs_files);
            }

            if !rs_files.is_empty() {
                let mut cmd = TokioCommand::new("rustfmt");
                cmd.args(["--check", "--edition", "2024"]);
                for f in &rs_files {
                    cmd.arg(f);
                }
                let fmt_result = cmd
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .await;

                match fmt_result {
                    Ok(status) if status.success() => result.pass("Dioxus rustfmt check passed"),
                    Ok(_) => result.fail(
                        "Dioxus frontend formatting issues found",
                        "Run 'rustfmt --edition 2024 frontend/src/**/*.rs'",
                    ),
                    Err(_) => result.warn("Could not run rustfmt", "Ensure rustfmt is installed"),
                }
            }
        }

        if !frontend_dir.join("node_modules").exists() {
            return;
        }

        if target == FrontendTarget::SvelteKit {
            let eslint_result = TokioCommand::new("bunx")
                .args(["eslint", "."])
                .current_dir(frontend_dir)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .await;

            match eslint_result {
                Ok(status) if status.success() => result.pass("ESLint check passed"),
                Ok(_) => result.fail(
                    "ESLint errors found",
                    "Run 'cd frontend && bunx eslint .' to see errors",
                ),
                Err(_) => result.warn(
                    "Could not run ESLint",
                    "Ensure eslint is installed in frontend/",
                ),
            }
        }

        let prettier_result = TokioCommand::new("bunx")
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

fn project_uses_compile_time_sqlx_macros(src_dir: &Path) -> Result<bool> {
    if !src_dir.exists() {
        return Ok(false);
    }

    for entry in std::fs::read_dir(src_dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            if project_uses_compile_time_sqlx_macros(&path)? {
                return Ok(true);
            }
            continue;
        }

        if !file_type.is_file() || path.extension().is_none_or(|ext| ext != "rs") {
            continue;
        }

        let content = std::fs::read_to_string(&path)?;
        if content.contains("sqlx::query!(")
            || content.contains("sqlx::query_as!(")
            || content.contains("sqlx::query_scalar!(")
            || content.contains("sqlx::query_file!(")
            || content.contains("sqlx::query_file_as!(")
        {
            return Ok(true);
        }
    }

    Ok(false)
}

fn inspect_sqlx_cache(sqlx_dir: &Path) -> Result<SqlxCacheCheck> {
    if !sqlx_dir.exists() {
        return Ok(SqlxCacheCheck::Missing);
    }

    let query_file_count = std::fs::read_dir(sqlx_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with("query-"))
        .count();

    if query_file_count == 0 {
        Ok(SqlxCacheCheck::Empty)
    } else {
        Ok(SqlxCacheCheck::Ready(query_file_count))
    }
}

fn format_generated_bindings_for_check(
    target: FrontendTarget,
    frontend_dir: &Path,
    output_path: &Path,
    tmp_output: &Path,
) -> Result<()> {
    if target != FrontendTarget::SvelteKit {
        return Ok(());
    }

    if generated_bindings_are_prettier_ignored(frontend_dir, output_path)? {
        return Ok(());
    }

    let prettier_target = tmp_output
        .canonicalize()
        .unwrap_or_else(|_| tmp_output.to_path_buf());

    let local_prettier = frontend_dir
        .join("node_modules/.bin/prettier")
        .canonicalize()
        .ok();
    let mut prettier = if let Some(local_prettier) = local_prettier {
        let mut cmd = StdCommand::new(local_prettier);
        cmd.arg("--write");
        cmd
    } else {
        let mut cmd = StdCommand::new("bunx");
        cmd.args(["prettier", "--write"]);
        cmd
    };

    let status = prettier
        .arg(prettier_target.to_string_lossy().to_string())
        .current_dir(frontend_dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;

    if status.success() {
        Ok(())
    } else {
        anyhow::bail!("bunx prettier --write failed for temporary generated bindings")
    }
}

fn generated_bindings_are_prettier_ignored(
    frontend_dir: &Path,
    output_path: &Path,
) -> Result<bool> {
    let ignore_path = frontend_dir.join(".prettierignore");
    if !ignore_path.exists() {
        return Ok(false);
    }

    let relative_output = output_path
        .strip_prefix(frontend_dir)
        .unwrap_or(output_path)
        .to_string_lossy()
        .replace('\\', "/");
    let content = std::fs::read_to_string(ignore_path)?;

    for line in content.lines() {
        let pattern = line.trim().trim_end_matches('/');
        if pattern.is_empty() || pattern.starts_with('#') {
            continue;
        }

        if relative_output == pattern || relative_output.starts_with(&format!("{pattern}/")) {
            return Ok(true);
        }
    }

    Ok(false)
}

fn collect_rs_files(entries: std::fs::ReadDir, out: &mut Vec<std::path::PathBuf>) {
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Ok(sub) = std::fs::read_dir(&path) {
                collect_rs_files(sub, out);
            }
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
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

    #[test]
    fn test_detect_compile_time_sqlx_macros() {
        let dir = tempfile::tempdir().unwrap();
        let src_dir = dir.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::write(
            src_dir.join("queries.rs"),
            r#"fn demo() { let _ = sqlx::query!("SELECT 1"); }"#,
        )
        .unwrap();

        assert!(project_uses_compile_time_sqlx_macros(&src_dir).unwrap());
    }

    #[test]
    fn test_ignore_runtime_sqlx_calls() {
        let dir = tempfile::tempdir().unwrap();
        let src_dir = dir.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::write(
            src_dir.join("queries.rs"),
            r#"fn demo() { let _ = sqlx::query("SELECT 1"); }"#,
        )
        .unwrap();

        assert!(!project_uses_compile_time_sqlx_macros(&src_dir).unwrap());
    }

    #[test]
    fn test_empty_sqlx_directory_is_detected() {
        let dir = tempfile::tempdir().unwrap();
        let sqlx_dir = dir.path().join(".sqlx");
        std::fs::create_dir_all(&sqlx_dir).unwrap();

        assert_eq!(
            inspect_sqlx_cache(&sqlx_dir).unwrap(),
            SqlxCacheCheck::Empty
        );
    }

    #[test]
    fn test_sqlx_directory_with_query_cache_is_detected() {
        let dir = tempfile::tempdir().unwrap();
        let sqlx_dir = dir.path().join(".sqlx");
        std::fs::create_dir_all(&sqlx_dir).unwrap();
        std::fs::write(sqlx_dir.join("query-demo.json"), "{}").unwrap();

        assert_eq!(
            inspect_sqlx_cache(&sqlx_dir).unwrap(),
            SqlxCacheCheck::Ready(1)
        );
    }
}
