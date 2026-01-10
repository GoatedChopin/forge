use anyhow::Result;
use clap::Parser;
use console::style;
use std::fs;
use std::path::Path;
use std::process::Command;

use super::template::render;
use crate::template_vars;

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
    Command::new("git")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Check if Docker is installed.
fn check_docker() -> std::result::Result<String, String> {
    let output = Command::new("docker")
        .arg("--version")
        .output()
        .map_err(|_| "Docker is not installed. Install from https://docs.docker.com/get-docker/".to_string())?;

    if !output.status.success() {
        return Err("Failed to run docker --version".to_string());
    }

    let version_output = String::from_utf8_lossy(&output.stdout);
    // Parse "Docker version 24.0.0, ..." -> "24.0.0"
    let version_str = version_output
        .split_whitespace()
        .nth(2)
        .map(|s| s.trim_end_matches(','))
        .unwrap_or("unknown")
        .to_string();

    Ok(version_str)
}

/// Check if the directory is inside an existing git repository.
fn is_inside_git_repo(dir: &Path) -> bool {
    Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .current_dir(dir)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Generate bun.lock file using Docker.
/// Runs `bun install --lockfile-only` in the frontend directory.
fn generate_bun_lockfile(dir: &Path) -> Result<()> {
    let frontend_dir = dir.join("frontend");
    let abs_frontend = frontend_dir.canonicalize()?;

    println!(
        "  {} Generating bun.lock...",
        style("→").cyan()
    );

    let output = Command::new("docker")
        .args([
            "run",
            "--rm",
            "-v",
            &format!("{}:/app", abs_frontend.display()),
            "-w",
            "/app",
            "oven/bun:1",
            "bun",
            "install",
            "--lockfile-only",
        ])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!(
            "  {} Failed to generate bun.lock: {}",
            style("⚠").yellow(),
            stderr.trim()
        );
        // Non-fatal: continue without lockfile
        return Ok(());
    }

    println!(
        "  {} bun.lock generated",
        style("✓").green()
    );

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
    let init = Command::new("git")
        .args(["init"])
        .current_dir(dir)
        .output()?;

    if !init.status.success() {
        return Ok(()); // Silently skip if init fails
    }

    // git add .
    let add = Command::new("git")
        .args(["add", "."])
        .current_dir(dir)
        .output()?;

    if !add.status.success() {
        return Ok(());
    }

    // git commit
    let _ = Command::new("git")
        .args(["commit", "-m", "Initialize project with Forge"])
        .current_dir(dir)
        .output()?;

    Ok(())
}

// Populated project templates (default)
const CARGO_TOML: &str = include_str!("../../templates/populated/project/Cargo.toml.tmpl");
const FORGE_TOML: &str = include_str!("../../templates/populated/project/forge.toml.tmpl");
const MAIN_RS: &str = include_str!("../../templates/populated/project/main.rs.tmpl");
const BUILD_RS: &str = include_str!("../../templates/populated/project/build.rs.tmpl");
const GITIGNORE: &str = include_str!("../../templates/populated/project/gitignore.tmpl");
const ENV: &str = include_str!("../../templates/populated/project/env.tmpl");
const DOCKERFILE: &str = include_str!("../../templates/populated/project/Dockerfile.tmpl");
const DOCKER_COMPOSE: &str =
    include_str!("../../templates/populated/project/docker-compose.yml.tmpl");
const README: &str = include_str!("../../templates/populated/project/README.md.tmpl");
const MIGRATION_INITIAL: &str =
    include_str!("../../templates/populated/project/migrations/0001_initial.sql.tmpl");
const SCHEMA_MOD: &str = include_str!("../../templates/populated/project/schema/mod.rs.tmpl");
const SCHEMA_USER: &str = include_str!("../../templates/populated/project/schema/user.rs.tmpl");
const FUNCTIONS_MOD: &str = include_str!("../../templates/populated/project/functions/mod.rs.tmpl");
const FUNCTIONS_USERS: &str =
    include_str!("../../templates/populated/project/functions/users.rs.tmpl");
const FUNCTIONS_APP_STATS: &str =
    include_str!("../../templates/populated/project/functions/app_stats.rs.tmpl");
