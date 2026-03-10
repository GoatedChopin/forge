# Frontend Framework Architecture

This codebase now treats a frontend target as a small framework spec instead of a set of unrelated match statements.

## Current layout

- `crates/forge/src/cli/frontend_target.rs`
  - Central registry for supported frontend frameworks.
  - Owns detection, display metadata, default output paths, runtime directory names, manifest patching, formatting hooks, scaffold factories, runtime template lists, and binding generation entry points.
- `crates/forge/src/cli/frontend_scaffold/`
  - One module per framework (`sveltekit.rs`, `dioxus.rs`).
  - Keeps project and frontend template lists local to that framework.
  - Shared frontend test templates live in `mod.rs`.
- `crates/forge/src/cli/frontend_runtime.rs`
  - Runtime package template inventories and manifest patch helpers.
- `crates/forge/src/cli/frontend_codegen.rs`
  - Binding generator functions per framework behind a common input shape.
- `crates/forge/templates/`
  - `scaffold/demo/project/common` and `scaffold/minimal/project/common` hold backend scaffolding shared across frameworks.
  - `scaffold/<mode>/project/<framework>` holds framework-specific backend entry/build/deploy templates.
  - `scaffold/<mode>/frontend/<framework>` holds framework-specific frontend scaffolding.
  - `scaffold/shared/frontend/tests` holds truly shared frontend test support files.
  - `runtime/<framework>` holds local runtime package templates.

## Why this matters

Adding a new framework should no longer require editing template data in one file, runtime metadata in another, and CLI branching in several more places.

The intended flow is:

1. Add a scaffold module under `crates/forge/src/cli/frontend_scaffold/`.
2. Add runtime templates and a manifest updater if the framework needs a local runtime package.
3. Add a binding generator function in `frontend_codegen.rs`.
4. Register a single `FrontendTargetSpec` in `frontend_target.rs`.

Most CLI code (`forge new`, `forge generate`, runtime generation) should continue working through the spec without any extra branching.

## Rules for new frameworks

- Keep framework-specific template lists in their own module. Do not append more giant arrays to shared files.
- Put shared templates in shared modules only when they are truly framework-agnostic.
- If a framework needs custom formatting, manifest patching, or post-generation steps, hang that behavior off the framework spec.
- Keep generated runtime files under `frontend/.forge/<framework>/`.
- Keep generated user-facing bindings in a framework-specific default output directory declared in the framework spec.

## Remaining work

- `crates/forge/src/cli/check.rs` still contains some framework-specific validation branches.
- If more frameworks are added, move those checks behind the same spec pattern so quality gates scale with the rest of the pipeline.
