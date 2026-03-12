use anyhow::Result;
use clap::Parser;
use console::style;
use std::fs;
use std::io::IsTerminal;
use std::path::Path;
use std::process::{Command as StdCommand, Stdio};
use tokio::process::Command as TokioCommand;
use tokio::signal;

use super::frontend_scaffold::{ScaffoldMode, TemplateFile, shared_frontend_templates};
use super::frontend_target::FrontendTarget;
use super::template::render;
use super::ui;
use crate::template_vars;

// In debug builds, embed the path to the forge source directory
#[cfg(debug_assertions)]
const FORGE_MANIFEST_DIR: &str = env!("CARGO_MANIFEST_DIR");

/// Get the forge workspace root directory (only available in debug builds).
/// CARGO_MANIFEST_DIR points to crates/forge, so we go up two levels.
#[cfg(debug_assertions)]
fn get_forge_workspace_dir() -> Option<&'static str> {
    let manifest_dir = Path::new(FORGE_MANIFEST_DIR);
    // Go up from crates/forge to the workspace root
    manifest_dir.parent()?.parent()?.to_str()
}

/// Append cargo patch section to use local forge crates (only in debug builds).
#[cfg(debug_assertions)]
fn append_cargo_patch(cargo_toml_path: &Path) -> Result<()> {
    let workspace_dir = get_forge_workspace_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not determine forge workspace directory"))?;

    let patch_section = format!(
        r#"
# Local dev patches (debug build) - remove before publishing
[patch.crates-io]
forgex = {{ path = "{ws}/crates/forge" }}
forge-core = {{ path = "{ws}/crates/forge-core" }}
forge-macros = {{ path = "{ws}/crates/forge-macros" }}
forge-runtime = {{ path = "{ws}/crates/forge-runtime" }}
forge-codegen = {{ path = "{ws}/crates/forge-codegen" }}
"#,
        ws = workspace_dir
    );

    let mut content = fs::read_to_string(cargo_toml_path)?;
    content.push_str(&patch_section);
    fs::write(cargo_toml_path, content)?;

    Ok(())
}

/// Add forge workspace volume to docker-compose.yml (only in debug builds).
///
/// Mounts the host workspace at the same absolute path inside the container
/// so cargo patch paths resolve identically in both environments.
#[cfg(debug_assertions)]
fn patch_docker_compose(docker_compose_path: &Path) -> Result<()> {
    let workspace_dir = get_forge_workspace_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not determine forge workspace directory"))?;

    let content = fs::read_to_string(docker_compose_path)?;
    let patched = content.replace(
        "      - target_cache:/app/target\n",
        &format!(
            "      - target_cache:/app/target\n      - {ws}:{ws}\n",
            ws = workspace_dir
        ),
    );
    fs::write(docker_compose_path, patched)?;

    Ok(())
}

/// Extract project name from a path (last segment only).
/// Handles: "my-app", "path/to/my-app", "./my-app", "../my-app"
pub(super) fn extract_project_name(input: &str) -> String {
    Path::new(input)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(input)
        .to_string()
}