const FUNCTIONS_EXPORT_USERS_JOB: &str =
    include_str!("../../templates/populated/project/functions/export_users_job.rs.tmpl");
const FUNCTIONS_HEARTBEAT_CRON: &str =
    include_str!("../../templates/populated/project/functions/heartbeat_stats_cron.rs.tmpl");
const FUNCTIONS_VERIFICATION_WORKFLOW: &str = include_str!(
    "../../templates/populated/project/functions/account_verification_workflow.rs.tmpl"
);
const FUNCTIONS_GET_BITCOIN_PRICE_ACTION: &str =
    include_str!("../../templates/populated/project/functions/get_bitcoin_price_action.rs.tmpl");
const AGENTS_MD: &str = include_str!("../../templates/populated/project/AGENTS.md.tmpl");

// Populated frontend templates (default)
const FRONTEND_PACKAGE_JSON: &str =
    include_str!("../../templates/populated/frontend/package.json.tmpl");
const FRONTEND_SVELTE_CONFIG: &str =
    include_str!("../../templates/populated/frontend/svelte.config.js.tmpl");
const FRONTEND_VITE_CONFIG: &str =
    include_str!("../../templates/populated/frontend/vite.config.ts.tmpl");
const FRONTEND_TSCONFIG: &str =
    include_str!("../../templates/populated/frontend/tsconfig.json.tmpl");
const FRONTEND_APP_HTML: &str = include_str!("../../templates/populated/frontend/app.html.tmpl");
const FRONTEND_ENV_EXAMPLE: &str = include_str!("../../templates/populated/frontend/env.tmpl");
const FRONTEND_LAYOUT_SVELTE: &str =
    include_str!("../../templates/populated/frontend/routes/layout.svelte.tmpl");
const FRONTEND_LAYOUT_TS: &str =
    include_str!("../../templates/populated/frontend/routes/layout.ts.tmpl");
const FRONTEND_PAGE_SVELTE: &str =
    include_str!("../../templates/populated/frontend/routes/page.svelte.tmpl");
const FRONTEND_TYPES_TS: &str =
    include_str!("../../templates/populated/frontend/lib/forge/types.ts.tmpl");
const FRONTEND_API_TS: &str =
    include_str!("../../templates/populated/frontend/lib/forge/api.ts.tmpl");
const FRONTEND_INDEX_TS: &str =
    include_str!("../../templates/populated/frontend/lib/forge/index.ts.tmpl");
const FRONTEND_ESLINT_CONFIG: &str =
    include_str!("../../templates/populated/frontend/eslint.config.js.tmpl");

// Empty project templates (for --empty flag)
const EMPTY_CARGO_TOML: &str = include_str!("../../templates/empty/project/Cargo.toml.tmpl");
const EMPTY_FORGE_TOML: &str = include_str!("../../templates/empty/project/forge.toml.tmpl");
const EMPTY_MAIN_RS: &str = include_str!("../../templates/empty/project/main.rs.tmpl");
const EMPTY_BUILD_RS: &str = include_str!("../../templates/empty/project/build.rs.tmpl");
const EMPTY_GITIGNORE: &str = include_str!("../../templates/empty/project/gitignore.tmpl");
const EMPTY_ENV: &str = include_str!("../../templates/empty/project/env.tmpl");
const EMPTY_DOCKERFILE: &str = include_str!("../../templates/empty/project/Dockerfile.tmpl");
const EMPTY_DOCKER_COMPOSE: &str =
    include_str!("../../templates/empty/project/docker-compose.yml.tmpl");
const EMPTY_README: &str = include_str!("../../templates/empty/project/README.md.tmpl");
const EMPTY_MIGRATION_INITIAL: &str =
    include_str!("../../templates/empty/project/migrations/0001_initial.sql.tmpl");
const EMPTY_SCHEMA_MOD: &str = include_str!("../../templates/empty/project/schema/mod.rs.tmpl");
const EMPTY_FUNCTIONS_MOD: &str =
    include_str!("../../templates/empty/project/functions/mod.rs.tmpl");
const EMPTY_AGENTS_MD: &str = include_str!("../../templates/empty/project/AGENTS.md.tmpl");

