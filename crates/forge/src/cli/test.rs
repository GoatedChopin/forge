use anyhow::Result;
use clap::Parser;
use console::style;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;

use super::ui;

/// Run project tests (backend unit tests and frontend Playwright tests).
///
/// Runs both suites by default. If the dev server is not running,
/// it starts docker compose, runs the tests, then tears it down.
#[derive(Parser)]
pub struct TestCommand {
    /// Skip backend unit tests
    #[arg(long)]
    pub skip_backend: bool,

    /// Skip frontend Playwright tests
    #[arg(long)]
    pub skip_frontend: bool,

    /// Run Playwright in interactive UI mode
    #[arg(long)]
    pub ui: bool,

    /// Run tests in a visible browser window
    #[arg(long)]
    pub headed: bool,

    /// Extra arguments passed through to the test runner
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub args: Vec<String>,
}

impl TestCommand {
    pub async fn execute(self) -> Result<()> {
        if !Path::new("forge.toml").exists() {
            anyhow::bail!(
                "Not a FORGE project (forge.toml not found).\n\n\
                To create a new project:\n  forge new my-app --template with-svelte/minimal"
            );
        }

        ui::section("FORGE Test");

        let mut any_failed = false;

        if !self.skip_backend && !self.run_backend_tests().await? {
            any_failed = true;
        }

        if !self.skip_frontend {
            let result = self.run_frontend_tests().await;
            match result {
                Ok(passed) => {
                    if !passed {
                        any_failed = true;
                    }
                }
                Err(e) => return Err(e),
            }
        }

        println!();
        if any_failed {
            println!("{} Some tests failed.", ui::error());
            std::process::exit(1);
        } else {
            println!("{} All tests passed.", ui::ok());
        }

        Ok(())
    }

    async fn run_backend_tests(&self) -> Result<bool> {
        println!();
        println!("  {} {}", ui::step(), style("Backend Tests").bold());

        let mut cargo_args = vec!["test"];

        if self.skip_frontend {
            for arg in &self.args {
                cargo_args.push(arg);
            }
        }

        println!("  {} Running: cargo {}", ui::step(), cargo_args.join(" "));
        println!();

        let status = Command::new("cargo")
            .args(&cargo_args)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .await?;

        if status.success() {
            println!();
            println!("  {} Backend tests passed.", ui::ok());
            Ok(true)
        } else {
            println!();
            println!("  {} Backend tests failed.", ui::error());
            Ok(false)
        }
    }

    async fn run_frontend_tests(&self) -> Result<bool> {
        let frontend_dir = Path::new("frontend");
        if !frontend_dir.exists() {
            println!();
            println!(
                "  {} No frontend/ directory, skipping frontend tests.",
                ui::info()
            );
            return Ok(true);
        }

        let tests_dir = frontend_dir.join("tests");
        if !tests_dir.exists() {
            println!();
            println!(
                "  {} No frontend/tests/ directory, skipping frontend tests.",
                ui::info()
            );
            return Ok(true);
        }

        println!();
        println!("  {} {}", ui::step(), style("Frontend Tests").bold());

        // Check if we need to start the dev environment
        let mut started_compose = false;

        print!("  {} Checking backend...", ui::step());
        if check_backend_health().await {
            println!(" {}", style("ready").green());
        } else {
            println!(" {}", style("not running").yellow());
            started_compose = self.start_dev_environment().await?;
        }

        // Run tests, tearing down compose if we started it
        let result = self.execute_frontend_tests(frontend_dir).await;

        if started_compose {
            self.stop_dev_environment().await;
        }

        result
    }

    async fn start_dev_environment(&self) -> Result<bool> {
        if !check_docker_available().await {
            anyhow::bail!(
                "Backend is not running and Docker is not available.\n\n\
                Either start the backend manually or install Docker."
            );
        }

        println!("  {} Starting dev environment for tests...", ui::step());

        let status = Command::new("docker")
            .args(["compose", "up", "-d", "--build"])
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .status()
            .await?;

        if !status.success() {
            anyhow::bail!("Failed to start dev environment with docker compose");
        }

        // Wait for backend to become healthy
        print!("  {} Waiting for backend...", ui::step());
        let healthy = wait_for_backend_health(Duration::from_secs(120)).await;
        if healthy {
            println!(" {}", style("ready").green());
        } else {
            println!(" {}", style("timed out").red());
            // Tear down since we started it
            self.stop_dev_environment().await;
            anyhow::bail!(
                "Backend did not become healthy within 120s.\n\
                Check docker compose logs for details."
            );
        }

        Ok(true)
    }

