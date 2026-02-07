use anyhow::Result;
use clap::Parser;
use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::time::Duration;

use super::runtime_generator::{
    FORGE_VERSION, generate_runtime, get_installed_version, has_legacy_runtime, needs_update,
    remove_legacy_runtime, update_frontend_package_json,
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
        let output_dir = self
            .output
            .unwrap_or_else(|| "frontend/src/lib/forge".to_string());
        let output_path = Path::new(&output_dir);

        let src_dir = self.src.unwrap_or_else(|| "src".to_string());
        let src_path = Path::new(&src_dir);

        // Detect frontend directory (parent of output_dir, or current dir)
        let frontend_dir = output_path
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
            .unwrap_or(Path::new("."));

        // Show progress
        let pb = ProgressBar::new(6);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} {msg}")
                .unwrap(),
        );
        pb.enable_steady_tick(Duration::from_millis(100));

        // Step 1: Check for legacy runtime and handle migration
        pb.set_message("Checking project structure...");
        if has_legacy_runtime(frontend_dir) {
            pb.finish_and_clear();

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
            generate_runtime(frontend_dir)?;
            println!("  {} Created .forge/svelte/ package", ui::ok());

            // Update package.json
            update_frontend_package_json(frontend_dir)?;
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

            // Continue with type generation
            pb.reset();
            pb.enable_steady_tick(Duration::from_millis(100));
        }

        // Step 2: Check runtime version and update if needed
        if !self.skip_runtime {
            pb.set_message("Checking @forge/svelte version...");

            let forge_dir_exists = frontend_dir.join(".forge/svelte").exists();

            if forge_dir_exists && needs_update(frontend_dir) {
                let installed =
                    get_installed_version(frontend_dir).unwrap_or_else(|| "unknown".to_string());

                pb.finish_and_clear();

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
                            "{} Update declined. Use --skip-runtime to only regenerate types.",
                            ui::info()
                        );
                        return Ok(());
                    }
                }

                pb.reset();
                pb.enable_steady_tick(Duration::from_millis(100));
                pb.set_message("Updating @forge/svelte runtime...");
                generate_runtime(frontend_dir)?;

                println!();
                println!(
                    "  {} Updated @forge/svelte runtime (v{} → v{})",
                    ui::ok(),
                    installed,
                    FORGE_VERSION
                );
            } else if !forge_dir_exists {
                // First time generation
                pb.set_message("Generating @forge/svelte runtime...");
                generate_runtime(frontend_dir)?;
                update_frontend_package_json(frontend_dir)?;
            }
            pb.inc(1);
        } else {
            pb.set_message("Skipping runtime generation...");
            pb.inc(1);
        }

        // Step 3: Parse source files
        pb.set_message("Scanning Rust source files...");
        let registry = if src_path.exists() {
            forge_codegen::parse_project(src_path)?
        } else {
            pb.set_message("No src directory found, using defaults...");
            forge_core::schema::SchemaRegistry::new()
        };
        pb.inc(1);

        // Check if we have any schema definitions (tables, enums, or functions)
        let has_schema = !registry.all_tables().is_empty()
            || !registry.all_enums().is_empty()
            || !registry.all_functions().is_empty();

        if has_schema {
            // Use forge_codegen to generate TypeScript
            pb.set_message("Generating TypeScript from schema...");

            // Check if auth is configured in forge.toml
            let generate_auth = forge_core::config::ForgeConfig::from_file("forge.toml")
                .map(|c| c.auth.jwt_secret.is_some() || c.auth.jwks_url.is_some())
                .unwrap_or(false);

            let options = forge_codegen::GenerateOptions {
                generate_auth_store: generate_auth,
            };
            let generator = forge_codegen::TypeScriptGenerator::with_options(&output_dir, options);
            generator.generate(&registry)?;
            pb.inc(4);
        } else {
            // Fall back to default templates if no schema found
            pb.set_message("No schema found, generating defaults...");

            // Create output directory if it doesn't exist
            if !output_path.exists() {
                fs::create_dir_all(output_path)?;
            }

            // Generate default files
            pb.set_message("Generating types...");
            generate_types(output_path, self.force)?;
            pb.inc(1);

            pb.set_message("Generating API bindings...");
            generate_api(output_path, self.force)?;
            pb.inc(1);

            pb.set_message("Generating stores...");
            generate_stores(output_path, self.force)?;
            pb.inc(1);

            pb.set_message("Generating runes...");
            generate_runes(output_path)?;

            pb.set_message("Generating index...");
            generate_index(output_path)?;
            pb.inc(1);
        }

        pb.finish_with_message("Done!");

        println!();
        if !self.skip_runtime {
            println!(
                "  {} Generated @forge/svelte runtime (v{})",
                ui::ok(),
                FORGE_VERSION
            );
        }
        if has_schema {
            let table_count = registry.all_tables().len();
            let enum_count = registry.all_enums().len();
            let function_count = registry.all_functions().len();
            println!(
                "  {} Generated TypeScript from {} models, {} enums, {} functions",
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

/// Generate types.ts from schema.
fn generate_types(output_dir: &Path, force: bool) -> Result<()> {
    let file_path = output_dir.join("types.ts");
    if file_path.exists() && !force {
        return Ok(());
    }

    let content = r#"// Auto-generated by FORGE - DO NOT EDIT
// Run `forge generate` after adding or modifying models

// Model types will be generated here based on your Rust schema
// Example: export interface User { id: string; name: string; }

// Common types (re-exported from @forge/svelte for convenience)
export type { ForgeError, QueryResult, SubscriptionResult } from "@forge/svelte";
"#;

    fs::write(file_path, content)?;
    Ok(())
}

/// Generate api.ts with function bindings.
fn generate_api(output_dir: &Path, force: bool) -> Result<()> {
    let file_path = output_dir.join("api.ts");
    if file_path.exists() && !force {
        return Ok(());
    }

    let content = r#"// Auto-generated by FORGE - DO NOT EDIT
// Run `forge generate` after adding functions to src/functions/

// Function bindings will be generated here based on your Rust functions
// Example:
// import { createQuery, createMutation } from "@forge/svelte";
// import type { User } from "./types";
// export const getUsers = createQuery<null, User[]>("get_users");
// export const createUser = createMutation<{ name: string }, User>("create_user");

export {};
"#;

    fs::write(file_path, content)?;
    Ok(())
}

/// Generate stores.ts for Svelte integration.
fn generate_stores(output_dir: &Path, force: bool) -> Result<()> {
    let file_path = output_dir.join("stores.ts");
    if file_path.exists() && !force {
        return Ok(());
    }

    let content = r#"// Auto-generated by FORGE - DO NOT EDIT
export {
  getForgeClient,
  createConnectionStore,
  createQueryStore,
  createSubscriptionStore,
  createJobStore,
  createWorkflowStore,
  dt,
} from '@forge/svelte';
export type {
  Readable,
  ConnectionStatusStore,
  QueryStore,
  SubscriptionStore,
  JobStore,
  WorkflowStore,
} from '@forge/svelte';
"#;

    fs::write(file_path, content)?;
    Ok(())
}

/// Generate runes.svelte.ts for Svelte 5 runes support.
fn generate_runes(output_dir: &Path) -> Result<()> {
    let file_path = output_dir.join("runes.svelte.ts");

    let content = r#"// Auto-generated by FORGE - DO NOT EDIT
import type { SubscriptionStore } from "@forge/svelte";
import type { SubscriptionResult } from "@forge/svelte";

export interface ReactiveQuery<T> extends SubscriptionResult<T> {
  unsubscribe: () => void;
}

export function toReactive<T>(store: SubscriptionStore<T>): ReactiveQuery<T> {
  const state: ReactiveQuery<T> = $state({
    loading: true,
    data: null,
    error: null,
    stale: false,
    unsubscribe: () => {
      unsubscribeCallback();
      store.unsubscribe();
    },
  });

  const unsubscribeCallback = store.subscribe((s) => {
    state.loading = s.loading;
    state.data = s.data;
    state.error = s.error;
    state.stale = s.stale;
  });

  return state;
}
"#;

    fs::write(file_path, content)?;
    Ok(())
}

/// Generate index.ts.
fn generate_index(output_dir: &Path) -> Result<()> {
    let file_path = output_dir.join("index.ts");

    let content = r#"// Auto-generated by FORGE - DO NOT EDIT

// Types
export * from './types';

// API bindings
export * from './api';

// Stores (re-exported from @forge/svelte)
export * from './stores';

// Runes (Svelte 5 reactive helpers)
export * from './runes.svelte';

// Client and Provider (re-exported from @forge/svelte)
export { ForgeClient, ForgeClientError, createForgeClient, ForgeProvider } from '@forge/svelte';
"#;

    fs::write(file_path, content)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_generate_types() {
        let dir = tempdir().unwrap();
        generate_types(dir.path(), false).unwrap();
        assert!(dir.path().join("types.ts").exists());
    }

    #[test]
    fn test_generate_api() {
        let dir = tempdir().unwrap();
        generate_api(dir.path(), false).unwrap();
        assert!(dir.path().join("api.ts").exists());
    }

    #[test]
    fn test_generate_stores() {
        let dir = tempdir().unwrap();
        generate_stores(dir.path(), false).unwrap();
        assert!(dir.path().join("stores.ts").exists());
    }
}