/// Check if git is available on the system.
fn is_git_available() -> bool {
    StdCommand::new("git")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Check if the directory is inside an existing git repository.
fn is_inside_git_repo(dir: &Path) -> bool {
    StdCommand::new("git")
        .args(["rev-parse", "--git-dir"])
        .current_dir(dir)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Run forge generate to create frontend types.
fn run_forge_generate(dir: &Path) -> Result<()> {
    println!("  {} Generating frontend types...", ui::step());

    // Get the current executable path to run forge generate
    let forge_exe = std::env::current_exe().unwrap_or_else(|_| "forge".into());

    let output = StdCommand::new(&forge_exe)
        .args(["generate", "-y"])
        .current_dir(dir)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!(
            "  {} Failed to generate types: {}",
            ui::warn(),
            stderr.trim()
        );
        return Ok(());
    }

    println!("  {} Frontend types generated", ui::ok());

    let frontend_dir = dir.join("frontend");
    if let Some(target) = FrontendTarget::detect(&frontend_dir) {
        target.post_generate(&frontend_dir)?;
    }

    Ok(())
}

/// Run formatters (bun format and cargo fmt) to ensure clean code.
fn run_formatters(dir: &Path) -> Result<()> {
    let frontend_dir = dir.join("frontend");
    if frontend_dir.exists() && frontend_dir.join("package.json").exists() {
        println!("  {} Formatting frontend...", ui::step());
        let output = StdCommand::new("bun")
            .args(["run", "format"])
            .current_dir(&frontend_dir)
            .output();

        match output {
            Ok(o) if o.status.success() => {
                println!("  {} Frontend formatted", ui::ok());
            }
            _ => {}
        }
    }

    if let Some(target) = FrontendTarget::detect(&frontend_dir)
        && target.extra_format(dir)?
    {
        println!("  {} Dioxus frontend formatted", ui::ok());
    }

    let cargo_check = StdCommand::new("cargo").arg("--version").output();
    if matches!(cargo_check, Ok(ref o) if o.status.success()) {
        println!("  {} Formatting backend...", ui::step());
        let output = StdCommand::new("cargo")
            .args(["fmt"])
            .current_dir(dir)
            .output();

        match output {
            Ok(o) if o.status.success() => {
                println!("  {} Backend formatted", ui::ok());
            }
            _ => {}
        }
    }

    Ok(())
}

/// Generate Cargo.lock before initial commit.
/// Also generates frontend/Cargo.lock for Dioxus projects.
fn generate_cargo_lockfile(dir: &Path) -> Result<()> {
    println!("  {} Generating Cargo.lock...", ui::step());

    if !matches!(StdCommand::new("cargo").arg("--version").output(), Ok(o) if o.status.success()) {
        eprintln!(
            "  {} cargo not found, skipping lockfile generation",
            ui::warn()
        );
        return Ok(());
    }

    let output = StdCommand::new("cargo")
        .args(["generate-lockfile"])
        .current_dir(dir)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!(
            "  {} Failed to generate Cargo.lock: {}",
            ui::warn(),
            stderr.trim()
        );
        return Ok(());
    }

    println!("  {} Cargo.lock generated", ui::ok());

    // Dioxus frontend is a separate workspace, generate its lockfile too
    let frontend_cargo = dir.join("frontend/Cargo.toml");
    if frontend_cargo.exists() {
        let output = StdCommand::new("cargo")
            .args(["generate-lockfile"])
            .current_dir(dir.join("frontend"))
            .output()?;

        if output.status.success() {
            println!("  {} frontend/Cargo.lock generated", ui::ok());
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            eprintln!(
                "  {} Failed to generate frontend/Cargo.lock: {}",
                ui::warn(),
                stderr.trim()
            );
        }
    }

    Ok(())
}

/// Generate bun.lock file using native bun.
/// Runs `bun install --lockfile-only` in the frontend directory.
fn generate_bun_lockfile(dir: &Path) -> Result<()> {
    let frontend_dir = dir.join("frontend");
    if !frontend_dir.join("package.json").exists() {
        return Ok(());
    }

    println!("  {} Generating bun.lock...", ui::step());

    // Check if bun is available
    let bun_check = StdCommand::new("bun").arg("--version").output();

    if !matches!(bun_check, Ok(ref o) if o.status.success()) {
        eprintln!(
            "  {} bun not found, skipping lockfile generation",
            ui::warn()
        );
        eprintln!(
            "    Run {} in frontend/ after installing bun",
            style("bun install").cyan()
        );
        return Ok(());
    }

    let output = StdCommand::new("bun")
        .args(["install", "--lockfile-only"])
        .current_dir(&frontend_dir)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!(
            "  {} Failed to generate bun.lock: {}",
            ui::warn(),
            stderr.trim()
        );
        // Non-fatal: continue without lockfile
        return Ok(());
    }

    println!("  {} bun.lock generated", ui::ok());

    Ok(())
}

const SKILL_INSTALL_URL: &str =
    "https://github.com/isala404/forge/tree/main/docs/skills/forge-idiomatic-engineer";