    async fn stop_dev_environment(&self) {
        println!();
        println!("  {} Stopping dev environment...", ui::step());
        // Use -v to remove volumes so the next test run starts with a clean DB
        let _ = Command::new("docker")
            .args(["compose", "down", "-v"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;
    }

    async fn execute_frontend_tests(&self, frontend_dir: &Path) -> Result<bool> {
        // Check dependencies are installed
        if !frontend_dir.join("node_modules").exists() {
            println!("  {} Installing frontend dependencies...", ui::step());
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
            println!("  {} Installing Playwright browsers...", ui::step());
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

        // Wait for the frontend dev server (Docker or manual).
        // Playwright's reuseExistingServer won't work if Docker has the port
        // mapped but the process inside isn't ready yet.
        print!("  {} Checking frontend...", ui::step());
        if check_frontend_health().await {
            println!(" {}", style("ready").green());
        } else {
            let frontend_ready = wait_for_frontend_health(Duration::from_secs(90)).await;
            if frontend_ready {
                println!(" {}", style("ready").green());
            } else {
                println!(" {}", style("will be started by Playwright").dim());
            }
        }

        // Build Playwright command
        let mut pw_args = vec!["playwright", "test"];

        if self.ui {
            pw_args.push("--ui");
        }

        if self.headed {
            pw_args.push("--headed");
        }

        if self.skip_backend {
            for arg in &self.args {
                pw_args.push(arg);
            }
        }

        println!();
        println!("  {} Running: bunx {}", ui::step(), pw_args.join(" "));
        println!();

        let status = Command::new("bunx")
            .args(&pw_args)
            .current_dir(frontend_dir)
            .env("VITE_API_URL", backend_base_url())
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .await?;

        if status.success() {
            println!();
            println!("  {} Frontend tests passed.", ui::ok());
            Ok(true)
        } else {
            println!();
            println!("  {} Frontend tests failed.", ui::error());
            println!(
                "  Debug with: {} or {}",
                style("forge test --skip-backend --ui").cyan(),
                style("forge test --skip-backend --headed").cyan()
            );
            Ok(false)
        }
    }
}

fn backend_base_url_from_env(get_env: impl Fn(&str) -> Option<String>) -> String {
    get_env("VITE_API_URL")
        .or_else(|| get_env("PUBLIC_API_URL"))
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "http://localhost:9081".to_string())
}

fn backend_base_url() -> String {
    backend_base_url_from_env(|key| std::env::var(key).ok())
}

async fn check_backend_health() -> bool {
    let base_url = backend_base_url();
    let urls = [
        format!("{base_url}/_api/health"),
        format!("{base_url}/_api/ready"),
    ];

    for url in &urls {
        let result = Command::new("curl")
            .args(["-sf", "--max-time", "2", url.as_str()])
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

async fn wait_for_backend_health(timeout: Duration) -> bool {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if check_backend_health().await {
            return true;
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
        print!(".");
    }
    false
}

async fn wait_for_frontend_health(timeout: Duration) -> bool {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if check_frontend_health().await {
            return true;
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
        print!(".");
    }
    false
}

async fn check_frontend_health() -> bool {
    let result = Command::new("curl")
        .args(["-sf", "--max-time", "2", "http://localhost:9080"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;

    matches!(result, Ok(status) if status.success())
}

async fn check_docker_available() -> bool {
    let result = Command::new("docker")
        .args(["info"])
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

    fn default_cmd() -> TestCommand {
        TestCommand {
            skip_backend: false,
            skip_frontend: false,
            ui: false,
            headed: false,
            args: vec![],
        }
    }

    #[test]
    fn test_command_default_runs_both() {
        let cmd = default_cmd();
        assert!(!cmd.skip_backend);
        assert!(!cmd.skip_frontend);
    }

    #[test]
    fn test_command_skip_backend() {
        let cmd = TestCommand {
            skip_backend: true,
            ..default_cmd()
        };
        assert!(cmd.skip_backend);
        assert!(!cmd.skip_frontend);
    }

    #[test]
    fn test_command_skip_frontend() {
        let cmd = TestCommand {
            skip_frontend: true,
            ..default_cmd()
        };
        assert!(!cmd.skip_backend);
        assert!(cmd.skip_frontend);
    }

    #[test]
    fn test_command_with_ui_and_args() {
        let cmd = TestCommand {
            ui: true,
            args: vec!["tests/todo.spec.ts".into()],
            ..default_cmd()
        };
        assert!(cmd.ui);
        assert_eq!(cmd.args.len(), 1);
    }

    #[test]
    fn test_command_headed() {
        let cmd = TestCommand {
            headed: true,
            ..default_cmd()
        };
        assert!(cmd.headed);
    }

    #[test]
    fn test_backend_base_url_prefers_vite_api_url() {
        let url = backend_base_url_from_env(|key| match key {
            "VITE_API_URL" => Some("http://localhost:19080".into()),
            "PUBLIC_API_URL" => Some("http://localhost:18080".into()),
            _ => None,
        });

        assert_eq!(url, "http://localhost:19080");
    }

    #[test]
    fn test_backend_base_url_falls_back_to_public_api_url() {
        let url = backend_base_url_from_env(|key| match key {
            "PUBLIC_API_URL" => Some("http://localhost:18080".into()),
            _ => None,
        });

        assert_eq!(url, "http://localhost:18080");
    }

    #[test]
    fn test_backend_base_url_defaults_to_localhost() {
        let url = backend_base_url_from_env(|_| None);

        assert_eq!(url, "http://localhost:9081");
    }
}
