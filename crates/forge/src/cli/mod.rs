mod add;
mod check;
mod dev;
mod generate;
mod migrate;
mod new;
mod run;
mod runtime_generator;
mod template;

pub use add::AddCommand;
pub use check::CheckCommand;
pub use dev::DevCommand;
pub use generate::GenerateCommand;
pub use migrate::MigrateCommand;
pub use new::NewCommand;
pub use run::RunCommand;

use anyhow::Result;
use clap::{Parser, Subcommand};

const ABOUT: &str = r#"FORGE - The Full-Stack Framework for the Impatient

Everything you need in one binary. No Redis, no Kafka, just PostgreSQL.

Quick Start:
  forge new my-app --demo     Create a demo project with examples
  forge new my-app --minimal  Create a clean project to start fresh
  cd my-app
  forge dev                   Start development environment

Learn more: https://tryforge.dev/docs"#;

const AFTER_HELP: &str = r#"Examples:
  forge new my-app --demo        Full demo with User CRUD, jobs, workflows
  forge new my-app --minimal     Clean slate with just the structure
  forge check                    Validate project setup
  forge dev                      Start backend + frontend
  forge add query get_users      Add a new query function
  forge generate                 Regenerate TypeScript types
  forge migrate status           Check migration status

Dashboard: http://localhost:8080/_dashboard (when running)"#;

/// FORGE - The Full-Stack Framework for the Impatient
#[derive(Parser)]
#[command(name = "forge")]
#[command(author, version)]
#[command(about = ABOUT, long_about = None, after_help = AFTER_HELP)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

/// CLI commands.
#[derive(Subcommand)]
pub enum Commands {
    /// Create a new FORGE project
    New(NewCommand),

    /// Initialize FORGE in an existing directory
    Init(InitCommand),

    /// Validate project setup and dependencies
    Check(CheckCommand),

    /// Start development environment (backend + frontend)
    Dev(DevCommand),

    /// Add a new component (model, query, mutation, etc.)
    Add(AddCommand),

    /// Generate TypeScript client code
    Generate(GenerateCommand),

    /// Run the FORGE server
    Run(RunCommand),

    /// Manage database migrations
    Migrate(MigrateCommand),
}

/// Initialize in existing directory.
#[derive(Parser)]
pub struct InitCommand {
    /// Project name (defaults to directory name).
    #[arg(short, long)]
    pub name: Option<String>,

    /// Create a full demo project with example code.
    #[arg(long, conflicts_with = "minimal")]
    pub demo: bool,

    /// Create a clean project with minimal scaffolding.
    #[arg(long, conflicts_with = "demo")]
    pub minimal: bool,
}

impl Cli {
    /// Execute the CLI command.
    pub async fn execute(self) -> Result<()> {
        match self.command {
            Commands::New(cmd) => cmd.execute().await,
            Commands::Init(cmd) => init_project(cmd).await,
            Commands::Check(cmd) => cmd.execute().await,
            Commands::Dev(cmd) => cmd.execute().await,
            Commands::Add(cmd) => cmd.execute().await,
            Commands::Generate(cmd) => cmd.execute().await,
            Commands::Run(cmd) => cmd.execute().await,
            Commands::Migrate(cmd) => cmd.execute().await,
        }
    }
}

/// Initialize a new project in the current directory.
async fn init_project(cmd: InitCommand) -> Result<()> {
    use console::style;

    // Require either --demo or --minimal
    if !cmd.demo && !cmd.minimal {
        eprintln!(
            "{}",
            style("Error: You must specify a template mode")
                .red()
                .bold()
        );
        eprintln!();
        eprintln!("Choose one of:");
        eprintln!("  {} - Full demo with examples", style("--demo").cyan());
        eprintln!("  {} - Clean slate", style("--minimal").cyan());
        eprintln!();
        eprintln!(
            "Example: {} or {}",
            style("forge init --demo").green(),
            style("forge init --minimal").green()
        );
        std::process::exit(1);
    }

    let current_dir = std::env::current_dir()?;
    let name = cmd.name.unwrap_or_else(|| {
        current_dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("forge-app")
            .to_string()
    });

    new::create_project(&current_dir, &name, cmd.demo)?;
    println!("✅ Initialized FORGE project: {}", name);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parse_new_demo() {
        let cli = Cli::try_parse_from(["forge", "new", "my-app", "--demo"]);
        assert!(cli.is_ok());
    }

    #[test]
    fn test_cli_parse_new_minimal() {
        let cli = Cli::try_parse_from(["forge", "new", "my-app", "--minimal"]);
        assert!(cli.is_ok());
    }

    #[test]
    fn test_cli_parse_new_conflicting_flags() {
        // --demo and --minimal are mutually exclusive
        let cli = Cli::try_parse_from(["forge", "new", "my-app", "--demo", "--minimal"]);
        assert!(cli.is_err());
    }

    #[test]
    fn test_cli_parse_add() {
        let cli = Cli::try_parse_from(["forge", "add", "query", "get_users"]);
        assert!(cli.is_ok());
    }
}