/// Runs `bunx skills add` so AI agents get project-aware conventions out of the box.
/// When `non_interactive` is true, passes `-y` to skip prompts.
async fn install_skill(dir: &Path, non_interactive: bool) -> Result<()> {
    println!(
        "  {} Preparing forge-idiomatic-engineer skill installer...",
        ui::step()
    );

    let bun_check = StdCommand::new("bun").arg("--version").output();
    if !matches!(bun_check, Ok(ref o) if o.status.success()) {
        eprintln!(
            "  {} bun not found, skipping skill installation",
            ui::warn()
        );
        eprintln!(
            "    Run {} to install later",
            style(format!("bunx skills add {SKILL_INSTALL_URL}")).cyan()
        );
        return Ok(());
    }

    if non_interactive {
        let output = StdCommand::new("bunx")
            .args(["skills", "add", "-y", SKILL_INSTALL_URL])
            .current_dir(dir)
            .output();

        match output {
            Ok(o) if o.status.success() => {
                println!("  {} forge-idiomatic-engineer skill installed", ui::ok());
            }
            Ok(o) => {
                let stderr = String::from_utf8_lossy(&o.stderr);
                eprintln!(
                    "  {} Skill installation failed: {}",
                    ui::warn(),
                    stderr.trim()
                );
                eprintln!(
                    "    Run {} to install later",
                    style(format!("bunx skills add {SKILL_INSTALL_URL}")).cyan()
                );
            }
            Err(err) => {
                eprintln!("  {} Failed to run skill installer: {}", ui::warn(), err);
            }
        }
        return Ok(());
    }

    if !std::io::stdin().is_terminal()
        || !std::io::stdout().is_terminal()
        || !std::io::stderr().is_terminal()
    {
        eprintln!(
            "  {} Interactive terminal not available, skipping skill installer",
            ui::warn()
        );
        eprintln!(
            "    Run {} to install later",
            style(format!("bunx skills add {SKILL_INSTALL_URL}")).cyan()
        );
        return Ok(());
    }

    println!(
        "  {} Handing terminal control to the skill installer...",
        ui::step()
    );
    println!(
        "    Run completes when the installer exits. Press Ctrl+C in the installer to stop and continue."
    );

    let mut child = match TokioCommand::new("bunx")
        .args(["skills", "add", SKILL_INSTALL_URL])
        .current_dir(dir)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
    {
        Ok(child) => child,
        Err(err) => {
            eprintln!("  {} Failed to start skill installer: {}", ui::warn(), err);
            eprintln!(
                "    Run {} to install later",
                style(format!("bunx skills add {SKILL_INSTALL_URL}")).cyan()
            );
            return Ok(());
        }
    };

    tokio::select! {
        status = child.wait() => {
            match status {
                Ok(status) if status.success() => {
                    println!("  {} forge-idiomatic-engineer skill installed", ui::ok());
                }
                Ok(status) => {
                    eprintln!(
                        "  {} Skill installer exited with status {}",
                        ui::warn(),
                        status
                    );
                    eprintln!(
                        "    Re-run {} if you still want the skill",
                        style(format!("bunx skills add {SKILL_INSTALL_URL}")).cyan()
                    );
                }
                Err(err) => {
                    eprintln!(
                        "  {} Failed to wait for skill installer: {}",
                        ui::warn(),
                        err
                    );
                }
            }
        }
        _ = signal::ctrl_c() => {
            println!();
            println!(
                "  {} Leaving skill installer and continuing project setup...",
                ui::stop()
            );

            #[cfg(unix)]
            if let Some(id) = child.id() {
                use nix::sys::signal::{Signal, kill};
                use nix::unistd::Pid;
                let _ = kill(Pid::from_raw(id as i32), Signal::SIGINT);
            }

            match child.wait().await {
                Ok(status) if status.success() => {
                    println!("  {} forge-idiomatic-engineer skill installed", ui::ok());
                }
                Ok(_) => {
                    eprintln!("  {} Skill installation left to the user", ui::warn());
                }
                Err(err) => {
                    eprintln!(
                        "  {} Failed to wait for skill installer after Ctrl+C: {}",
                        ui::warn(),
                        err
                    );
                }
            }
        }
    }

    Ok(())
}