// Empty frontend templates (for --empty flag)
const EMPTY_FRONTEND_PACKAGE_JSON: &str =
    include_str!("../../templates/empty/frontend/package.json.tmpl");
const EMPTY_FRONTEND_SVELTE_CONFIG: &str =
    include_str!("../../templates/empty/frontend/svelte.config.js.tmpl");
const EMPTY_FRONTEND_VITE_CONFIG: &str =
    include_str!("../../templates/empty/frontend/vite.config.ts.tmpl");
const EMPTY_FRONTEND_TSCONFIG: &str =
    include_str!("../../templates/empty/frontend/tsconfig.json.tmpl");
const EMPTY_FRONTEND_APP_HTML: &str = include_str!("../../templates/empty/frontend/app.html.tmpl");
const EMPTY_FRONTEND_ENV_EXAMPLE: &str = include_str!("../../templates/empty/frontend/env.tmpl");
const EMPTY_FRONTEND_LAYOUT_SVELTE: &str =
    include_str!("../../templates/empty/frontend/routes/layout.svelte.tmpl");
const EMPTY_FRONTEND_LAYOUT_TS: &str =
    include_str!("../../templates/empty/frontend/routes/layout.ts.tmpl");
const EMPTY_FRONTEND_PAGE_SVELTE: &str =
    include_str!("../../templates/empty/frontend/routes/page.svelte.tmpl");
const EMPTY_FRONTEND_TYPES_TS: &str =
    include_str!("../../templates/empty/frontend/lib/forge/types.ts.tmpl");
const EMPTY_FRONTEND_API_TS: &str =
    include_str!("../../templates/empty/frontend/lib/forge/api.ts.tmpl");
const EMPTY_FRONTEND_INDEX_TS: &str =
    include_str!("../../templates/empty/frontend/lib/forge/index.ts.tmpl");
const EMPTY_FRONTEND_ESLINT_CONFIG: &str =
    include_str!("../../templates/empty/frontend/eslint.config.js.tmpl");

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

    /// Skip generating bun.lock file before initial commit.
    ///
    /// By default, forge new runs `bun install --lockfile-only` in Docker
    /// to generate the bun.lock file before the initial git commit.
    /// Use this flag to skip lockfile generation.
    #[arg(long)]
    pub no_lock: bool,
}

const NEW_AFTER_HELP: &str = r#"TEMPLATE MODES:
  You must choose one of --demo or --minimal:

  --demo      Full demo project with working examples
              - User model with CRUD operations
              - Background job (export users)
              - Cron task (heartbeat stats)
              - Durable workflow (account verification)
              - External API action (Bitcoin price)
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
        // Check Docker is installed
        println!("{}", style("Checking prerequisites...").dim());

        let docker_version = match check_docker() {
            Ok(v) => v,
            Err(msg) => {
                eprintln!("{} {}", style("✗").red().bold(), style(&msg).red());
                std::process::exit(1);
            }
        };

        println!(
            "  {} Docker {}",
            style("✓").green(),
            docker_version,
        );
        println!();

        // Require either --demo or --minimal
        if !self.demo && !self.minimal {
            eprintln!(
                "{}",
                style("Error: You must specify a template mode")
                    .red()
                    .bold()
            );
            eprintln!();
            eprintln!("Choose one of:");
            eprintln!();
            eprintln!(
                "  {} {}",
                style("--demo").cyan().bold(),
                style("Full demo project with working examples").dim()
            );
            eprintln!("          User CRUD, jobs, crons, workflows, actions, and demo UI");
            eprintln!();
            eprintln!(
                "  {} {}",
                style("--minimal").cyan().bold(),
                style("Clean slate with just the structure").dim()
            );
            eprintln!("          Empty directories with commented examples, starter frontend");
            eprintln!();
            eprintln!("Examples:");
            eprintln!(
                "  {} {}",
                style("forge new my-app --demo").green(),
                style("# Learn FORGE with examples").dim()
            );
            eprintln!(
                "  {} {}",
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
        create_project(path, &project_name, self.demo)?;

        // Generate bun.lock before git commit (unless --no-lock)
        if !self.no_lock {
            generate_bun_lockfile(path)?;
        }

        // Initialize git repository if git is available
        if is_git_available() {
            init_git_repo(path)?;
        }

        println!();
        println!(
            "{} Created new FORGE project: {}",
            style("✅").green(),
            style(&project_name).cyan()
        );
        println!();
        println!("{}", style("Next steps:").bold());
        println!();
        println!(
            "  {} {}",
            style("1.").dim(),
            style(format!("cd {}", project_dir)).cyan()
        );
        println!();
        println!("  {} {}", style("2.").dim(), style("forge dev").cyan());
        println!("     Start backend + frontend + database with Docker");
        println!();
        println!("{}", style("Useful commands:").bold());
        println!(
            "  {}    Stop the development environment",
            style("forge dev down").dim()
        );
        println!(
            "  {}  Stop and remove database",
            style("forge dev down --clean").dim()
        );
        println!();
        println!("{}", style("Services (when running):").bold());
        println!("  Frontend:  http://localhost:5173");
        println!("  Backend:   http://localhost:8080");
        println!("  Dashboard: http://localhost:8080/_dashboard");
        println!();
        println!("{}", style("Documentation:").bold());
        println!("  https://tryforge.dev/docs");
        println!();

        Ok(())
    }
}

