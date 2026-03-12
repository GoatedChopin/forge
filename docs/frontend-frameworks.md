# Frontend Framework Architecture

Forge now treats a frontend target as a small framework spec instead of a set of unrelated match statements. Template selection is separate: `forge new --template ...` copies from checked-in runnable examples, while the frontend target owns detection, formatting, post-generation hooks, and binding generation.

## Current support

| Target | Runtime package | Generated bindings | Default output dir |
|---|---|---|---|
| `sveltekit` | `@forge-rs/svelte` | TypeScript client, stores, and Svelte helpers | `frontend/src/lib/forge` |
| `dioxus` | `forge-dioxus` | Rust types, RPC functions, and Dioxus hooks | `frontend/src/forge` |

The template catalog currently groups projects under `examples/with-svelte/*` and `examples/with-dioxus/*`, but the target system is designed so additional frameworks can be registered through the same spec model.

## Current layout

- `crates/forge/src/cli/frontend_target.rs`
  - Central registry for supported frontend frameworks.
  - Owns detection, display metadata, default output paths, runtime directory names, manifest patching, formatting hooks, and binding generation entry points.
- `crates/forge/src/cli/template_catalog.rs`
  - Loads embedded template metadata derived from `examples/with-*/*`.
  - Validates supported public template ids and drives copy/rewrite behavior for `forge new`.
- `crates/forge/src/cli/frontend_runtime.rs`
  - Runtime package template inventories and manifest patch helpers.
- `crates/forge/src/cli/frontend_codegen.rs`
  - Binding generator functions per framework behind a common input shape.
- `examples/with-*/*`
  - Runnable checked-in example apps and the authored source of truth for scaffolding.
  - Each template root carries a `.forge-template.toml` file describing rewrite targets and copy exclusions.
- `crates/forge/generated/examples.tar`
  - Generated publish-time archive derived from `examples/with-*/*`.
  - Used only when the crate is built outside the workspace and the live `examples/` tree is unavailable.

## Why this matters

Adding a new framework should no longer require editing template data in one file, runtime metadata in another, and CLI branching in several more places.

The intended flow is:

1. Add example-backed templates under `examples/with-<framework>/...` plus `.forge-template.toml` metadata.
2. Refresh the generated publish archive with `scripts/build-template-archive.sh`.
3. Add runtime templates and a manifest updater if the framework needs a local runtime package.
4. Add a binding generator function in `frontend_codegen.rs`.
5. Register a single `FrontendTargetSpec` in `frontend_target.rs`.

Most CLI code (`forge new`, `forge generate`, runtime generation) should continue working through the spec without any extra branching.

The backend contract does not change per framework. Rust handlers, schema models, migrations, and `sqlx` compile-time validation remain the source of truth; each frontend target just projects that contract into a different binding format.

## Rules for new frameworks

- Keep template authoring in runnable example directories, not hidden scaffold arrays.
- Put shared behavior in metadata or reusable runtime/codegen helpers only when it is truly framework-agnostic.
- If a framework needs custom formatting, manifest patching, or post-generation steps, hang that behavior off the framework spec.
- Publish runtime packages to registries (npm for JS frameworks, crates.io for Rust frameworks).
- Keep generated user-facing bindings in a framework-specific default output directory declared in the framework spec.

## Remaining work

- `crates/forge/src/cli/check.rs` still contains some framework-specific validation branches.
- If more frameworks are added, move those checks behind the same spec pattern so quality gates scale with the rest of the pipeline.