/// Initialize git repository and create initial commit.
/// Skips if directory is already inside a git repository.
fn init_git_repo(dir: &Path) -> Result<()> {
    // Skip if already inside a git repo (parent or current)
    if is_inside_git_repo(dir) {
        return Ok(());
    }

    // git init
    let init = StdCommand::new("git")
        .args(["init"])
        .current_dir(dir)
        .output()?;

    if !init.status.success() {
        return Ok(()); // Silently skip if init fails
    }

    // git add .
    let add = StdCommand::new("git")
        .args(["add", "."])
        .current_dir(dir)
        .output()?;

    if !add.status.success() {
        return Ok(());
    }

    // git commit
    let _ = StdCommand::new("git")
        .args(["commit", "-m", "Initialize project with Forge"])
        .current_dir(dir)
        .output()?;

    Ok(())
}

const CARGO_TOML: &str =
    include_str!("../../templates/scaffold/demo/project/common/Cargo.toml.tmpl");
const FORGE_TOML: &str =
    include_str!("../../templates/scaffold/demo/project/common/forge.toml.tmpl");
const GITIGNORE: &str = include_str!("../../templates/scaffold/demo/project/common/gitignore.tmpl");
const ENV: &str = include_str!("../../templates/scaffold/demo/project/common/env.tmpl");
const MIGRATION_INITIAL: &str =
    include_str!("../../templates/scaffold/demo/project/common/migrations/0001_initial.sql.tmpl");
const SCHEMA_MOD: &str =
    include_str!("../../templates/scaffold/demo/project/common/schema/mod.rs.tmpl");
const SCHEMA_USER: &str =
    include_str!("../../templates/scaffold/demo/project/common/schema/user.rs.tmpl");
const FUNCTIONS_MOD: &str =
    include_str!("../../templates/scaffold/demo/project/common/functions/mod.rs.tmpl");
const FUNCTIONS_USERS: &str =
    include_str!("../../templates/scaffold/demo/project/common/functions/users.rs.tmpl");
const FUNCTIONS_ISS: &str =
    include_str!("../../templates/scaffold/demo/project/common/functions/iss.rs.tmpl");
const FUNCTIONS_TRADES: &str =
    include_str!("../../templates/scaffold/demo/project/common/functions/trades.rs.tmpl");
const FUNCTIONS_EXPORT: &str =
    include_str!("../../templates/scaffold/demo/project/common/functions/export.rs.tmpl");
const FUNCTIONS_VERIFICATION: &str =
    include_str!("../../templates/scaffold/demo/project/common/functions/verification.rs.tmpl");
const FUNCTIONS_WEBHOOK: &str =
    include_str!("../../templates/scaffold/demo/project/common/functions/webhook.rs.tmpl");
const IGNORE: &str = include_str!("../../templates/scaffold/demo/project/common/ignore.tmpl");

const EMPTY_CARGO_TOML: &str =
    include_str!("../../templates/scaffold/minimal/project/common/Cargo.toml.tmpl");
const EMPTY_FORGE_TOML: &str =
    include_str!("../../templates/scaffold/minimal/project/common/forge.toml.tmpl");
const EMPTY_GITIGNORE: &str =
    include_str!("../../templates/scaffold/minimal/project/common/gitignore.tmpl");
const EMPTY_ENV: &str = include_str!("../../templates/scaffold/minimal/project/common/env.tmpl");
const EMPTY_MIGRATION_INITIAL: &str = include_str!(
    "../../templates/scaffold/minimal/project/common/migrations/0001_initial.sql.example.tmpl"
);
const EMPTY_SCHEMA_MOD: &str =
    include_str!("../../templates/scaffold/minimal/project/common/schema/mod.rs.tmpl");
const EMPTY_FUNCTIONS_MOD: &str =
    include_str!("../../templates/scaffold/minimal/project/common/functions/mod.rs.tmpl");