/// Create project files in the given directory.
///
/// - `demo = true`: Full demo project with example code
/// - `demo = false`: Minimal scaffolding without example code
pub fn create_project(dir: &Path, name: &str, demo: bool) -> Result<()> {
    let vars = template_vars!("name" => name, "project_name" => name);

    // Create directory structure
    fs::create_dir_all(dir.join("src/schema"))?;
    fs::create_dir_all(dir.join("src/functions"))?;
    fs::create_dir_all(dir.join("migrations"))?;

    if demo {
        // Demo templates - full example code
        fs::write(dir.join("Cargo.toml"), render(CARGO_TOML, &vars))?;
        fs::write(dir.join("forge.toml"), render(FORGE_TOML, &vars))?;
        fs::write(dir.join("build.rs"), BUILD_RS)?;
        fs::write(dir.join(".gitignore"), GITIGNORE)?;
        fs::write(dir.join(".env"), render(ENV, &vars))?;
        fs::write(dir.join("Dockerfile"), render(DOCKERFILE, &vars))?;
        fs::write(
            dir.join("docker-compose.yml"),
            render(DOCKER_COMPOSE, &vars),
        )?;
        fs::write(dir.join("README.md"), render(README, &vars))?;
        fs::write(dir.join("src/main.rs"), MAIN_RS)?;
        fs::write(dir.join("migrations/0001_initial.sql"), MIGRATION_INITIAL)?;
        fs::write(dir.join("src/schema/mod.rs"), SCHEMA_MOD)?;
        fs::write(dir.join("src/schema/user.rs"), SCHEMA_USER)?;
        fs::write(dir.join("src/functions/mod.rs"), FUNCTIONS_MOD)?;
        fs::write(dir.join("src/functions/users.rs"), FUNCTIONS_USERS)?;
        fs::write(dir.join("src/functions/app_stats.rs"), FUNCTIONS_APP_STATS)?;
        fs::write(
            dir.join("src/functions/export_users_job.rs"),
            FUNCTIONS_EXPORT_USERS_JOB,
        )?;
        fs::write(
            dir.join("src/functions/heartbeat_stats_cron.rs"),
            FUNCTIONS_HEARTBEAT_CRON,
        )?;
        fs::write(
            dir.join("src/functions/account_verification_workflow.rs"),
            FUNCTIONS_VERIFICATION_WORKFLOW,
        )?;
        fs::write(
            dir.join("src/functions/get_bitcoin_price_action.rs"),
            FUNCTIONS_GET_BITCOIN_PRICE_ACTION,
        )?;
        fs::write(dir.join("AGENTS.md"), AGENTS_MD)?;
        // Demo frontend
        create_frontend(dir, name, true)?;
    } else {
        // Minimal templates - clean scaffolding without example code
        fs::write(dir.join("Cargo.toml"), render(EMPTY_CARGO_TOML, &vars))?;
        fs::write(dir.join("forge.toml"), render(EMPTY_FORGE_TOML, &vars))?;
        fs::write(dir.join("build.rs"), EMPTY_BUILD_RS)?;
        fs::write(dir.join(".gitignore"), EMPTY_GITIGNORE)?;
        fs::write(dir.join(".env"), render(EMPTY_ENV, &vars))?;
        fs::write(dir.join(".env.example"), render(EMPTY_ENV, &vars))?;
        fs::write(dir.join("Dockerfile"), render(EMPTY_DOCKERFILE, &vars))?;
        fs::write(
            dir.join("docker-compose.yml"),
            render(EMPTY_DOCKER_COMPOSE, &vars),
        )?;
        fs::write(dir.join("README.md"), render(EMPTY_README, &vars))?;
        fs::write(dir.join("src/main.rs"), EMPTY_MAIN_RS)?;
        fs::write(
            dir.join("migrations/0001_initial.sql"),
            EMPTY_MIGRATION_INITIAL,
        )?;
        fs::write(dir.join("src/schema/mod.rs"), EMPTY_SCHEMA_MOD)?;
        fs::write(dir.join("src/functions/mod.rs"), EMPTY_FUNCTIONS_MOD)?;
        fs::write(dir.join("AGENTS.md"), EMPTY_AGENTS_MD)?;
        // Minimal frontend
        create_frontend(dir, name, false)?;
    }

    Ok(())
}

