mod add;
mod check;
mod dev;
mod generate;
mod migrate;
mod new;
mod runtime_generator;
mod template;
mod ui;

pub use add::AddCommand;
pub use check::CheckCommand;
pub use dev::DevCommand;
pub use generate::GenerateCommand;
pub use migrate::MigrateCommand;
pub use new::NewCommand;

use anyhow::Result;
use clap::{Parser, Subcommand};

const ABOUT: &str = r#"FORGE - The Full-Stack Framework for the Impatient

Everything you need in one binary. No Redis, no Kafka, just PostgreSQL.

Quick Start:
  forge new my-app --demo   Create a demo project with examples
  cd my-app
  forge dev                 Start development (docker compose)

Learn more: https://tryforge.dev/docs"#;

const AFTER_HELP: &str = r#"Examples:
  forge new my-app --demo        Full demo with User CRUD, jobs, workflows
  forge new my-app --minimal     Clean slate with just the structure
  forge dev                      Start development (requires Docker)
  forge dev down                 Stop the development environment
  forge dev down --clear         Stop and remove volumes + target/
  forge check                    Validate project configuration
  forge add query get_users      Add a new query function
  forge generate                 Regenerate TypeScript types
  forge migrate status           Check migration status
"#;

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

    /// Validate project configuration and dependencies
    Check(CheckCommand),

    /// Start development environment with Docker Compose
    Dev(DevCommand),

    /// Add a new component (model, query, mutation, etc.)
    Add(AddCommand),

    /// Generate TypeScript client code
    Generate(GenerateCommand),

    /// Manage database migrations
    Migrate(MigrateCommand),
}

impl Cli {
    /// Execute the CLI command.
    pub async fn execute(self) -> Result<()> {
        match self.command {
            Commands::New(cmd) => cmd.execute().await,
            Commands::Check(cmd) => cmd.execute().await,
            Commands::Dev(cmd) => cmd.execute().await,
            Commands::Add(cmd) => cmd.execute().await,
            Commands::Generate(cmd) => cmd.execute().await,
            Commands::Migrate(cmd) => cmd.execute().await,
        }
    }
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
