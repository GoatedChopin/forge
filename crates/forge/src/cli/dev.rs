use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use console::style;
use std::collections::HashSet;
use std::path::Path;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::signal;
use tokio::sync::{Mutex, mpsc};
use tokio::time::{Duration, timeout};

use super::ui;

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

    /// Use external DATABASE_URL instead of embedded PostgreSQL
    #[arg(long)]
    pub no_pg: bool,

    /// Backend HTTP port
    #[arg(long, default_value_t = 8080)]
    pub backend_port: u16,

    /// Frontend Vite port
    #[arg(long, default_value_t = 5173)]
    pub frontend_port: u16,

    /// Embedded PostgreSQL port
    #[arg(long, default_value_t = 5432)]
    pub db_port: u16,

    /// Kill process(es) occupying requested ports and take over
    #[arg(long)]
    pub takeover_ports: bool,
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
                    ui::error(),
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

        ui::section("FORGE Dev (Bare Metal)");
        println!("  {} Starting development environment...", ui::tool());

        // Check prerequisites
        println!("  {} Checking prerequisites...", ui::step());

        // Check cargo version
        let cargo_version = match get_tool_version("cargo", &["--version"]).await {
            Ok(v) => v,
            Err(_) => {
                eprintln!("    {} cargo not found", ui::error());
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
                "    {} cargo {} found, but 1.92+ required",
                ui::error(),
                cargo_version
            );
            eprintln!();
            eprintln!(
                "Alternatively, use {} to run with Docker.",
                style("forge dev --docker").cyan()
            );
            std::process::exit(1);
        }
        println!("    {} cargo {}", ui::ok(), cargo_version);

        // Check cargo-watch
        if !check_tool_exists("cargo-watch").await {
            eprintln!(
                "    {} cargo-watch not found (required for hot reload)",
                ui::error()
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
        println!("    {} cargo-watch", ui::ok());

        // Check bun version
        let bun_version = match get_tool_version("bun", &["--version"]).await {
            Ok(v) => v,
            Err(_) => {
                eprintln!("    {} bun not found", ui::error());
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
                ui::error(),
                bun_version
            );
            eprintln!();
            eprintln!(
                "Alternatively, use {} to run with Docker.",
                style("forge dev --docker").cyan()
            );
            std::process::exit(1);
        }
        println!("    {} bun {}", ui::ok(), bun_version);

        println!();

        let backend_port = self.backend_port;
        let frontend_port = self.frontend_port;
        let db_port = self.db_port;

        validate_distinct_ports(self.no_pg, backend_port, frontend_port, db_port)?;
        ensure_port_available("backend", backend_port, self.takeover_ports).await?;
        ensure_port_available("frontend", frontend_port, self.takeover_ports).await?;
        if !self.no_pg {
            ensure_port_available("postgres", db_port, self.takeover_ports).await?;
        }

        println!("  {} Runtime settings:", ui::step());
        ui::kv("Backend", style(format!(":{}", backend_port)).yellow());
        ui::kv("Frontend", style(format!(":{}", frontend_port)).yellow());
        if self.no_pg {
            ui::kv("Database", style("external (DATABASE_URL)").yellow());
        } else {
            ui::kv("Database", style(format!("embedded :{}", db_port)).yellow());
        }
        ui::kv(
            "Watch",
            style("src/, migrations/, Cargo.toml, build.rs, .env, forge.toml").dim(),
        );
        println!();

        // Get project name from forge.toml
        let forge_toml = std::fs::read_to_string("forge.toml")?;
        let config: toml::Value = toml::from_str(&forge_toml)?;
        let project_name = config
            .get("project")
            .and_then(|p| p.get("name"))
            .and_then(|n| n.as_str())
            .unwrap_or("forge_dev");

        let mut embedded_pg: Option<PostgreSQL> = None;
        let mut backend: Option<Child> = None;
        let mut frontend: Option<Child> = None;

        let run_result: Result<()> = tokio::select! {
            result = async {
            let database_url = if self.no_pg {
                load_external_database_url()
            } else {
                println!("  {} Starting embedded PostgreSQL...", ui::step());

                // Store PostgreSQL data in project's pg_data directory
                let project_dir = std::env::current_dir()?;
                let pg_data_dir = project_dir.join("pg_data");

                let settings = Settings {
                    installation_dir: pg_data_dir.join("installation"),
                    data_dir: pg_data_dir.join("data"),
                    port: db_port,
                    username: "postgres".to_string(),
                    password: "forge".to_string(),
                    temporary: false,
                    ..Default::default()
                };

                let mut pg = PostgreSQL::new(settings);
                pg.setup().await?;
                pg.start().await?;

                // Create database if not exists
                let admin_url = format!("postgres://postgres:forge@localhost:{}/postgres", db_port);
                let pool = sqlx::PgPool::connect(&admin_url).await?;
                let db_name = project_name.replace('-', "_");
                let _ = sqlx::query(&format!("CREATE DATABASE \"{}\"", db_name))
                    .execute(&pool)
                    .await;
                pool.close().await;

                let database_url = format!("postgres://postgres:forge@localhost:{}/{}", db_port, db_name);
                embedded_pg = Some(pg);
                println!(
                    "    {} PostgreSQL running on localhost:{}",
                    ui::ok(),
                    db_port
                );
                println!();
                database_url
            };

            // Channel to signal when frontend is ready
            let (tx, mut rx) = mpsc::channel::<()>(1);
            let no_open = self.no_open;
            let detected_frontend_url = std::sync::Arc::new(Mutex::new(format!(
                "http://localhost:{}",
                frontend_port
            )));

            // Start backend with cargo-watch for hot reload
            println!(
                "  {} Starting backend with scoped cargo-watch...",
                ui::step()
            );

            let mut watch_args = vec![
                "watch",
                "--watch", "src",
                "--watch", "migrations",
                "--watch", "build.rs",
                "--watch", "Cargo.toml",
                "--watch", ".env",
                "--watch", "forge.toml",
            ];

            if std::path::Path::new("Cargo.lock").exists() {
                watch_args.extend(["--watch", "Cargo.lock"]);
            }

            watch_args.extend([
                "--ignore", "frontend/*",
                "--ignore", "**/*.json",
                "-x", "run --no-default-features",
            ]);

            let mut backend_child = Command::new("cargo")
                .args(&watch_args)
                .env("DATABASE_URL", &database_url)
                .env("RUST_LOG", "warn,forge=info")
                .env("HOST", "0.0.0.0")
                .env("PORT", backend_port.to_string())
                .env("WEBHOOK_SECRET", "demo-secret")
                .stdin(Stdio::inherit())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .context("failed to start cargo-watch backend")?;

            let backend_stdout = backend_child.stdout.take().expect("stdout piped");
            let backend_stderr = backend_child.stderr.take().expect("stderr piped");
            backend = Some(backend_child);

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
                ui::step()
            );

            let backend_ready_url = format!("http://localhost:{}/_api/ready", backend_port);
            let mut backend_ready = false;
            for _ in 0..300 {
                tokio::time::sleep(Duration::from_secs(1)).await;

                if let Some(child) = backend.as_mut()
                    && let Some(status) = child.try_wait()?
                {
                    anyhow::bail!("Backend exited early with status {}", status);
                }

                if let Ok(resp) = reqwest::get(&backend_ready_url).await
                    && resp.status().is_success()
                {
                    println!(
                        "    {} Backend running on http://localhost:{}",
                        ui::ok(),
                        backend_port
                    );
                    backend_ready = true;
                    break;
                }
            }
            if !backend_ready {
                anyhow::bail!("Backend failed to start within 5 minutes");
            }

            println!();

            // Start frontend with bun
            println!("  {} Starting frontend with bun...", ui::step());

            // Always run bun install to ensure dependencies are installed
            println!(
                "    {} Installing frontend dependencies...",
                ui::step()
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
                println!("    {} Syncing SvelteKit types...", ui::step());
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

            let mut frontend_child = Command::new("./node_modules/.bin/vite")
                .args([
                    "dev",
                    "--host",
                    "0.0.0.0",
                    "--port",
                    &frontend_port.to_string(),
                    "--strictPort",
                ])
                .current_dir("frontend")
                .stdin(Stdio::inherit())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .context("failed to start vite frontend")?;

            let frontend_stdout = frontend_child.stdout.take().expect("stdout piped");
            let frontend_stderr = frontend_child.stderr.take().expect("stderr piped");
            frontend = Some(frontend_child);

            // Spawn task to read frontend stdout
            let stdout_tx = tx.clone();
            let stdout_url = detected_frontend_url.clone();
            tokio::spawn(async move {
                let mut reader = BufReader::new(frontend_stdout).lines();
                let mut frontend_ready = false;
                while let Ok(Some(line)) = reader.next_line().await {
                    println!("{} {}", style("[frontend]").magenta(), line);
                    if let Some(url) = extract_http_url(&line) {
                        let mut guard = stdout_url.lock().await;
                        *guard = url;
                    }
                    if !frontend_ready && line.contains("ready in") && line.contains("VITE") {
                        frontend_ready = true;
                        let _ = stdout_tx.send(()).await;
                    }
                }
            });

            // Spawn task to read frontend stderr
            let stderr_tx = tx;
            let stderr_url = detected_frontend_url.clone();
            tokio::spawn(async move {
                let mut reader = BufReader::new(frontend_stderr).lines();
                let mut frontend_ready = false;
                while let Ok(Some(line)) = reader.next_line().await {
                    eprintln!("{} {}", style("[frontend]").magenta(), line);
                    if let Some(url) = extract_http_url(&line) {
                        let mut guard = stderr_url.lock().await;
                        *guard = url;
                    }
                    if !frontend_ready && line.contains("ready in") && line.contains("VITE") {
                        frontend_ready = true;
                        let _ = stderr_tx.send(()).await;
                    }
                }
            });

            // Wait for frontend readiness signal
            match timeout(Duration::from_secs(120), rx.recv()).await {
                Ok(Some(_)) => {}
                Ok(None) => anyhow::bail!("Frontend exited before signaling readiness"),
                Err(_) => anyhow::bail!("Frontend failed to become ready within 120 seconds"),
            }

            let frontend_url = detected_frontend_url.lock().await.clone();

            // Open browser when frontend is ready
            if !no_open {
                let _ = open_browser(&frontend_url);
            }

            println!();
            println!("  {} Development environment running:", ui::ok());
            ui::kv("Frontend", &frontend_url);
            ui::kv("Backend", format!("http://localhost:{}", backend_port));
            println!();
            println!("  Press {} to stop.", style("Ctrl+C").yellow());
            println!();

            // Block until shutdown
            loop { tokio::time::sleep(Duration::from_secs(3600)).await; }
            } => result,
            _ = signal::ctrl_c() => Ok(()),
        };

        println!();
        println!("{} Stopping development environment...", ui::stop());

        shutdown_child("frontend", &mut frontend).await;
        shutdown_child("backend", &mut backend).await;

        if let Some(pg) = embedded_pg {
            let _ = pg.stop().await;
        }

        if run_result.is_ok() {
            println!("{} Development environment stopped.", ui::ok());
        } else {
            eprintln!(
                "{} Development environment stopped due to error.",
                ui::warn()
            );
        }

        run_result
    }

    /// Start the development environment with Docker Compose.
    async fn up_docker(&self) -> Result<()> {
        ui::section("FORGE Dev (Docker)");
        println!("  {} Starting development environment...", ui::tool());

        println!("  {} docker found", ui::ok());
        println!();

        println!("  {} Running: docker compose up --build", ui::step());
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
                let timeout = Duration::from_secs(300);
                if tokio::time::timeout(timeout, rx.recv()).await.is_ok() {
                    // Small delay to ensure server is fully ready
                    tokio::time::sleep(Duration::from_millis(500)).await;
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
                    ui::stop()
                );

                // Send SIGTERM to docker compose process
                if let Some(id) = child.id() {
                    use nix::sys::signal::{Signal, kill};
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
                    ui::ok()
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

            // Also remove target/ and pg_data/ if they exist
            let target_dir = Path::new("target");
            let pg_data_dir = Path::new("pg_data");

            if target_dir.exists() {
                println!("  {} Removing target/...", ui::step());
                std::fs::remove_dir_all(target_dir)?;
            }
            if pg_data_dir.exists() {
                println!("  {} Removing pg_data/...", ui::step());
                std::fs::remove_dir_all(pg_data_dir)?;
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

    /// Stop the bare metal development environment.
    async fn down_bare_metal(&self, clear: bool) -> Result<()> {
        println!();
        println!("{} Stopping FORGE development environment...", ui::stop());
        println!();

        // Kill orphaned postgres processes from pg_data
        let pg_data_dir = Path::new("pg_data");
        if pg_data_dir.exists() {
            let postmaster_pid = pg_data_dir.join("data/postmaster.pid");
            if postmaster_pid.exists()
                && let Ok(content) = std::fs::read_to_string(&postmaster_pid)
                && let Some(pid_str) = content.lines().next()
                && let Ok(pid) = pid_str.parse::<i32>()
            {
                println!("  {} Stopping PostgreSQL (PID {})...", ui::step(), pid);
                use nix::sys::signal::{Signal, kill};
                use nix::unistd::Pid;
                let _ = kill(Pid::from_raw(pid), Signal::SIGTERM);
                // Wait a moment for graceful shutdown
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }

        // Kill backend processes matching current project binary path
        let project_dir = std::env::current_dir()?;
        if let Some(project_name) = project_dir.file_name().and_then(|n| n.to_str()) {
            let _ = Command::new("pkill")
                .args(["-f", &format!("target/debug/{}", project_name)])
                .status()
                .await;
        }

        // Kill cargo-watch and vite descendants from this workspace if they remain
        let cwd_pattern = project_dir.display().to_string();
        let _ = Command::new("pkill")
            .args(["-f", &format!("cargo watch.*{}", cwd_pattern)])
            .status()
            .await;
        let _ = Command::new("pkill")
            .args(["-f", &format!("vite.*{}", cwd_pattern)])
            .status()
            .await;

        println!("  {} Orphaned processes cleaned", ui::ok());

        if clear {
            println!();
            let target_dir = Path::new("target");
            let pg_data_dir = Path::new("pg_data");

            if target_dir.exists() {
                println!("  {} Removing target/...", ui::step());
                std::fs::remove_dir_all(target_dir)?;
                println!("  {} target/ removed", ui::ok());
            }
            if pg_data_dir.exists() {
                println!("  {} Removing pg_data/...", ui::step());
                std::fs::remove_dir_all(pg_data_dir)?;
                println!("  {} pg_data/ removed", ui::ok());
            }

            println!();
            println!("{} Development environment stopped and cleaned.", ui::ok());
        } else {
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

async fn get_tool_version(name: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(name).args(args).output().await?;

    if !output.status.success() {
        anyhow::bail!("{} not found", name);
    }

    let version_str = String::from_utf8_lossy(&output.stdout).trim().to_string();

    // Extract version number from output like "cargo 1.94.0 (abc123 2024-01-01)" or "1.3.2"
    let version = version_str
        .split_whitespace()
        .find(|s| {
            let mut parts = s.split('.');
            let valid_major = parts.next().is_some_and(|p| p.parse::<u32>().is_ok());
            let valid_minor = parts.next().is_some_and(|p| p.parse::<u32>().is_ok());
            valid_major && valid_minor
        })
        .unwrap_or(&version_str)
        .to_string();

    Ok(version)
}

fn check_version(version: &str, min_major: u32, min_minor: u32) -> bool {
    let mut parts = version.split('.');
    let Some(major) = parts.next().and_then(|p| p.parse::<u32>().ok()) else {
        return false;
    };
    let Some(minor) = parts.next().and_then(|p| p.parse::<u32>().ok()) else {
        return false;
    };

    major > min_major || (major == min_major && minor >= min_minor)
}

async fn is_port_in_use(port: u16) -> bool {
    tokio::net::TcpStream::connect(format!("127.0.0.1:{}", port))
        .await
        .is_ok()
}

#[derive(Clone, Debug)]
struct PortOwner {
    pid: i32,
    command: String,
    user: String,
}

impl PortOwner {
    fn display(&self) -> String {
        format!(
            "{} (pid {}, user {})",
            self.command,
            style(self.pid).yellow(),
            self.user
        )
    }
}

fn validate_distinct_ports(
    no_pg: bool,
    backend_port: u16,
    frontend_port: u16,
    db_port: u16,
) -> Result<()> {
    if backend_port == frontend_port {
        anyhow::bail!(
            "backend and frontend ports are the same ({}). Use different values.",
            backend_port
        );
    }

    if !no_pg {
        if backend_port == db_port {
            anyhow::bail!(
                "backend and postgres ports are the same ({}). Use different values.",
                db_port
            );
        }
        if frontend_port == db_port {
            anyhow::bail!(
                "frontend and postgres ports are the same ({}). Use different values.",
                db_port
            );
        }
    }

    Ok(())
}

async fn ensure_port_available(name: &str, port: u16, takeover: bool) -> Result<()> {
    if !is_port_in_use(port).await {
        return Ok(());
    }

    let owners = get_port_owners(port).await;
    if !owners.is_empty() {
        eprintln!("  {} Port {} ({}) is in use by:", ui::error(), port, name);
        for owner in &owners {
            eprintln!("      - {}", owner.display());
        }
    } else {
        eprintln!(
            "  {} Port {} ({}) is in use (unable to identify process).",
            ui::error(),
            port,
            name
        );
    }

    if !takeover {
        anyhow::bail!(
            "Port {} is occupied. Stop the process, choose another port, or rerun with --takeover-ports.",
            port
        );
    }

    println!("  {} Taking over port {} ({})...", ui::step(), port, name);
    kill_port_owners(port, &owners).await?;

    for _ in 0..20 {
        if !is_port_in_use(port).await {
            println!("    {} Port {} is now available", ui::ok(), port);
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }

    anyhow::bail!(
        "Port {} is still in use after takeover attempt. Please stop it manually.",
        port
    )
}

async fn get_port_owners(port: u16) -> Vec<PortOwner> {
    let output = Command::new("lsof")
        .args(["-nP", &format!("-iTCP:{}", port), "-sTCP:LISTEN"])
        .output()
        .await;

    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut owners = Vec::new();
    let mut seen = HashSet::new();

    for line in stdout.lines().skip(1) {
        let mut parts = line.split_whitespace();
        let command = parts.next().unwrap_or("").to_string();
        let pid = parts
            .next()
            .and_then(|p| p.parse::<i32>().ok())
            .unwrap_or(0);
        let user = parts.next().unwrap_or("unknown").to_string();
        if pid <= 0 || !seen.insert(pid) {
            continue;
        }
        owners.push(PortOwner { pid, command, user });
    }

    owners
}

async fn get_port_listener_pids(port: u16) -> Vec<i32> {
    let output = Command::new("lsof")
        .args(["-ti", &format!("-iTCP:{}", port), "-sTCP:LISTEN"])
        .output()
        .await;

    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.trim().parse::<i32>().ok())
        .collect()
}

async fn kill_port_owners(port: u16, owners: &[PortOwner]) -> Result<()> {
    use nix::sys::signal::{Signal, kill};
    use nix::unistd::Pid;

    let mut pids: HashSet<i32> = owners.iter().map(|o| o.pid).collect();
    if pids.is_empty() {
        pids.extend(get_port_listener_pids(port).await);
    }
    if pids.is_empty() {
        anyhow::bail!("Could not identify process owning port {}", port);
    }

    for pid in &pids {
        let _ = kill(Pid::from_raw(*pid), Signal::SIGTERM);
    }
    tokio::time::sleep(Duration::from_secs(1)).await;

    if !is_port_in_use(port).await {
        return Ok(());
    }

    for pid in &pids {
        let _ = kill(Pid::from_raw(*pid), Signal::SIGKILL);
    }
    tokio::time::sleep(Duration::from_millis(500)).await;
    Ok(())
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

fn extract_http_url(line: &str) -> Option<String> {
    let start = line.find("http://").or_else(|| line.find("https://"))?;
    let rest = &line[start..];
    let end = rest
        .find(|c: char| c.is_whitespace() || c == '\u{001b}')
        .unwrap_or(rest.len());
    let url = rest[..end].trim_end_matches('/');
    if url.starts_with("http://") || url.starts_with("https://") {
        Some(url.to_string())
    } else {
        None
    }
}

fn load_external_database_url() -> String {
    dotenvy::dotenv().ok();
    match std::env::var("DATABASE_URL") {
        Ok(url) if !url.trim().is_empty() => url,
        _ => {
            eprintln!(
                "{} DATABASE_URL is required when using {}",
                ui::error(),
                style("--no-pg").yellow()
            );
            eprintln!("Set DATABASE_URL in .env or environment.");
            std::process::exit(1);
        }
    }
}

async fn shutdown_child(name: &str, child: &mut Option<Child>) {
    let Some(mut child) = child.take() else {
        return;
    };

    let Some(id) = child.id() else {
        let _ = child.wait().await;
        return;
    };

    use nix::sys::signal::{Signal, kill};
    use nix::unistd::Pid;

    let pid = id as i32;

    let signal_step = |sig: Signal| {
        let _ = kill(Pid::from_raw(pid), sig);
    };

    let kill_descendants = async |sig: &str| {
        let _ = Command::new("pkill")
            .args([sig, "-P", &id.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;
    };

    signal_step(Signal::SIGINT);
    kill_descendants("-INT").await;
    if wait_for_child(&mut child, Duration::from_secs(2)).await {
        return;
    }

    eprintln!(
        "  {} {} did not exit on SIGINT, escalating...",
        ui::step(),
        name
    );

    signal_step(Signal::SIGTERM);
    kill_descendants("-TERM").await;
    if wait_for_child(&mut child, Duration::from_secs(3)).await {
        return;
    }

    signal_step(Signal::SIGKILL);
    kill_descendants("-KILL").await;
    let _ = child.wait().await;
}

async fn wait_for_child(child: &mut Child, duration: Duration) -> bool {
    matches!(timeout(duration, child.wait()).await, Ok(Ok(_)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn test_dev_command_parse() {
        let cmd = DevCommand {
            action: None,
            no_open: false,
            docker: false,
            no_pg: false,
            backend_port: 8080,
            frontend_port: 5173,
            db_port: 5432,
            takeover_ports: false,
        };
        assert!(!cmd.no_open);
        assert!(!cmd.docker);
        assert!(!cmd.no_pg);
    }

    #[test]
    fn test_dev_command_docker() {
        let cmd = DevCommand {
            action: None,
            no_open: false,
            docker: true,
            no_pg: false,
            backend_port: 8080,
            frontend_port: 5173,
            db_port: 5432,
            takeover_ports: false,
        };
        assert!(cmd.docker);
    }

    #[test]
    fn test_dev_command_down() {
        let cmd = DevCommand {
            action: Some(DevAction::Down { clear: false }),
            no_open: false,
            docker: false,
            no_pg: false,
            backend_port: 8080,
            frontend_port: 5173,
            db_port: 5432,
            takeover_ports: false,
        };
        assert!(matches!(cmd.action, Some(DevAction::Down { clear: false })));
    }

    #[test]
    fn test_dev_command_down_clear() {
        let cmd = DevCommand {
            action: Some(DevAction::Down { clear: true }),
            no_open: false,
            docker: false,
            no_pg: false,
            backend_port: 8080,
            frontend_port: 5173,
            db_port: 5432,
            takeover_ports: false,
        };
        assert!(matches!(cmd.action, Some(DevAction::Down { clear: true })));
    }

    #[test]
    fn test_check_version() {
        assert!(check_version("1.94.0", 1, 85));
        assert!(check_version("1.92.0", 1, 92));
        assert!(!check_version("1.84.0", 1, 85));
        assert!(check_version("2.0.0", 1, 85));
        assert!(check_version("1.3.2", 1, 3));
        assert!(!check_version("1.2.9", 1, 3));
    }

    #[test]
    fn test_extract_http_url() {
        let url = extract_http_url("  ➜  Local:   http://localhost:5173/");
        assert_eq!(url.as_deref(), Some("http://localhost:5173"));
    }

    #[test]
    fn test_validate_distinct_ports_ok() {
        assert!(validate_distinct_ports(false, 8080, 5173, 5432).is_ok());
        assert!(validate_distinct_ports(true, 8080, 5173, 8080).is_ok());
    }

    #[test]
    fn test_validate_distinct_ports_conflicts() {
        assert!(validate_distinct_ports(false, 8080, 8080, 5432).is_err());
        assert!(validate_distinct_ports(false, 8080, 5173, 8080).is_err());
        assert!(validate_distinct_ports(false, 8080, 5173, 5173).is_err());
    }
}