/// Create frontend scaffolding.
///
/// - `demo = true`: Full demo frontend with complete UI
/// - `demo = false`: Minimal frontend with starter page
fn create_frontend(dir: &Path, name: &str, demo: bool) -> Result<()> {
    let vars = template_vars!("name" => name, "project_name" => name);

    let frontend_dir = dir.join("frontend");
    fs::create_dir_all(&frontend_dir)?;
    fs::create_dir_all(frontend_dir.join("src/routes"))?;
    fs::create_dir_all(frontend_dir.join("src/lib/forge"))?;

    if demo {
        // Demo templates - full frontend with complete UI
        fs::write(
            frontend_dir.join("package.json"),
            render(FRONTEND_PACKAGE_JSON, &vars),
        )?;
        fs::write(
            frontend_dir.join("svelte.config.js"),
            FRONTEND_SVELTE_CONFIG,
        )?;
        fs::write(frontend_dir.join("vite.config.ts"), FRONTEND_VITE_CONFIG)?;
        fs::write(frontend_dir.join("tsconfig.json"), FRONTEND_TSCONFIG)?;
        fs::write(frontend_dir.join("src/app.html"), FRONTEND_APP_HTML)?;
        fs::write(frontend_dir.join(".env"), FRONTEND_ENV_EXAMPLE)?;
        fs::write(frontend_dir.join(".env.example"), FRONTEND_ENV_EXAMPLE)?;
        fs::write(
            frontend_dir.join("eslint.config.js"),
            FRONTEND_ESLINT_CONFIG,
        )?;
        fs::write(
            frontend_dir.join("src/routes/+layout.svelte"),
            FRONTEND_LAYOUT_SVELTE,
        )?;
        fs::write(
            frontend_dir.join("src/routes/+layout.ts"),
            FRONTEND_LAYOUT_TS,
        )?;
        fs::write(
            frontend_dir.join("src/routes/+page.svelte"),
            FRONTEND_PAGE_SVELTE,
        )?;
        fs::write(
            frontend_dir.join("src/lib/forge/types.ts"),
            FRONTEND_TYPES_TS,
        )?;
        fs::write(frontend_dir.join("src/lib/forge/api.ts"), FRONTEND_API_TS)?;
        fs::write(
            frontend_dir.join("src/lib/forge/index.ts"),
            FRONTEND_INDEX_TS,
        )?;
    } else {
        // Minimal templates - starter frontend
        fs::write(
            frontend_dir.join("package.json"),
            render(EMPTY_FRONTEND_PACKAGE_JSON, &vars),
        )?;
        fs::write(
            frontend_dir.join("svelte.config.js"),
            EMPTY_FRONTEND_SVELTE_CONFIG,
        )?;
        fs::write(
            frontend_dir.join("vite.config.ts"),
            EMPTY_FRONTEND_VITE_CONFIG,
        )?;
        fs::write(frontend_dir.join("tsconfig.json"), EMPTY_FRONTEND_TSCONFIG)?;
        fs::write(frontend_dir.join("src/app.html"), EMPTY_FRONTEND_APP_HTML)?;
        fs::write(frontend_dir.join(".env"), EMPTY_FRONTEND_ENV_EXAMPLE)?;
        fs::write(
            frontend_dir.join(".env.example"),
            EMPTY_FRONTEND_ENV_EXAMPLE,
        )?;
        fs::write(
            frontend_dir.join("eslint.config.js"),
            EMPTY_FRONTEND_ESLINT_CONFIG,
        )?;
        fs::write(
            frontend_dir.join("src/routes/+layout.svelte"),
            EMPTY_FRONTEND_LAYOUT_SVELTE,
        )?;
        fs::write(
            frontend_dir.join("src/routes/+layout.ts"),
            EMPTY_FRONTEND_LAYOUT_TS,
        )?;
        fs::write(
            frontend_dir.join("src/routes/+page.svelte"),
            render(EMPTY_FRONTEND_PAGE_SVELTE, &vars),
        )?;
        fs::write(
            frontend_dir.join("src/lib/forge/types.ts"),
            EMPTY_FRONTEND_TYPES_TS,
        )?;
        fs::write(
            frontend_dir.join("src/lib/forge/api.ts"),
            EMPTY_FRONTEND_API_TS,
        )?;
        fs::write(
            frontend_dir.join("src/lib/forge/index.ts"),
            EMPTY_FRONTEND_INDEX_TS,
        )?;
    }

    // Generate @forge/svelte runtime package
    super::runtime_generator::generate_runtime(&frontend_dir)?;

    Ok(())
}