const EMPTY_IGNORE: &str =
    include_str!("../../templates/scaffold/minimal/project/common/ignore.tmpl");
const SQLX_TOML: &str =
    include_str!("../../templates/scaffold/minimal/project/common/sqlx.toml.tmpl");
fn write_template_files(
    base_dir: &Path,
    files: &[TemplateFile],
    vars: &std::collections::HashMap<&str, &str>,
) -> Result<()> {
    for file in files {
        let content = if file.render {
            render(file.template, vars)
        } else {
            file.template.to_string()
        };
        fs::write(base_dir.join(file.path), content)?;
    }

    Ok(())
}

/// Create a new FORGE project.
#[derive(Parser)]
#[command(after_help = NEW_AFTER_HELP)]
pub struct NewCommand {
    /// Project name.
    pub name: String,

    /// Create a full demo project with example code.
    ///
    /// Includes: User CRUD, background jobs, cron tasks, workflows,
    /// external API actions, and a complete frontend demo UI.
    /// Perfect for learning FORGE or starting with working examples.
    #[arg(long, conflicts_with = "minimal")]
    pub demo: bool,

    /// Create a clean project with minimal scaffolding.
    ///
    /// Includes: Empty schema, functions, and migrations directories
    /// with commented examples. Frontend has a starter page.
    /// Perfect for experienced developers starting fresh.
    #[arg(long, conflicts_with = "demo")]
    pub minimal: bool,

    /// Output directory (defaults to project name).
    #[arg(short, long)]
    pub output: Option<String>,

    /// Frontend target (`sveltekit` or `dioxus`).
    #[arg(long, default_value = "sveltekit")]
    pub target: FrontendTarget,

    /// Skip generating bun.lock file before initial commit.
    ///
    /// By default, forge new runs `bun install --lockfile-only` in Docker
    /// to generate the bun.lock file before the initial git commit.
    /// Use this flag to skip lockfile generation.
    #[arg(long)]
    pub no_lock: bool,

    /// Skip interactive skill installer prompts.
    ///
    /// When set, passes `-y` to `bunx skills add` so the
    /// forge-idiomatic-engineer skill installs without user input.
    /// Without this flag, the installer runs interactively.
    #[arg(long)]
    pub include_skill: bool,
}

const NEW_AFTER_HELP: &str = r#"TEMPLATE MODES:
  You must choose one of --demo or --minimal:

  --demo      Full demo project with working examples
              - User model with CRUD operations
              - Background job (export users)
              - Cron task (ISS location tracker)
              - Durable workflow (account verification)
              - Mutations with HTTP support
              - Complete frontend demo UI

  --minimal   Clean slate with just the structure
              - Empty schema/ and functions/ directories
              - Commented examples showing patterns
              - Starter frontend page
              - Ready for your own code

EXAMPLES:
  forge new my-app --demo       Learn FORGE with working examples
  forge new my-app --minimal    Start fresh with clean scaffolding"#;

