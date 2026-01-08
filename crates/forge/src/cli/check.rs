use anyhow::Result;
use clap::Parser;
use console::style;
use sqlx::postgres::PgPoolOptions;
use sqlx::Row;
use std::path::Path;

/// Validate project setup and dependencies.
#[derive(Parser)]
pub struct CheckCommand {
    /// Path to forge.toml (default: ./forge.toml)
    #[arg(short, long, default_value = "forge.toml")]
    pub config: String,

    /// Check database connectivity
    #[arg(long, default_value = "true")]
    pub db: bool,
}

impl CheckCommand {
    /// Execute the check command.
    pub async fn execute(self) -> Result<()> {
        println!();
        println!(
            "{} Checking FORGE project...",
            style("🔍").cyan()
        );
        println!();

        let mut all_passed = true;
        let mut warnings = Vec::new();

        // Check 1: forge.toml exists
        let config_path = Path::new(&self.config);
        if config_path.exists() {
            println!("  {} forge.toml found", style("✓").green());

            // Try to parse it
            let content = std::fs::read_to_string(config_path)?;
            match toml::from_str::<toml::Value>(&content) {
                Ok(config) => {
                    println!("  {} forge.toml is valid TOML", style("✓").green());

                    // Check for required sections
                    if config.get("project").is_some() {
                        println!("  {} [project] section found", style("✓").green());
                    } else {
                        println!("  {} [project] section missing", style("⚠").yellow());
                        warnings.push("Add [project] section to forge.toml");
                    }

                    if config.get("database").is_some() {
                        println!("  {} [database] section found", style("✓").green());
                    } else {
                        println!("  {} [database] section missing", style("⚠").yellow());
                        warnings.push("Add [database] section to forge.toml");
                    }
                }
                Err(e) => {
                    println!("  {} forge.toml parse error: {}", style("✗").red(), e);
                    all_passed = false;
                }
            }
        } else {
            println!("  {} forge.toml not found", style("✗").red());
            println!("    {} Run 'forge init' to create one", style("→").dim());
            all_passed = false;
        }

        // Check 2: Directory structure
        let schema_dir = Path::new("src/schema");
        let functions_dir = Path::new("src/functions");
        let migrations_dir = Path::new("migrations");

        if schema_dir.exists() {
            println!("  {} src/schema/ directory exists", style("✓").green());
        } else {
            println!("  {} src/schema/ directory missing", style("✗").red());
            all_passed = false;
        }

        if functions_dir.exists() {
            println!("  {} src/functions/ directory exists", style("✓").green());
        } else {
            println!("  {} src/functions/ directory missing", style("✗").red());
            all_passed = false;
        }

        if migrations_dir.exists() {
            println!("  {} migrations/ directory exists", style("✓").green());

            // Count migrations
            let count = std::fs::read_dir(migrations_dir)?
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().map_or(false, |ext| ext == "sql"))
                .count();
            println!("    {} {} migration file(s) found", style("→").dim(), count);
        } else {
            println!("  {} migrations/ directory missing", style("⚠").yellow());
            warnings.push("Create migrations/ directory for database migrations");
        }

        // Check 3: .env file
        let env_path = Path::new(".env");
        if env_path.exists() {
            println!("  {} .env file found", style("✓").green());
        } else {
            println!("  {} .env file missing", style("⚠").yellow());
            warnings.push("Create .env file with DATABASE_URL");
        }

        // Check 4: DATABASE_URL environment variable
        match std::env::var("DATABASE_URL") {
            Ok(url) => {
                println!("  {} DATABASE_URL is set", style("✓").green());

                // Mask the password in the URL for display
                let masked = mask_database_url(&url);
                println!("    {} {}", style("→").dim(), style(masked).dim());

                // Check database connectivity if requested
                if self.db {
                    println!();
                    println!("  {} Checking database connection...", style("⋯").cyan());

                    match check_database_connection(&url).await {
                        Ok(version) => {
                            println!("  {} Database connection successful", style("✓").green());
                            println!("    {} PostgreSQL {}", style("→").dim(), version);
                        }
                        Err(e) => {
                            println!("  {} Database connection failed: {}", style("✗").red(), e);
                            all_passed = false;
                        }
                    }
                }
            }
            Err(_) => {
                // Try loading from .env
                if env_path.exists() {
                    if let Ok(content) = std::fs::read_to_string(env_path) {
                        for line in content.lines() {
                            if line.starts_with("DATABASE_URL=") {
                                println!("  {} DATABASE_URL found in .env", style("✓").green());
                                let url = line.trim_start_matches("DATABASE_URL=");
                                let masked = mask_database_url(url);
                                println!("    {} {}", style("→").dim(), style(masked).dim());

                                if self.db {
                                    println!();
                                    println!("  {} Checking database connection...", style("⋯").cyan());

                                    match check_database_connection(url).await {
                                        Ok(version) => {
                                            println!("  {} Database connection successful", style("✓").green());
                                            println!("    {} PostgreSQL {}", style("→").dim(), version);
                                        }
                                        Err(e) => {
                                            println!("  {} Database connection failed: {}", style("✗").red(), e);
                                            all_passed = false;
                                        }
                                    }
                                }
                                break;
                            }
                        }
                    }
                } else {
                    println!("  {} DATABASE_URL not set", style("⚠").yellow());
                    warnings.push("Set DATABASE_URL in .env or environment");
                }
            }
        }

        // Check 5: Frontend (if exists)
        let frontend_dir = Path::new("frontend");
        if frontend_dir.exists() {
            println!("  {} frontend/ directory exists", style("✓").green());

            let package_json = frontend_dir.join("package.json");
            if package_json.exists() {
                println!("  {} frontend/package.json found", style("✓").green());
            } else {
                println!("  {} frontend/package.json missing", style("⚠").yellow());
                warnings.push("Run 'cd frontend && bun install' to set up frontend");
            }

            let node_modules = frontend_dir.join("node_modules");
            if node_modules.exists() {
                println!("  {} frontend dependencies installed", style("✓").green());
            } else {
                println!("  {} frontend dependencies not installed", style("⚠").yellow());
                warnings.push("Run 'cd frontend && bun install' to install dependencies");
            }
        }

        // Summary
        println!();
        if all_passed && warnings.is_empty() {
            println!(
                "{} All checks passed! Ready to run.",
                style("✅").green()
            );
            println!();
            println!("Next steps:");
            println!("  {} Start development", style("forge dev").cyan());
            println!("  {} Check migrations", style("forge migrate status").cyan());
        } else if all_passed {
            println!(
                "{} Checks passed with {} warning(s)",
                style("⚠").yellow(),
                warnings.len()
            );
            println!();
            for warning in &warnings {
                println!("  {} {}", style("→").dim(), warning);
            }
        } else {
            println!(
                "{} Some checks failed. Fix the issues above and run 'forge check' again.",
                style("❌").red()
            );
            return Err(anyhow::anyhow!("Project check failed"));
        }

        println!();
        Ok(())
    }
}

fn mask_database_url(url: &str) -> String {
    // Mask password in postgres://user:password@host/db
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

    let row = sqlx::query("SELECT version()")
        .fetch_one(&pool)
        .await?;

    let version_str: String = row.get(0);

    // Extract just the version number
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
}
