use anyhow::Result;
use clap::{Parser, Subcommand};
use console::style;
use std::path::Path;

use forge_core::config::ForgeConfig;
use forge_runtime::Database;
use forge_runtime::migrations::{MigrationRunner, load_migrations_from_dir};

use super::ui;

/// Manage database migrations.
#[derive(Parser)]
pub struct MigrateCommand {
    #[command(subcommand)]
    pub action: MigrateAction,

    /// Configuration file path.
    #[arg(short, long, default_value = "forge.toml", global = true)]
    pub config: String,

    /// Migrations directory path.
    #[arg(short, long, default_value = "migrations", global = true)]
    pub migrations_dir: String,
}

#[derive(Subcommand)]
pub enum MigrateAction {
    /// Run all pending migrations (default behavior).
    Up,

    /// Rollback the last N migrations.
    Down {
        /// Number of migrations to rollback.
        #[arg(default_value = "1")]
        count: usize,
    },

    /// Show migration status.
    Status,
}

impl MigrateCommand {
    pub async fn execute(self) -> Result<()> {
        // Load .env if present
        dotenvy::dotenv().ok();

        // Load configuration
        let config_path = Path::new(&self.config);
        if !config_path.exists() {
            anyhow::bail!(
                "Configuration file not found: {}\nRun `forge new` or `forge init` to create a project.",
                self.config
            );
        }

        let config = ForgeConfig::from_file(&self.config)?;

        // Connect to database
        let db = Database::from_config_with_service(&config.database, &config.project.name).await?;
        let pool = db.primary().clone();
        let runner = MigrationRunner::new(pool);

        // Load available migrations
        let migrations_dir = Path::new(&self.migrations_dir);
        let available = load_migrations_from_dir(migrations_dir)?;

        match self.action {
            MigrateAction::Up => {
                ui::section("FORGE Migrations");

                if available.is_empty() {
                    println!(
                        "  {} No migrations found in {}",
                        ui::info(),
                        self.migrations_dir
                    );
                    return Ok(());
                }

                println!("  {} Running pending migrations...", ui::step());
                runner.run(available).await?;
                println!("  {} Migrations complete", ui::ok());
                println!();
            }

            MigrateAction::Down { count } => {
                ui::section("FORGE Migrations");

                if count == 0 {
                    println!("  {} Nothing to rollback (count=0)", ui::info());
                    return Ok(());
                }

                println!("  {} Rolling back {} migration(s)...", ui::step(), count);

                let rolled_back = runner.rollback(count).await?;

                if rolled_back.is_empty() {
                    println!("  {} No migrations to rollback", ui::info());
                } else {
                    for name in &rolled_back {
                        println!("  {} Rolled back: {}", ui::ok(), name);
                    }
                    println!();
                    println!(
                        "  {} Rolled back {} migration(s)",
                        ui::ok(),
                        rolled_back.len()
                    );
                }
                println!();
            }

            MigrateAction::Status => {
                ui::section("FORGE Migration Status");

                let status = runner.status(&available).await?;

                if status.applied.is_empty() && status.pending.is_empty() {
                    println!("  {} No migrations found", ui::info());
                    return Ok(());
                }

                // Show applied migrations
                if !status.applied.is_empty() {
                    println!("  {} Applied:", ui::ok());
                    for m in &status.applied {
                        let down_marker = if m.has_down {
                            style("↓").green().to_string()
                        } else {
                            style("-").dim().to_string()
                        };
                        println!(
                            "    {} {} {} ({})",
                            down_marker,
                            style(&m.name).cyan(),
                            style("at").dim(),
                            m.applied_at.format("%Y-%m-%d %H:%M:%S")
                        );
                    }
                }

                // Show pending migrations
                if !status.pending.is_empty() {
                    if !status.applied.is_empty() {
                        println!();
                    }
                    println!("  {} Pending:", ui::warn());
                    for name in &status.pending {
                        println!("    {} {}", ui::step(), style(name).yellow());
                    }
                }

                println!();
                println!(
                    "  {} {} applied, {} pending",
                    ui::info(),
                    status.applied.len(),
                    status.pending.len()
                );
                println!();

                // Legend
                println!(
                    "  {} = has down migration, {} = no down migration",
                    style("↓").green(),
                    style("-").dim()
                );
                println!();
            }
        }

        Ok(())
    }
}
