use anyhow::Result;
use clap::{Parser, Subcommand};
use console::style;
use std::path::Path;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::signal;
use tokio::sync::mpsc;

/// Start the development environment.
///
/// By default, runs with cargo and bun (bare metal). Use --docker to run with Docker Compose.
#[derive(Parser)]
pub struct DevCommand {
    #[command(subcommand)]
    pub action: Option<DevAction>,

    /// Don't open browser automatically
    #[arg(long)]
    pub no_open: bool,

    /// Run with Docker Compose instead of cargo/bun
    #[arg(long)]
    pub docker: bool,
}

#[derive(Subcommand)]
pub enum DevAction {
    /// Stop the development environment
    Down {
        /// Clear target/ and pg_data/ directories
        #[arg(long)]
        clear: bool,
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

        if self.docker {
            // Docker mode
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
                Some(DevAction::Down { clear }) => self.down_docker(clear).await,
                None => self.up_docker().await,
            }
        } else {
            // Bare metal mode (default)
            match self.action {
                Some(DevAction::Down { clear }) => self.down_bare_metal(clear).await,
                None => self.up_bare_metal().await,
            }
        }
    }

    /// Start the development environment with cargo and bun (bare metal).
    async fn up_bare_metal(&self) -> Result<()> {
        use postgresql_embedded::{PostgreSQL, Settings};

        println!();
        println!(
            "{} Starting FORGE development environment...",
            style("🚀").cyan()
        );
        println!();

        // Check prerequisites
        println!("  {} Checking prerequisites...", style("→").cyan());

        // Check cargo version
        let cargo_version = match get_tool_version("cargo", &["--version"]).await {
            Ok(v) => v,
            Err(_) => {
                eprintln!("    {} cargo not found", style("✗").red());
                eprintln!();
                eprintln!("Install Rust from: https://rustup.rs/");
                eprintln!();
                eprintln!(
                    "Alternatively, use {} to run with Docker.",
                    style("forge dev --docker").cyan()
                );
                std::process::exit(1);
            }
        };
        if !check_version(&cargo_version, 1, 92) {
            eprintln!(
                "    {} cargo {} found, but 92+ required",
                style("✗").red(),
                cargo_version
            );
            eprintln!();
            eprintln!(
                "Alternatively, use {} to run with Docker.",
                style("forge dev --docker").cyan()
            );
            std::process::exit(1);
        }
        println!("    {} cargo {}", style("✓").green(), cargo_version);

        // Check cargo-watch
        if !check_tool_exists("cargo-watch").await {
            eprintln!(
                "    {} cargo-watch not found (required for hot reload)",
                style("✗").red()
            );
            eprintln!();
            eprintln!("Install with: cargo install cargo-watch");
            eprintln!();
            eprintln!(
                "Alternatively, use {} to run with Docker.",
                style("forge dev --docker").cyan()
            );
            std::process::exit(1);
        }
        println!("    {} cargo-watch", style("✓").green());

        // Check bun version
        let bun_version = match get_tool_version("bun", &["--version"]).await {
            Ok(v) => v,
            Err(_) => {
                eprintln!("    {} bun not found", style("✗").red());
                eprintln!();
                eprintln!("Install bun from: https://bun.sh/");
                eprintln!();
                eprintln!(
                    "Alternatively, use {} to run with Docker.",
                    style("forge dev --docker").cyan()
                );
                std::process::exit(1);
            }
        };
        if !check_version(&bun_version, 1, 3) {
            eprintln!(
                "    {} bun {} found, but 1.3+ required",
                style("✗").red(),
                bun_version
            );
            eprintln!();
            eprintln!(
                "Alternatively, use {} to run with Docker.",
                style("forge dev --docker").cyan()
            );
            std::process::exit(1);
        }
        println!("    {} bun {}", style("✓").green(), bun_version);

        println!();

        // Get project name from forge.toml
        let forge_toml = std::fs::read_to_string("forge.toml")?;
        let config: toml::Value = toml::from_str(&forge_toml)?;
        let project_name = config
            .get("project")
            .and_then(|p| p.get("name"))
            .and_then(|n| n.as_str())
            .unwrap_or("forge_dev");

        // Start embedded PostgreSQL
        println!("  {} Starting embedded PostgreSQL...", style("→").cyan());

        // Check if port 5432 is already in use
        if is_port_in_use(5432).await {
            anyhow::bail!("Port 5432 is already in use, unable to start embedded PostgreSQL");
        }

        // Store PostgreSQL data in project's pg_data directory
        let project_dir = std::env::current_dir()?;
        let pg_data_dir = project_dir.join("pg_data");

        let settings = Settings {
            installation_dir: pg_data_dir.join("installation"),
            data_dir: pg_data_dir.join("data"),
            port: 5432,
            username: "postgres".to_string(),
            password: "forge".to_string(),
            temporary: false,
            ..Default::default()
        };

        let mut pg = PostgreSQL::new(settings);
        pg.setup().await?;
        pg.start().await?;

        // Create database if not exists
        let db_url = "postgres://postgres:forge@localhost:5432/postgres".to_string();
        let pool = sqlx::PgPool::connect(&db_url).await?;
        let db_name = project_name.replace('-', "_");
        let _ = sqlx::query(&format!("CREATE DATABASE \"{}\"", db_name))
            .execute(&pool)
            .await;
        pool.close().await;

        let database_url = format!("postgres://postgres:forge@localhost:5432/{}", db_name);

        println!("    {} PostgreSQL running on port 5432", style("✓").green());
        println!();

        // Channel to signal when frontend is ready
        let (tx, mut rx) = mpsc::channel::<()>(1);
        let no_open = self.no_open;

        // Start backend with cargo-watch for hot reload
        println!(
            "  {} Starting backend with cargo-watch (hot reload enabled)...",
            style("→").cyan()
        );

        let mut backend = Command::new("cargo")
            .args([
                "watch",
                "--ignore",
                "frontend/*",
                "-x",
                "run --no-default-features",
            ])
            .env("DATABASE_URL", &database_url)
            .env("RUST_LOG", "info,forge_runtime::function::executor=trace")
            .env("HOST", "0.0.0.0")
            .env("PORT", "8080")
            .stdin(Stdio::inherit())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let backend_stdout = backend.stdout.take().expect("stdout piped");
        let backend_stderr = backend.stderr.take().expect("stderr piped");

        // Spawn task to read backend stdout
        tokio::spawn(async move {
            let mut reader = BufReader::new(backend_stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                println!("{} {}", style("[backend]").blue(), line);
            }
        });

        // Spawn task to read backend stderr
        tokio::spawn(async move {
            let mut reader = BufReader::new(backend_stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                eprintln!("{} {}", style("[backend]").blue(), line);
            }
        });

        // Wait for backend to be ready (up to 5 minutes for first compile)
        println!(
            "    {} Waiting for backend to be ready (compiling, this may take a few minutes)...",
            style("→").cyan()
        );
        let mut backend_ready = false;
        for _ in 0..300 {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            if let Ok(resp) = reqwest::get("http://localhost:8080/ready").await {
                if resp.status().is_success() {
                    println!(
                        "    {} Backend running on http://localhost:8080",
                        style("✓").green()
                    );
                    backend_ready = true;
                    break;
                }
            }
        }
        if !backend_ready {
            anyhow::bail!("Backend failed to start within 5 minutes");
        }

        println!();

        // Start frontend with bun
        println!("  {} Starting frontend with bun...", style("→").cyan());

        // Always run bun install to ensure dependencies are installed
        println!(
            "    {} Installing frontend dependencies...",
            style("→").cyan()
        );
        let status = Command::new("bun")
            .args(["install"])
            .current_dir("frontend")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await?;
        if !status.success() {
            anyhow::bail!("Failed to install frontend dependencies");
        }

        // Run svelte-kit sync to generate .svelte-kit directory if it doesn't exist
        let svelte_kit_dir = std::path::Path::new("frontend/.svelte-kit");
        if !svelte_kit_dir.exists() {
            println!("    {} Syncing SvelteKit types...", style("→").cyan());
            let status = Command::new("./node_modules/.bin/svelte-kit")
                .args(["sync"])
                .current_dir("frontend")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .await?;
            if !status.success() {
                anyhow::bail!("Failed to sync SvelteKit types");
            }
        }

        let mut frontend = Command::new("./node_modules/.bin/vite")
            .args(["dev", "--host", "0.0.0.0"])
            .current_dir("frontend")
            .stdin(Stdio::inherit())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let frontend_stdout = frontend.stdout.take().expect("stdout piped");
        let frontend_stderr = frontend.stderr.take().expect("stderr piped");

        // Spawn task to read frontend stdout
        let stdout_tx = tx.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(frontend_stdout).lines();
            let mut frontend_ready = false;
            while let Ok(Some(line)) = reader.next_line().await {
                println!("{} {}", style("[frontend]").magenta(), line);
                if !frontend_ready && line.contains("ready in") && line.contains("VITE") {
                    frontend_ready = true;
                    let _ = stdout_tx.send(()).await;
                }
            }
        });

        // Spawn task to read frontend stderr
        let stderr_tx = tx;
        tokio::spawn(async move {
            let mut reader = BufReader::new(frontend_stderr).lines();
            let mut frontend_ready = false;
            while let Ok(Some(line)) = reader.next_line().await {
                eprintln!("{} {}", style("[frontend]").magenta(), line);
                if !frontend_ready && line.contains("ready in") && line.contains("VITE") {
                    frontend_ready = true;
                    let _ = stderr_tx.send(()).await;
                }
            }
        });

        // Open browser when frontend is ready
        if !no_open {
            tokio::spawn(async move {
                let timeout = tokio::time::Duration::from_secs(120);
                if tokio::time::timeout(timeout, rx.recv()).await.is_ok() {
                    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                    let _ = open_browser("http://localhost:5173");
                }
            });
        }

        println!();
        println!("  {} Development environment running:", style("✅").green());
        println!("    Frontend:  http://localhost:5173");
        println!("    Backend:   http://localhost:8080");
        println!("    Dashboard: http://localhost:8080/_dashboard");
        println!();
        println!("  Press {} to stop.", style("Ctrl+C").yellow());
        println!();

        // Wait for Ctrl+C
        signal::ctrl_c().await?;

        println!();
        println!("{} Stopping development environment...", style("⏹").cyan());

        // Kill frontend
        if let Some(id) = frontend.id() {
            use nix::sys::signal::{Signal, kill};
            use nix::unistd::Pid;
            let _ = kill(Pid::from_raw(id as i32), Signal::SIGTERM);
        }
        let _ = frontend.wait().await;

        // Kill backend
        if let Some(id) = backend.id() {
            use nix::sys::signal::{Signal, kill};
            use nix::unistd::Pid;
            let _ = kill(Pid::from_raw(id as i32), Signal::SIGTERM);
        }
        let _ = backend.wait().await;

        // Stop PostgreSQL
        pg.stop().await?;

        println!("{} Development environment stopped.", style("✅").green());

        Ok(())
    }

    /// Start the development environment with Docker Compose.
    async fn up_docker(&self) -> Result<()> {
        println!();
        println!(
            "{} Starting FORGE development environment (Docker mode)...",
            style("🚀").cyan()
        );
        println!();

        println!("  {} docker found", style("✓").green());
        println!();

        println!("  {} Running: docker compose up --build", style("→").cyan());
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

    /// Stop the Docker development environment.
    async fn down_docker(&self, clear: bool) -> Result<()> {
        println!();
        if clear {
            println!(
                "{} Stopping and cleaning FORGE development environment...",
                style("🧹").cyan()
            );
            println!();
            println!("  {} Running: docker compose down -v", style("→").cyan());

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

            // Also remove target/ and pg_data/ if they exist
            let target_dir = Path::new("target");
            let pg_data_dir = Path::new("pg_data");

            if target_dir.exists() {
                println!("  {} Removing target/...", style("→").cyan());
                std::fs::remove_dir_all(target_dir)?;
            }
            if pg_data_dir.exists() {
                println!("  {} Removing pg_data/...", style("→").cyan());
                std::fs::remove_dir_all(pg_data_dir)?;
            }

            println!();
            println!(
                "{} Development environment stopped and cleaned.",
                style("✅").green()
            );
        } else {
            println!(
                "{} Stopping FORGE development environment...",
                style("⏹").cyan()
            );
            println!();
            println!("  {} Running: docker compose down", style("→").cyan());

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
            println!("{} Development environment stopped.", style("✅").green());
        }

        Ok(())
    }

    /// Stop the bare metal development environment.
    async fn down_bare_metal(&self, clear: bool) -> Result<()> {
        println!();
        println!(
            "{} Stopping FORGE development environment...",
            style("⏹").cyan()
        );
        println!();

        // Kill orphaned postgres processes from pg_data
        let pg_data_dir = Path::new("pg_data");
        if pg_data_dir.exists() {
            let postmaster_pid = pg_data_dir.join("data/postmaster.pid");
            if postmaster_pid.exists() {
                if let Ok(content) = std::fs::read_to_string(&postmaster_pid) {
                    if let Some(pid_str) = content.lines().next() {
                        if let Ok(pid) = pid_str.parse::<i32>() {
                            println!(
                                "  {} Stopping PostgreSQL (PID {})...",
                                style("→").cyan(),
                                pid
                            );
                            use nix::sys::signal::{Signal, kill};
                            use nix::unistd::Pid;
                            let _ = kill(Pid::from_raw(pid), Signal::SIGTERM);
                            // Wait a moment for graceful shutdown
                            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                        }
                    }
                }
            }
        }

        // Kill any cargo processes running from this directory
        // We look for processes matching the project name
        let project_dir = std::env::current_dir()?;
        if let Some(project_name) = project_dir.file_name().and_then(|n| n.to_str()) {
            // Use pkill to find and kill related processes
            let _ = Command::new("pkill")
                .args(["-f", &format!("target/debug/{}", project_name)])
                .status()
                .await;
        }

        println!("  {} Orphaned processes cleaned", style("✓").green());

        if clear {
            println!();
            let target_dir = Path::new("target");
            let pg_data_dir = Path::new("pg_data");

            if target_dir.exists() {
                println!("  {} Removing target/...", style("→").cyan());
                std::fs::remove_dir_all(target_dir)?;
                println!("  {} target/ removed", style("✓").green());
            }
            if pg_data_dir.exists() {
                println!("  {} Removing pg_data/...", style("→").cyan());
                std::fs::remove_dir_all(pg_data_dir)?;
                println!("  {} pg_data/ removed", style("✓").green());
            }

            println!();
            println!(
                "{} Development environment stopped and cleaned.",
                style("✅").green()
            );
        } else {
            println!();
            println!("{} Development environment stopped.", style("✅").green());
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

async fn get_tool_version(name: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(name).args(args).output().await?;

    if !output.status.success() {
        anyhow::bail!("{} not found", name);
    }

    let version_str = String::from_utf8_lossy(&output.stdout).trim().to_string();

    // Extract version number from output like "cargo 1.94.0 (abc123 2024-01-01)" or "1.3.2"
    // Look for a pattern like X.Y.Z
    let version = version_str
        .split_whitespace()
        .find(|s| {
            let parts: Vec<&str> = s.split('.').collect();
            parts.len() >= 2 && parts[0].parse::<u32>().is_ok() && parts[1].parse::<u32>().is_ok()
        })
        .unwrap_or(&version_str)
        .to_string();

    Ok(version)
}

fn check_version(version: &str, min_major: u32, min_minor: u32) -> bool {
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() < 2 {
        return false;
    }

    let major: u32 = parts[0].parse().unwrap_or(0);
    let minor: u32 = parts[1].parse().unwrap_or(0);

    major > min_major || (major == min_major && minor >= min_minor)
}

async fn is_port_in_use(port: u16) -> bool {
    tokio::net::TcpStream::connect(format!("127.0.0.1:{}", port))
        .await
        .is_ok()
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
            docker: false,
        };
        assert!(!cmd.no_open);
        assert!(!cmd.docker);
    }

    #[test]
    fn test_dev_command_docker() {
        let cmd = DevCommand {
            action: None,
            no_open: false,
            docker: true,
        };
        assert!(cmd.docker);
    }

    #[test]
    fn test_dev_command_down() {
        let cmd = DevCommand {
            action: Some(DevAction::Down { clear: false }),
            no_open: false,
            docker: false,
        };
        assert!(matches!(cmd.action, Some(DevAction::Down { clear: false })));
    }

    #[test]
    fn test_dev_command_down_clear() {
        let cmd = DevCommand {
            action: Some(DevAction::Down { clear: true }),
            no_open: false,
            docker: false,
        };
        assert!(matches!(cmd.action, Some(DevAction::Down { clear: true })));
    }

    #[test]
    fn test_check_version() {
        assert!(check_version("1.94.0", 1, 85));
        assert!(check_version("92.0", 1, 92));
        assert!(!check_version("1.84.0", 1, 85));
        assert!(check_version("2.0.0", 1, 85));
        assert!(check_version("1.3.2", 1, 3));
        assert!(!check_version("1.2.9", 1, 3));
    }
}