#[cfg(test)]
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

        create_project(&path, "test-demo", true).unwrap();

        // All demo files should exist
        assert!(path.join("Cargo.toml").exists());
        assert!(path.join("forge.toml").exists());
        assert!(path.join("src/main.rs").exists());
        assert!(path.join("build.rs").exists());
        assert!(path.join("src/schema/mod.rs").exists());
        assert!(path.join("src/schema/user.rs").exists());
        assert!(path.join("src/functions/users.rs").exists());
        assert!(path.join("src/functions/export_users_job.rs").exists());
        assert!(path.join("src/functions/heartbeat_stats_cron.rs").exists());
        assert!(
            path.join("src/functions/account_verification_workflow.rs")
                .exists()
        );
        assert!(path.join("frontend/package.json").exists());
        assert!(path.join("frontend/src/lib/forge/types.ts").exists());
        assert!(path.join("frontend/src/lib/forge/api.ts").exists());
        assert!(path.join("frontend/src/routes/+layout.ts").exists());
        assert!(path.join("frontend/eslint.config.js").exists());
        assert!(path.join("migrations/0001_initial.sql").exists());
        assert!(path.join("Dockerfile").exists());
        assert!(path.join("docker-compose.yml").exists());
        assert!(path.join("README.md").exists());
        assert!(path.join("AGENTS.md").exists());
    }

    #[test]
    fn test_create_minimal_project() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test-minimal");
        fs::create_dir_all(&path).unwrap();

        create_project(&path, "test-minimal", false).unwrap();

        // Core files should exist
        assert!(path.join("Cargo.toml").exists());
        assert!(path.join("forge.toml").exists());
        assert!(path.join("src/main.rs").exists());
        assert!(path.join("src/schema/mod.rs").exists());
        assert!(path.join("src/functions/mod.rs").exists());
        assert!(path.join("migrations/0001_initial.sql").exists());
        assert!(path.join("AGENTS.md").exists());

        // Example files should NOT exist
        assert!(!path.join("src/schema/user.rs").exists());
        assert!(!path.join("src/functions/users.rs").exists());
        assert!(!path.join("src/functions/export_users_job.rs").exists());
        assert!(!path.join("src/functions/heartbeat_stats_cron.rs").exists());
        assert!(
            !path
                .join("src/functions/account_verification_workflow.rs")
                .exists()
        );

        // Frontend should exist with minimal templates
        assert!(path.join("frontend/package.json").exists());
        assert!(path.join("frontend/src/lib/forge/types.ts").exists());
        assert!(path.join("frontend/src/lib/forge/api.ts").exists());
    }
}
