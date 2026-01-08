use anyhow::Result;
use clap::Parser;
use console::style;
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;
use tokio::signal;

/// Start the development environment.
#[derive(Parser)]
pub struct DevCommand {
    /// Backend port (default: 8080)
    #[arg(short, long, default_value = "8080")]
    pub port: u16,

    /// Skip frontend (backend only)
    #[arg(long)]
    pub backend_only: bool,

    /// Skip backend (frontend only)
    #[arg(long)]
    pub frontend_only: bool,

    /// Don't open browser automatically
    #[arg(long)]
    pub no_open: bool,
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
                To create a new project:\n  forge new my-app\n\n\
                To initialize in current directory:\n  forge init"
            );
        }

        // Load .env if present
        if Path::new(".env").exists() {
            if let Ok(content) = std::fs::read_to_string(".env") {
                for line in content.lines() {
                    if let Some((key, value)) = line.split_once('=') {
                        if std::env::var(key).is_err() {
                            std::env::set_var(key.trim(), value.trim());
                        }
                    }
                }
            }
            println!("  {} Loaded .env", style("✓").green());
        }

        let has_frontend = Path::new("frontend").exists() && !self.backend_only;
        let run_backend = !self.frontend_only;

        // Check for required tools
        if run_backend {
            check_tool("cargo", "Rust is required for the backend").await?;
        }
        if has_frontend && !self.frontend_only {
            check_tool("bun", "Bun is required for the frontend (https://bun.sh)").await?;
        }

        println!();

        // Start processes
        let mut handles = Vec::new();

        if run_backend {
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

            // Check if cargo-watch is available, fall back to cargo run
            let backend_handle = if check_tool_silent("cargo-watch").await {
                start_backend_watch(self.port).await?
            } else {
                println!(
                    "    {} Install cargo-watch for auto-reload: cargo install cargo-watch",
                    style("tip:").dim()
                );
                start_backend(self.port).await?
            };
            handles.push(("backend", backend_handle));
        }

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

            println!(
                "  {} Frontend: http://localhost:5173",
                style("→").cyan()
            );

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
}

async fn check_tool(name: &str, message: &str) -> Result<()> {
    let result = Command::new("which")
        .arg(name)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;

    match result {
        Ok(status) if status.success() => Ok(()),
        _ => {
            // Try 'where' on Windows
            let result = Command::new("where")
                .arg(name)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .await;

            match result {
                Ok(status) if status.success() => Ok(()),
                _ => anyhow::bail!("{} not found. {}", name, message),
            }
        }
    }
}

async fn check_tool_silent(name: &str) -> bool {
    let result = Command::new("which")
        .arg(name)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;

    matches!(result, Ok(status) if status.success())
}

async fn start_backend_watch(port: u16) -> Result<tokio::process::Child> {
    let child = Command::new("cargo")
        .args([
            "watch",
            "-x",
            &format!("run -- --port {}", port),
            "-w",
            "src",
            "-w",
            "Cargo.toml",
        ])
        .env("RUST_LOG", std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()))
        .spawn()?;

    Ok(child)
}

async fn start_backend(port: u16) -> Result<tokio::process::Child> {
    let child = Command::new("cargo")
        .args(["run", "--", "--port", &port.to_string()])
        .env("RUST_LOG", std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()))
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
        std::process::Command::new("open")
            .arg(url)
            .spawn()?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()?;
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/c", "start", url])
            .spawn()?;
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
            frontend_only: false,
            no_open: false,
        };
        assert_eq!(cmd.port, 8080);
    }
}