impl NewCommand {
    /// Execute the new project command.
    pub async fn execute(self) -> Result<()> {
        ui::section("Create FORGE Project");
        println!("  {} Generating project files...", ui::tool());

        // Require either --demo or --minimal
        if !self.demo && !self.minimal {
            eprintln!("{} You must specify a template mode", ui::error());
            eprintln!();
            eprintln!("Choose one of:");
            eprintln!();
            eprintln!(
                "  {} {} {}",
                ui::bullet(),
                style("--demo").cyan().bold(),
                style("Full demo project with working examples").dim()
            );
            eprintln!("          User CRUD, jobs, crons, workflows, actions, and demo UI");
            eprintln!();
            eprintln!(
                "  {} {} {}",
                ui::bullet(),
                style("--minimal").cyan().bold(),
                style("Clean slate with just the structure").dim()
            );
            eprintln!("          Empty directories with commented examples, starter frontend");
            eprintln!();
            eprintln!("Examples:");
            eprintln!(
                "  {} {} {}",
                ui::bullet(),
                style("forge new my-app --demo").green(),
                style("# Learn FORGE with examples").dim()
            );
            eprintln!(
                "  {} {} {}",
                ui::bullet(),
                style("forge new my-app --minimal").green(),
                style("# Start fresh").dim()
            );
            eprintln!();
            eprintln!("Run {} for more details", style("forge new --help").cyan());
            std::process::exit(1);
        }

        // Extract just the project name from paths like "path/to/my-app"
        let project_name = extract_project_name(&self.name);
        let project_dir = self.output.as_ref().unwrap_or(&self.name);
        let path = Path::new(project_dir);

        if path.exists() {
            anyhow::bail!("Directory already exists: {}", project_dir);
        }

        fs::create_dir_all(path)?;
        create_project(path, &project_name, self.demo, self.target)?;

        // Generate lockfiles before git commit (unless --no-lock)
        if !self.no_lock {
            generate_bun_lockfile(path)?;
            generate_cargo_lockfile(path)?;
        }

        // Generate frontend types
        run_forge_generate(path)?;

        // Run formatters before git commit
        run_formatters(path)?;

        // Install forge-idiomatic-engineer skill for AI agents
        install_skill(path, self.include_skill).await?;

        // Initialize git repository if git is available
        if is_git_available() {
            init_git_repo(path)?;
        }

        println!();
        println!(
            "{} Created new FORGE project: {}",
            ui::ok(),
            style(&project_name).cyan()
        );
        ui::section("Next Steps");
        println!("  1. {}", style(format!("cd {}", project_dir)).cyan());
        println!("  2. {}", style("forge dev").cyan());
        println!("     Start development environment (requires Docker)");
        if self.target == FrontendTarget::Dioxus {
            println!(
                "  3. {}",
                style("cd frontend && dx serve").cyan()
            );
            println!("     Start the Dioxus frontend natively (web by default)");
        }

        ui::section("Useful Commands");
        ui::command("forge dev down", "Stop the development environment");
        ui::command(
            "forge dev down --clear",
            "Stop and remove volumes + target/",
        );

        ui::section("Default Service URLs");
        if self.target != FrontendTarget::Dioxus {
            ui::kv("Frontend", "http://localhost:5173");
        }
        ui::kv("Backend", "http://localhost:8080");
        ui::kv("Grafana", "http://localhost:3000");

        ui::section("Docs");
        println!("  {} https://tryforge.dev/docs", ui::info());
        println!();

        Ok(())
    }
}

