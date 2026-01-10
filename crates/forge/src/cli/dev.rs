use anyhow::Result;
use clap::{Parser, Subcommand};
use console::style;
use std::path::Path;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::signal;
use tokio::sync::mpsc;

/// Start the development environment using Docker Compose.
#[derive(Parser)]
pub struct DevCommand {
    #[command(subcommand)]
    pub action: Option<DevAction>,

    /// Don't open browser automatically
    #[arg(long)]
    pub no_open: bool,
}

#[derive(Subcommand)]
pub enum DevAction {
    /// Stop the development environment
    Down {
        /// Remove volumes (clean database)
        #[arg(long)]
        clean: bool,
    },
}

impl DevCommand {
    /// Execute the dev command.
    pub async fn execute(self) -> Result<()> {
        // Check if we're in a FORGE project
        if !Path::new("forge.toml").exists() {
            anyhow::bail!(
                "Not a FORGE project (forge.toml not found).\n\n\
                To create a new project:\n  forge new my-app --demo"
            );
        }

        // Check for docker
        if !check_tool_exists("docker").await {
            eprintln!(
                "{} {} is required but not installed.",
                style("✗").red(),
                style("docker").yellow()
            );
            eprintln!();
            eprintln!("Install Docker from: https://docs.docker.com/get-docker/");
            std::process::exit(1);
        }

        match self.action {
            Some(DevAction::Down { clean }) => self.down(clean).await,
            None => self.up().await,
        }
    }

    /// Start the development environment with docker compose up --build.
    async fn up(&self) -> Result<()> {
        println!();
        println!(
            "{} Starting FORGE development environment...",
            style("🚀").cyan()
        );
        println!();

        println!("  {} docker found", style("✓").green());
        println!();

        println!(
            "  {} Running: docker compose up --build",
            style("→").cyan()
        );
        println!();

        // Spawn docker compose and capture output to detect frontend ready
        // Use --progress plain to avoid interactive progress display that requires a TTY
        let mut child = Command::new("docker")
            .args(["compose", "--progress", "plain", "up", "--build"])
            .stdin(Stdio::inherit())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let stdout = child.stdout.take().expect("stdout piped");
        let stderr = child.stderr.take().expect("stderr piped");

        // Channel to signal when frontend is ready
        let (tx, mut rx) = mpsc::channel::<()>(1);
        let no_open = self.no_open;

        // Spawn task to read stdout
        let stdout_tx = tx.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            let mut frontend_ready = false;
            while let Ok(Some(line)) = reader.next_line().await {
                println!("{}", line);
                // Detect Vite ready signal (e.g., "VITE v5.4.11  ready in 123 ms")
                if !frontend_ready && line.contains("ready in") && line.contains("VITE") {
                    frontend_ready = true;
                    let _ = stdout_tx.send(()).await;
                }
            }
        });

        // Spawn task to read stderr
        let stderr_tx = tx;
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            let mut frontend_ready = false;
            while let Ok(Some(line)) = reader.next_line().await {
                eprintln!("{}", line);
                // Also check stderr for Vite ready signal
                if !frontend_ready && line.contains("ready in") && line.contains("VITE") {
                    frontend_ready = true;
                    let _ = stderr_tx.send(()).await;
                }
            }
        });

        // Open browser when frontend is ready
        if !no_open {
            tokio::spawn(async move {
                // Wait for frontend ready signal or timeout after 5 minutes
                let timeout = tokio::time::Duration::from_secs(300);
                if tokio::time::timeout(timeout, rx.recv()).await.is_ok() {
                    // Small delay to ensure server is fully ready
                    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                    let _ = open_browser("http://localhost:5173");
                }
            });
        }

        // Wait for docker compose to exit or Ctrl+C
        tokio::select! {
            status = child.wait() => {
                match status {
                    Ok(status) if status.success() => Ok(()),
                    Ok(_) => {
                        anyhow::bail!("docker compose up failed");
                    }
                    Err(e) => Err(e.into()),
                }
            }
            _ = signal::ctrl_c() => {
                println!();
                println!(
                    "{} Stopping containers gracefully...",
                    style("⏹").cyan()
                );

                // Send SIGTERM to docker compose process
                if let Some(id) = child.id() {
                    use nix::sys::signal::{kill, Signal};
                    use nix::unistd::Pid;
                    let _ = kill(Pid::from_raw(id as i32), Signal::SIGTERM);
                }

                // Wait for docker compose to finish gracefully
                let _ = child.wait().await;

                // Run docker compose down to ensure clean shutdown
                let _ = Command::new("docker")
                    .args(["compose", "down"])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .await;

                println!(
                    "{} Development environment stopped.",
                    style("✅").green()
                );
                Ok(())
            }
        }
    }

    /// Stop the development environment.
    async fn down(&self, clean: bool) -> Result<()> {
        println!();
        if clean {
            println!(
                "{} Stopping and cleaning FORGE development environment...",
                style("🧹").cyan()
            );
            println!();
            println!(
                "  {} Running: docker compose down -v",
                style("→").cyan()
            );

            let status = Command::new("docker")
                .args(["compose", "down", "-v"])
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .status()
                .await?;

            if !status.success() {
                anyhow::bail!("docker compose down -v failed");
            }

            println!();
            println!(
                "{} Development environment stopped and volumes removed.",
                style("✅").green()
            );
        } else {
            println!(
                "{} Stopping FORGE development environment...",
                style("⏹").cyan()
            );
            println!();
            println!(
                "  {} Running: docker compose down",
                style("→").cyan()
            );

            let status = Command::new("docker")
                .args(["compose", "down"])
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .status()
                .await?;

            if !status.success() {
                anyhow::bail!("docker compose down failed");
            }

            println!();
            println!(
                "{} Development environment stopped.",
                style("✅").green()
            );
        }

        Ok(())
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
            action: None,
            no_open: false,
        };
        assert!(!cmd.no_open);
    }

    #[test]
    fn test_dev_command_down() {
        let cmd = DevCommand {
            action: Some(DevAction::Down { clean: false }),
            no_open: false,
        };
        assert!(matches!(cmd.action, Some(DevAction::Down { clean: false })));
    }

    #[test]
    fn test_dev_command_down_clean() {
        let cmd = DevCommand {
            action: Some(DevAction::Down { clean: true }),
            no_open: false,
        };
        assert!(matches!(cmd.action, Some(DevAction::Down { clean: true })));
    }
}
