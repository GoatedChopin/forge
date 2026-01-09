use anyhow::Result;
use clap::Parser;
use console::style;
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;
use tokio::signal;

/// Start the development environment.
///
/// Starts embedded PostgreSQL, compiles the backend, then runs both
/// backend and frontend servers with hot reload.
#[derive(Parser)]
pub struct DevCommand {
    /// Backend port (default: 8080)
    #[arg(short, long, default_value = "8080")]
    pub port: u16,

    /// Skip frontend (backend only)
    #[arg(long)]
    pub backend_only: bool,

    /// Don't open browser automatically
    #[arg(long)]
    pub no_open: bool,

    /// Skip embedded PostgreSQL (use external database)
    #[arg(long)]
    pub no_pg: bool,
}

impl DevCommand {
    /// Execute the dev command.
    pub async fn execute(self) -> Result<()> {
        println!();
        println!(
            "{} Starting FORGE development environment...",
            style("🚀").cyan()
        );
        println!();

        // Check if we're in a FORGE project
        if !Path::new("forge.toml").exists() {
            anyhow::bail!(
                "Not a FORGE project (forge.toml not found).\n\n\
                To create a new project:\n  forge new my-app --demo"
            );
        }

        // Require cargo
        if !check_tool_exists("cargo").await {
            eprintln!(
                "{} {} is required but not installed.",
                style("✗").red(),
                style("cargo").yellow()
            );
            eprintln!();
            eprintln!("Install Rust from: https://rustup.rs");
            std::process::exit(1);
        }
        println!("  {} cargo found", style("✓").green());

        // Check for bun if we need frontend
        let has_frontend = Path::new("frontend").exists() && !self.backend_only;
        if has_frontend && !check_tool_exists("bun").await {
            eprintln!(
                "{} {} is required for frontend but not installed.",
                style("✗").red(),
                style("bun").yellow()
            );
            eprintln!();
            eprintln!("Install Bun from: https://bun.sh");
            std::process::exit(1);
        }
        if has_frontend {
            println!("  {} bun found", style("✓").green());
        }

        // Load .env if present
        let mut database_url = std::env::var("DATABASE_URL").ok();
        if Path::new(".env").exists() {
            if let Ok(content) = std::fs::read_to_string(".env") {
                for line in content.lines() {
                    if let Some((key, value)) = line.split_once('=') {
                        let key = key.trim();
                        let value = value.trim();
                        if std::env::var(key).is_err() {
                            // SAFETY: Called at startup before spawning threads
                            unsafe { std::env::set_var(key, value) };
                        }
                        if key == "DATABASE_URL" && database_url.is_none() {
                            database_url = Some(value.to_string());
                        }
                    }
                }
            }
            println!("  {} Loaded .env", style("✓").green());
        }

        println!();

        // Start embedded PostgreSQL if needed
        #[cfg(feature = "embedded-db")]
        let _pg_handle = if !self.no_pg && database_url.is_none() {
            Some(self.start_embedded_postgres().await?)
        } else {
            if self.no_pg {
                println!(
                    "  {} Skipping embedded PostgreSQL (--no-pg)",
                    style("→").dim()
                );
            } else {
                println!("  {} Using DATABASE_URL from environment", style("→").dim());
            }
            None
        };

        #[cfg(not(feature = "embedded-db"))]
        {
            if !self.no_pg && database_url.is_none() {
                eprintln!(
                    "{} No DATABASE_URL set and embedded PostgreSQL not available.",
                    style("✗").red()
                );
                eprintln!();
                eprintln!("Options:");
                eprintln!("  1. Set DATABASE_URL in .env or environment");
                eprintln!(
                    "  2. Build forge with embedded-db feature: cargo install forgex --features embedded-db"
                );
                std::process::exit(1);
            }
        }

        // Build backend first
        println!();
        println!("  {} Compiling backend...", style("⋯").cyan());

        let build_status = Command::new("cargo")
            .args(["build"])
            .env(
                "RUST_LOG",
                std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()),
            )
            .status()
            .await?;

        if !build_status.success() {
            anyhow::bail!("Backend compilation failed");
        }
        println!("  {} Backend compiled successfully", style("✓").green());

        println!();

        // Start processes
        let mut handles = Vec::new();

        // Start backend with cargo-watch (or cargo run if not available)
        println!(
            "  {} Backend: http://localhost:{}",
            style("→").cyan(),
            self.port
        );
        println!(
            "  {} Dashboard: http://localhost:{}/_dashboard",
            style("→").cyan(),
            self.port
        );

        let backend_handle = if check_tool_exists("cargo-watch").await {
            start_backend_watch(self.port).await?
        } else {
            println!(
                "    {} Install cargo-watch for auto-reload: cargo install cargo-watch",
                style("tip:").dim()
            );
            start_backend(self.port).await?
        };
        handles.push(("backend", backend_handle));

