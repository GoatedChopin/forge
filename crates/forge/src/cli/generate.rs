use anyhow::Result;
use clap::Parser;
use console::style;
use std::io::{self, Write};
use std::path::Path;

use super::frontend_codegen::BindingGeneratorInput;
use super::frontend_target::FrontendTarget;
use super::runtime_generator::{
    FORGE_VERSION, generate_runtime, get_installed_version, has_legacy_runtime, needs_update,
    remove_legacy_runtime,
};
use super::ui;

/// Generate TypeScript client code.
#[derive(Parser)]
pub struct GenerateCommand {
    /// Force regeneration even if files exist.
    #[arg(long)]
    pub force: bool,

    /// Output directory (defaults to frontend/src/lib/forge).
    #[arg(short, long)]
    pub output: Option<String>,

    /// Frontend target (`sveltekit` or `dioxus`). Defaults to auto-detection.
    #[arg(long)]
    pub target: Option<FrontendTarget>,

    /// Source directory to scan for models (defaults to src).
    #[arg(short, long)]
    pub src: Option<String>,

    /// Skip runtime regeneration (only regenerate types).
    #[arg(long)]
    pub skip_runtime: bool,

    /// Auto-accept prompts (useful for CI).
    #[arg(short = 'y', long)]
    pub yes: bool,
}

impl GenerateCommand {
    /// Execute the generate command.
    pub async fn execute(self) -> Result<()> {
        let src_dir = self.src.unwrap_or_else(|| "src".to_string());
        let src_path = Path::new(&src_dir);

        let detected_target = self
            .target
            .or_else(|| FrontendTarget::detect(Path::new("frontend")))
            .unwrap_or(FrontendTarget::SvelteKit);
        let output_dir = self
            .output
            .unwrap_or_else(|| detected_target.default_output_dir().to_string());
        let output_path = Path::new(&output_dir);
        let frontend_dir = Path::new("frontend");

        // Step 1: Check for legacy runtime and handle migration
        eprint!("  Checking project structure...");
        if detected_target == FrontendTarget::SvelteKit && has_legacy_runtime(frontend_dir) {
            eprintln!();

            ui::section("Runtime Migration");
            println!("{} Legacy project structure detected.", ui::warn());
            println!();
            println!("  This project uses the old embedded runtime structure.");
            println!("  Migration to the new .forge/ package structure is recommended.");
            println!();

            if !self.yes {
                print!("  Migrate to new structure? [Y/n] ");
                io::stdout().flush()?;

                let mut input = String::new();
                io::stdin().read_line(&mut input)?;
                let input = input.trim().to_lowercase();

                if input == "n" || input == "no" {
                    println!();
                    println!(
                        "{} Migration declined. Use --skip-runtime to only regenerate types.",
                        ui::info()
                    );
                    return Ok(());
                }
            }

            println!();
            println!("  {} Migrating...", ui::step());

            // Remove legacy runtime
            remove_legacy_runtime(frontend_dir)?;
            println!(
                "  {} Removed old src/lib/forge/runtime/ directory",
                ui::ok()
            );

            // Generate new runtime
            generate_runtime(frontend_dir, FrontendTarget::SvelteKit)?;
            println!("  {} Created .forge/svelte/ package", ui::ok());

            // Update package.json
            FrontendTarget::SvelteKit.update_frontend_manifest(frontend_dir)?;
            println!(
                "  {} Updated package.json with @forge/svelte dependency",
                ui::ok()
            );

            println!();
            println!(
                "  {} Migration complete! Please run: {}",
                ui::ok(),
                style("bun install").cyan()
            );
            println!();
        }

        // Step 2: Check runtime version and update if needed
        if !self.skip_runtime {
            let forge_dir_exists = frontend_dir
                .join(".forge")
                .join(detected_target.runtime_dir_name())
                .exists();

            if forge_dir_exists && needs_update(frontend_dir, detected_target) {
                let installed =
                    get_installed_version(frontend_dir).unwrap_or_else(|| "unknown".to_string());
                let version_changed = installed != FORGE_VERSION;

                if version_changed {
                    println!();
                    println!("{} Version mismatch detected:", ui::warn());
                    println!("    - Project runtime: v{}", style(&installed).cyan());
                    println!("    - Forge CLI: v{}", style(FORGE_VERSION).cyan());
                    println!();

                    if !self.yes {
                        print!(
                            "  This will update the @forge/svelte runtime to v{}. Continue? [Y/n] ",
                            FORGE_VERSION
                        );
                        io::stdout().flush()?;

                        let mut input = String::new();
                        io::stdin().read_line(&mut input)?;
                        let input = input.trim().to_lowercase();

                        if input == "n" || input == "no" {
                            println!();
                            println!(
                                "{} Update declined. Use --skip-runtime to only regenerate bindings.",
                                ui::info()
                            );
                            return Ok(());
                        }
                    }
                }

                eprint!(
                    "  Regenerating {} runtime...",
                    detected_target.display_name()
                );
                generate_runtime(frontend_dir, detected_target)?;
                eprintln!(" done");

                if version_changed {
                    println!();
                    println!(
                        "  {} Updated {} runtime (v{} → v{})",
                        ui::ok(),
                        detected_target.display_name(),
                        installed,
                        FORGE_VERSION
                    );
                }
            } else if !forge_dir_exists {
                // First time generation
                eprint!("  Generating {} runtime...", detected_target.display_name());
                generate_runtime(frontend_dir, detected_target)?;
                detected_target.update_frontend_manifest(frontend_dir)?;
                eprintln!(" done");
            }
        }

        // Step 3: Parse source files
        eprint!("  Scanning Rust source files...");
        let registry = if src_path.exists() {
            forge_codegen::parse_project(src_path)?
        } else {
            forge_core::schema::SchemaRegistry::new()
        };
        eprintln!(" done");

        // Check if we have any schema definitions (tables, enums, or functions)
        let has_schema = !registry.all_tables().is_empty()
            || !registry.all_enums().is_empty()
            || !registry.all_functions().is_empty();

        eprint!(
            "  Generating {} bindings...",
            detected_target.display_name()
        );
        detected_target.generate_bindings(&BindingGeneratorInput {
            output_dir: &output_dir,
            output_path,
            registry: &registry,
            has_schema,
            force: self.force,
        })?;
        eprintln!(" done");

        println!();
        if !self.skip_runtime {
            println!(
                "  {} Generated {} runtime (v{})",
                ui::ok(),
                detected_target.display_name(),
                FORGE_VERSION
            );
        }
        if has_schema {
            let table_count = registry.all_tables().len();
            let enum_count = registry.all_enums().len();
            let function_count = registry.all_functions().len();
            println!(
                "  {} Generated bindings from {} models, {} enums, {} functions",
                ui::ok(),
                style(table_count).cyan(),
                style(enum_count).cyan(),
                style(function_count).cyan()
            );
        }
        println!("  {} Output: {}", ui::info(), style(&output_dir).cyan());
        println!();

        Ok(())
    }
}
