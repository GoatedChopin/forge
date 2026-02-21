use anyhow::Result;
use clap::{Parser, Subcommand};
use console::style;
use std::path::Path;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::signal;

use super::ui;

/// Start the development environment with Docker Compose.
#[derive(Parser)]
pub struct DevCommand {
    #[command(subcommand)]
    pub action: Option<DevAction>,
}

#[derive(Subcommand)]
pub enum DevAction {
    /// Stop the development environment
    Down {
        /// Clear volumes and target/ directory
        #[arg(long)]
        clear: bool,
    },
}

impl DevCommand {
    pub async fn execute(self) -> Result<()> {
        if !Path::new("forge.toml").exists() {
            anyhow::bail!(
                "Not a FORGE project (forge.toml not found).\n\n\
                To create a new project:\n  forge new my-app --demo"
            );
        }

        if !check_tool_exists("docker").await {
            eprintln!(
                "{} {} is required but not installed.",
                ui::error(),
                style("docker").yellow()
            );
            eprintln!();
            eprintln!("Install Docker from: https://docs.docker.com/get-docker/");
            std::process::exit(1);
        }

        match self.action {
            Some(DevAction::Down { clear }) => self.down(clear).await,
            None => self.up().await,
        }
    }

    async fn up(&self) -> Result<()> {
        ui::section("FORGE Dev");
        println!("  {} Starting development environment...", ui::tool());
        println!();
        println!("  {} Running: docker compose up --build", ui::step());
        println!();

        let mut child = Command::new("docker")
            .args(["compose", "--progress", "plain", "up", "--build"])
            .stdin(Stdio::inherit())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let stdout = child.stdout.take().expect("stdout piped");
        let stderr = child.stderr.take().expect("stderr piped");

        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                println!("{}", line);
            }
        });

        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                eprintln!("{}", line);
            }
        });

        tokio::select! {
            status = child.wait() => {
                match status {
                    Ok(status) if status.success() => Ok(()),
                    Ok(_) => anyhow::bail!("docker compose up failed"),
                    Err(e) => Err(e.into()),
                }
            }
            _ = signal::ctrl_c() => {
                println!();
                println!("{} Stopping containers...", ui::stop());

                #[cfg(unix)]
                if let Some(id) = child.id() {
                    use nix::sys::signal::{Signal, kill};
                    use nix::unistd::Pid;
                    let _ = kill(Pid::from_raw(id as i32), Signal::SIGTERM);
                }

                let _ = child.wait().await;

                let _ = Command::new("docker")
                    .args(["compose", "down"])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .await;

                println!("{} Development environment stopped.", ui::ok());
                Ok(())
            }
        }
    }

    async fn down(&self, clear: bool) -> Result<()> {
        println!();

        if clear {
            println!(
                "{} Stopping and cleaning FORGE development environment...",
                ui::step()
            );
            println!();
            println!("  {} Running: docker compose down -v", ui::step());

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

            let target_dir = Path::new("target");
            if target_dir.exists() {
                println!("  {} Removing target/...", ui::step());
                std::fs::remove_dir_all(target_dir)?;
            }

            println!();
            println!("{} Development environment stopped and cleaned.", ui::ok());
        } else {
            println!("{} Stopping FORGE development environment...", ui::stop());
            println!();
            println!("  {} Running: docker compose down", ui::step());

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
            println!("{} Development environment stopped.", ui::ok());
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn test_dev_command_parse() {
        let cmd = DevCommand { action: None };
        assert!(cmd.action.is_none());
    }

    #[test]
    fn test_dev_command_down() {
        let cmd = DevCommand {
            action: Some(DevAction::Down { clear: false }),
        };
        assert!(matches!(cmd.action, Some(DevAction::Down { clear: false })));
    }
}