/// Create project files in the given directory.
///
/// - `demo = true`: Full demo project with example code
/// - `demo = false`: Minimal scaffolding without example code
pub fn create_project(dir: &Path, name: &str, demo: bool, target: FrontendTarget) -> Result<()> {
    let vars = template_vars!("name" => name, "project_name" => name);
    let mode = if demo {
        ScaffoldMode::Demo
    } else {
        ScaffoldMode::Minimal
    };

    // Create directory structure
    fs::create_dir_all(dir.join("src/schema"))?;
    fs::create_dir_all(dir.join("src/functions"))?;
    fs::create_dir_all(dir.join("migrations"))?;

    if demo {
        fs::write(dir.join("Cargo.toml"), render(CARGO_TOML, &vars))?;

        // In debug builds, patch for local forge development
        #[cfg(debug_assertions)]
        {
            append_cargo_patch(&dir.join("Cargo.toml"))?;
            println!("  {} Added cargo patch for local development", ui::step());
        }

        fs::write(dir.join("forge.toml"), render(FORGE_TOML, &vars))?;
        fs::write(dir.join("sqlx.toml"), SQLX_TOML)?;
        fs::write(dir.join(".gitignore"), GITIGNORE)?;
        fs::write(dir.join(".ignore"), IGNORE)?;
        fs::write(dir.join(".env"), render(ENV, &vars))?;
        write_template_files(dir, target.project_templates(mode), &vars)?;

        #[cfg(debug_assertions)]
        patch_docker_compose(&dir.join("docker-compose.yml"))?;
        fs::write(dir.join("migrations/0001_initial.sql"), MIGRATION_INITIAL)?;
        fs::write(dir.join("src/schema/mod.rs"), SCHEMA_MOD)?;
        fs::write(dir.join("src/schema/user.rs"), SCHEMA_USER)?;
        fs::write(dir.join("src/functions/mod.rs"), FUNCTIONS_MOD)?;
        fs::write(dir.join("src/functions/users.rs"), FUNCTIONS_USERS)?;
        fs::write(dir.join("src/functions/iss.rs"), FUNCTIONS_ISS)?;
        fs::write(dir.join("src/functions/trades.rs"), FUNCTIONS_TRADES)?;
        fs::write(dir.join("src/functions/export.rs"), FUNCTIONS_EXPORT)?;
        fs::write(
            dir.join("src/functions/verification.rs"),
            FUNCTIONS_VERIFICATION,
        )?;
        fs::write(dir.join("src/functions/webhook.rs"), FUNCTIONS_WEBHOOK)?;
        // Demo frontend
        create_frontend(dir, name, true, target)?;
    } else {
        fs::write(dir.join("Cargo.toml"), render(EMPTY_CARGO_TOML, &vars))?;

        // In debug builds, patch for local forge development
        #[cfg(debug_assertions)]
        {
            append_cargo_patch(&dir.join("Cargo.toml"))?;
            println!("  {} Added cargo patch for local development", ui::step());
        }

        fs::write(dir.join("forge.toml"), render(EMPTY_FORGE_TOML, &vars))?;
        fs::write(dir.join("sqlx.toml"), SQLX_TOML)?;
        fs::write(dir.join(".gitignore"), EMPTY_GITIGNORE)?;
        fs::write(dir.join(".ignore"), EMPTY_IGNORE)?;
        fs::write(dir.join(".env"), render(EMPTY_ENV, &vars))?;
        fs::write(dir.join(".env.example"), render(EMPTY_ENV, &vars))?;
        write_template_files(dir, target.project_templates(mode), &vars)?;

        #[cfg(debug_assertions)]
        patch_docker_compose(&dir.join("docker-compose.yml"))?;
        fs::write(
            dir.join("migrations/0001_initial.sql.example"),
            EMPTY_MIGRATION_INITIAL,
        )?;
        fs::write(dir.join("src/schema/mod.rs"), EMPTY_SCHEMA_MOD)?;
        fs::write(dir.join("src/functions/mod.rs"), EMPTY_FUNCTIONS_MOD)?;
        // Minimal frontend
        create_frontend(dir, name, false, target)?;
    }

    Ok(())
}

