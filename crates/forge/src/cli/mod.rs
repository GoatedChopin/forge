mod check;
mod dev;
mod frontend_codegen;
mod frontend_target;
mod generate;
mod migrate;
mod new;
mod template;
mod template_catalog;
mod test;
mod ui;

pub use check::CheckCommand;
pub use dev::DevCommand;
pub use generate::GenerateCommand;
pub use migrate::MigrateCommand;
pub use new::NewCommand;
pub use test::TestCommand;

use anyhow::Result;
use clap::{Parser, Subcommand};

const ABOUT: &str = r#"FORGE - The Full-Stack Framework for the Impatient

Everything you need in one binary. No Redis, no Kafka, just PostgreSQL.

Quick Start:
  forge new my-app --template with-svelte/minimal
  cd my-app
  forge dev                 Start development (docker compose)

Learn more: https://tryforge.dev/docs"#;

const AFTER_HELP: &str = r#"Examples:
  forge new my-app --template with-svelte/minimal
  forge new my-app --template with-dioxus/realtime-todo-list
  forge dev                      Start development (requires Docker)
  forge dev down                 Stop the development environment
  forge dev down --clear         Stop and remove volumes + target/
  forge test                     Run Playwright tests
  forge test --ui                Run tests in Playwright UI mode
  forge check                    Validate project configuration
  forge generate                 Generate frontend bindings from backend code
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

    /// Generate frontend bindings from backend source
    Generate(GenerateCommand),

    /// Run Playwright frontend tests
    Test(TestCommand),

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
            Commands::Test(cmd) => cmd.execute().await,
            Commands::Generate(cmd) => cmd.execute().await,
            Commands::Migrate(cmd) => cmd.execute().await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parse_new_template() {
        let cli = Cli::try_parse_from([
            "forge",
            "new",
            "my-app",
            "--template",
            "with-svelte/minimal",
        ]);
        assert!(cli.is_ok());
    }

    #[test]
    fn test_cli_rejects_removed_demo_flag() {
        let cli = Cli::try_parse_from(["forge", "new", "my-app", "--demo"]);
        assert!(cli.is_err());
    }

    #[test]
    fn test_cli_rejects_removed_minimal_flag() {
        let cli = Cli::try_parse_from(["forge", "new", "my-app", "--minimal"]);
        assert!(cli.is_err());
    }

    #[test]
    fn test_cli_rejects_removed_target_flag() {
        let cli = Cli::try_parse_from([
            "forge",
            "new",
            "my-app",
            "--template",
            "with-svelte/minimal",
            "--target",
            "dioxus",
        ]);
        assert!(cli.is_err());
    }

    #[test]
    fn test_cli_requires_template_for_new() {
        let cli = Cli::try_parse_from(["forge", "new", "my-app"]);
        assert!(cli.is_err());
    }

    #[test]
    fn test_cli_parse_generate() {
        let cli = Cli::try_parse_from(["forge", "generate"]);
        assert!(cli.is_ok());
    }

    #[test]
    fn test_cli_parse_test() {
        let cli = Cli::try_parse_from(["forge", "test"]);
        assert!(cli.is_ok());
    }

    #[test]
    fn test_cli_parse_test_ui() {
        let cli = Cli::try_parse_from(["forge", "test", "--ui"]);
        assert!(cli.is_ok());
    }

    #[test]
    fn test_cli_parse_test_with_args() {
        let cli = Cli::try_parse_from(["forge", "test", "tests/home.spec.ts", "--headed"]);
        assert!(cli.is_ok());
    }
}
