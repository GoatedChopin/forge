use anyhow::Result;
use clap::Parser;
use console::style;
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;

use super::ui;

/// Run Playwright tests for the frontend.
///
/// Checks that the backend and frontend are reachable, installs
/// Playwright browsers if needed, then runs the test suite.
#[derive(Parser)]
pub struct TestCommand {
    /// Run Playwright in interactive UI mode
    #[arg(long)]
    pub ui: bool,

    /// Extra arguments passed through to Playwright (e.g. file patterns, --headed)
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub args: Vec<String>,
}

impl TestCommand {
    pub async fn execute(self) -> Result<()> {
        if !Path::new("forge.toml").exists() {
            anyhow::bail!(
                "Not a FORGE project (forge.toml not found).\n\n\
                To create a new project:\n  forge new my-app --demo"
            );
        }

        let frontend_dir = Path::new("frontend");
        if !frontend_dir.exists() {
            anyhow::bail!("No frontend/ directory found. Nothing to test.");
        }

        let tests_dir = frontend_dir.join("tests");
        if !tests_dir.exists() {
            anyhow::bail!(
                "No frontend/tests/ directory found.\n\n\
                Create Playwright tests in frontend/tests/ to get started."
            );
        }

        ui::section("FORGE Test");

        // Check dependencies are installed
        if !frontend_dir.join("node_modules").exists() {
            println!(
                "  {} Installing frontend dependencies...",
                ui::step()
            );
            let status = Command::new("bun")
                .args(["install"])
                .current_dir(frontend_dir)
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .status()
                .await?;

            if !status.success() {
                anyhow::bail!("Failed to install frontend dependencies");
            }
        }

        // Check Playwright browsers are installed
        let pw_check = Command::new("bunx")
            .args(["playwright", "test", "--list"])
            .current_dir(frontend_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;

        let needs_install = match pw_check {
            Ok(status) => !status.success(),
            Err(_) => true,
        };

        if needs_install {
            println!(
                "  {} Installing Playwright browsers...",
                ui::step()
            );
            let status = Command::new("bunx")
                .args(["playwright", "install", "chromium"])
                .current_dir(frontend_dir)
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .status()
                .await?;

            if !status.success() {
                anyhow::bail!("Failed to install Playwright browsers");
            }
        }

        // Check backend health
        print!("  {} Checking backend...", ui::step());
        let backend_ready = check_backend_health().await;
        if backend_ready {
            println!(" {}", style("ready").green());
        } else {
            println!(" {}", style("not reachable").red());
            println!();
            println!(
                "  {} Backend is not running. Start it first:",
                ui::warn()
            );
            println!("    {} {}", ui::bullet(), style("forge dev").cyan());
            println!(
                "    {} {}",
                ui::bullet(),
                style("cargo run (for local dev)").cyan()
            );
            println!();
            anyhow::bail!("Backend must be running before tests can execute");
        }

        // Check frontend dev server
        print!("  {} Checking frontend...", ui::step());
        let frontend_ready = check_frontend_health().await;
        if frontend_ready {
            println!(" {}", style("ready").green());
        } else {
            println!(" {}", style("not reachable").yellow());
            println!(
                "  {} Frontend dev server not detected. Playwright will start one.",
                ui::info()
            );
        }

        // Build Playwright command
        let mut pw_args = vec!["playwright", "test"];

        if self.ui {
            pw_args.push("--ui");
        }

        for arg in &self.args {
            pw_args.push(arg);
        }

        println!();
        println!(
            "  {} Running: bunx {}",
            ui::step(),
            pw_args.join(" ")
        );
        println!();

        let status = Command::new("bunx")
            .args(&pw_args)
            .current_dir(frontend_dir)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .await?;

        println!();
        if status.success() {
            println!("{} All tests passed.", ui::ok());
        } else {
            println!("{} Some tests failed.", ui::error());
            println!();
            println!("  Debug with: {} or {}", style("forge test --ui").cyan(), style("forge test --headed").cyan());
            std::process::exit(1);
        }

        Ok(())
    }
}

async fn check_backend_health() -> bool {
    let urls = [
        "http://localhost:8080/_api/health",
        "http://localhost:8080/_api/ready",
    ];

    for url in &urls {
        let result = Command::new("curl")
            .args(["-sf", "--max-time", "2", url])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;

        if matches!(result, Ok(status) if status.success()) {
            return true;
        }
    }

    false
}

async fn check_frontend_health() -> bool {
    let result = Command::new("curl")
        .args(["-sf", "--max-time", "2", "http://localhost:5173"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;

    matches!(result, Ok(status) if status.success())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_command_default() {
        let cmd = TestCommand {
            ui: false,
            args: vec![],
        };
        assert!(!cmd.ui);
        assert!(cmd.args.is_empty());
    }

    #[test]
    fn test_command_with_args() {
        let cmd = TestCommand {
            ui: true,
            args: vec!["tests/todo.spec.ts".into(), "--headed".into()],
        };
        assert!(cmd.ui);
        assert_eq!(cmd.args.len(), 2);
    }
}