/// Create frontend scaffolding.
///
/// - `demo = true`: Full demo frontend with complete UI
/// - `demo = false`: Minimal frontend with starter page
fn create_frontend(dir: &Path, name: &str, demo: bool, target: FrontendTarget) -> Result<()> {
    let version = env!("CARGO_PKG_VERSION");
    let vars = template_vars!("name" => name, "project_name" => name, "version" => version);
    let mode = if demo {
        ScaffoldMode::Demo
    } else {
        ScaffoldMode::Minimal
    };
    let templates = target.frontend_templates(mode);

    let frontend_dir = dir.join("frontend");
    fs::create_dir_all(&frontend_dir)?;
    fs::create_dir_all(frontend_dir.join("tests"))?;

    for relative_dir in templates.directories {
        fs::create_dir_all(frontend_dir.join(relative_dir))?;
    }

    write_template_files(&frontend_dir, shared_frontend_templates(), &vars)?;
    write_template_files(&frontend_dir, templates.files, &vars)?;

    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_extract_project_name() {
        // Simple name
        assert_eq!(extract_project_name("my-app"), "my-app");

        // Path with slashes
        assert_eq!(extract_project_name("path/to/my-app"), "my-app");
        assert_eq!(extract_project_name("./my-app"), "my-app");
        assert_eq!(extract_project_name("../my-app"), "my-app");

        // Absolute path
        assert_eq!(extract_project_name("/home/user/projects/my-app"), "my-app");

        // Trailing slash (edge case)
        assert_eq!(extract_project_name("my-app/"), "my-app");
    }

    #[test]
    fn test_create_demo_project() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test-demo");
        fs::create_dir_all(&path).unwrap();

        create_project(&path, "test-demo", true, FrontendTarget::SvelteKit).unwrap();

        // All demo files should exist
        assert!(path.join("Cargo.toml").exists());
        assert!(path.join("forge.toml").exists());
        assert!(path.join("sqlx.toml").exists());
        assert!(path.join("src/main.rs").exists());
        assert!(path.join("build.rs").exists());
        assert!(path.join("src/schema/mod.rs").exists());
        assert!(path.join("src/schema/user.rs").exists());
        assert!(path.join("src/functions/users.rs").exists());
        assert!(path.join("src/functions/iss.rs").exists());
        assert!(path.join("src/functions/trades.rs").exists());
        assert!(path.join("src/functions/export.rs").exists());
        assert!(path.join("src/functions/verification.rs").exists());
        assert!(path.join("src/functions/webhook.rs").exists());
        assert!(path.join("frontend/package.json").exists());
        assert!(path.join("frontend/src/routes/+layout.ts").exists());
        // Note: frontend/src/lib/forge/ files are generated by `forge generate`, not scaffolded
        assert!(path.join("frontend/eslint.config.js").exists());
        assert!(path.join("migrations/0001_initial.sql").exists());
        assert!(path.join("Dockerfile").exists());
        assert!(path.join("docker-compose.yml").exists());
        assert!(path.join("README.md").exists());
        // Playwright test files
        assert!(path.join("frontend/playwright.config.ts").exists());
        assert!(path.join("frontend/tests/global-setup.ts").exists());
        assert!(path.join("frontend/tests/fixtures.ts").exists());
        assert!(path.join("frontend/tests/home.spec.ts").exists());
        // Observability baked into Docker image, not scaffolded
        assert!(!path.join("grafana").exists());
    }

    #[test]
    fn test_create_minimal_project() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test-minimal");
        fs::create_dir_all(&path).unwrap();

        create_project(&path, "test-minimal", false, FrontendTarget::SvelteKit).unwrap();

        // Core files should exist
        assert!(path.join("Cargo.toml").exists());
        assert!(path.join("forge.toml").exists());
        assert!(path.join("sqlx.toml").exists());
        assert!(path.join("src/main.rs").exists());
        assert!(path.join("src/schema/mod.rs").exists());
        assert!(path.join("src/functions/mod.rs").exists());
        assert!(path.join("migrations/0001_initial.sql.example").exists());

        // Example files should NOT exist
        assert!(!path.join("src/schema/user.rs").exists());
        assert!(!path.join("src/functions/users.rs").exists());
        assert!(!path.join("src/functions/iss.rs").exists());
        assert!(!path.join("src/functions/trades.rs").exists());
        assert!(!path.join("src/functions/export.rs").exists());
        assert!(!path.join("src/functions/verification.rs").exists());
        assert!(!path.join("src/functions/webhook.rs").exists());

        // Frontend should exist with minimal templates
        assert!(path.join("frontend/package.json").exists());
        // Note: frontend/src/lib/forge/ files are generated by `forge generate`, not scaffolded
        // Playwright test files
        assert!(path.join("frontend/playwright.config.ts").exists());
        assert!(path.join("frontend/tests/global-setup.ts").exists());
        assert!(path.join("frontend/tests/fixtures.ts").exists());
        assert!(path.join("frontend/tests/home.spec.ts").exists());
        // Observability baked into Docker image, not scaffolded
        assert!(!path.join("grafana").exists());
    }

    #[test]
    fn test_create_minimal_dioxus_project() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test-minimal-dioxus");
        fs::create_dir_all(&path).unwrap();

        create_project(&path, "test-minimal-dioxus", false, FrontendTarget::Dioxus).unwrap();

        assert!(path.join("frontend/Cargo.toml").exists());
        assert!(path.join("frontend/Dioxus.toml").exists());
        assert!(path.join("frontend/src/main.rs").exists());
    }
}