        // Start frontend
        if has_frontend {
            // Check if dependencies are installed
            let node_modules = Path::new("frontend/node_modules");
            if !node_modules.exists() {
                println!();
                println!(
                    "  {} Installing frontend dependencies...",
                    style("⋯").cyan()
                );

                let status = Command::new("bun")
                    .args(["install"])
                    .current_dir("frontend")
                    .status()
                    .await?;

                if !status.success() {
                    anyhow::bail!("Failed to install frontend dependencies");
                }
                println!("  {} Frontend dependencies installed", style("✓").green());
            }

            println!("  {} Frontend: http://localhost:5173", style("→").cyan());

            let frontend_handle = start_frontend().await?;
            handles.push(("frontend", frontend_handle));
        }

        println!();
        println!(
            "{} Development server running. Press {} to stop.",
            style("✅").green(),
            style("Ctrl+C").yellow()
        );
        println!();

        // Open browser if requested
        if !self.no_open {
            let url = if has_frontend {
                "http://localhost:5173"
            } else {
                &format!("http://localhost:{}", self.port)
            };

            // Give servers time to start
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            let _ = open_browser(url);
        }

        // Wait for Ctrl+C
        signal::ctrl_c().await?;

        println!();
        println!("{} Shutting down...", style("⏹").yellow());

        // Kill all processes
        for (name, mut handle) in handles {
            if let Err(e) = handle.kill().await {
                eprintln!("  {} Failed to stop {}: {}", style("⚠").yellow(), name, e);
            }
        }

        println!("{} Stopped.", style("✅").green());
        Ok(())
    }

    /// Start embedded PostgreSQL and set DATABASE_URL.
    #[cfg(feature = "embedded-db")]
    async fn start_embedded_postgres(&self) -> Result<postgresql_embedded::PostgreSQL> {
        use std::path::PathBuf;

        println!("  {} Starting embedded PostgreSQL...", style("⋯").cyan());

        // Use pg_data/ in project directory
        let data_dir = PathBuf::from("pg_data");

        let mut settings = postgresql_embedded::Settings::default();
        settings.data_dir = data_dir;
        // Use port 5433 to avoid conflicts with system postgres
        settings.port = 5433;

        let mut pg = postgresql_embedded::PostgreSQL::new(settings);

        pg.setup()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to setup embedded PostgreSQL: {}", e))?;

        pg.start()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to start embedded PostgreSQL: {}", e))?;

        // Create the database if it doesn't exist
        let db_name = "forge_dev";
        let url = pg.settings().url(db_name);

        // Set DATABASE_URL for child processes
        std::env::set_var("DATABASE_URL", &url);

        println!(
            "  {} Embedded PostgreSQL running on port 5433",
            style("✓").green()
        );
        println!("    {} Data: ./pg_data/", style("→").dim());

        Ok(pg)
    }
}

async fn check_tool_exists(name: &str) -> bool {
    let result = Command::new("which")
        .arg(name)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;

    matches!(result, Ok(status) if status.success())
}

async fn start_backend_watch(port: u16) -> Result<tokio::process::Child> {
    // Watch src/, migrations/, Cargo.toml - explicitly exclude frontend/
    let child = Command::new("cargo")
        .args([
            "watch",
            "-x",
            &format!("run -- --port {}", port),
            "-w",
            "src",
            "-w",
            "migrations",
            "-w",
            "Cargo.toml",
            // Ignore frontend directory
            "-i",
            "frontend/",
        ])
        .env(
            "RUST_LOG",
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()),
        )
        .spawn()?;

    Ok(child)
}

async fn start_backend(port: u16) -> Result<tokio::process::Child> {
    let child = Command::new("cargo")
        .args(["run", "--", "--port", &port.to_string()])
        .env(
            "RUST_LOG",
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()),
        )
        .spawn()?;

    Ok(child)
}

async fn start_frontend() -> Result<tokio::process::Child> {
    let child = Command::new("bun")
        .args(["run", "dev"])
        .current_dir("frontend")
        .spawn()?;

    Ok(child)
}

fn open_browser(url: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(url).spawn()?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open").arg(url).spawn()?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dev_command_parse() {
        let cmd = DevCommand {
            port: 8080,
            backend_only: false,
            no_open: false,
            no_pg: false,
        };
        assert_eq!(cmd.port, 8080);
    }

    #[test]
    fn test_dev_command_no_pg() {
        let cmd = DevCommand {
            port: 8080,
            backend_only: false,
            no_open: false,
            no_pg: true,
        };
        assert!(cmd.no_pg);
    }
}
