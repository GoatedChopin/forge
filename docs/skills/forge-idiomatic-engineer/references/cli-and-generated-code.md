# CLI and Generated Code

Forge CLI generation is the required workflow after backend contract changes.

## Golden Rule

Generate first, edit second.

Backend source (`src/schema`, `src/functions`) is the source of truth. `forge generate` is the sync step that projects backend contracts into generated frontend/runtime artifacts.

## Go-To Commands

```bash
forge new my-app --demo
forge migrate status
forge generate
forge check
```

## Required Behavior

- Use `forge new` for new projects. Do not hand-scaffold the initial layout.
- Author backend handlers/contracts directly in `src/functions/*` and `src/schema/*`.
- After changing schema/functions/macros, run `forge generate`.
- When migrations change, use `forge migrate status` and `forge migrate up` as appropriate for verification.
- Run `forge check` from app root (`forge.toml` directory) before completion.
- Fix all check findings, then rerun `forge check` until clean.
- For frontend changes, run CLI quality gates (`eslint`, `svelte-check`), preferring `bun` tooling when available.

## Backend Generation Policy

- `forge add` is intentionally not part of this workflow.
- Backend primitives (`query`, `mutation`, `job`, `cron`, `workflow`, `webhook`, `mcp_tool`, `daemon`) are authored directly in Rust source.
- `forge generate` is the required generator step that keeps client/runtime artifacts aligned with backend code.

### Frontend Quality Commands

Prefer:

```bash
bun install
bun run lint
bunx svelte-check
```

Fallback: use equivalent commands for the project's package manager.

## Never Edit Generated Files

Treat these as generated artifacts:
- `frontend/src/lib/forge/*`
- `frontend/.forge/svelte/*`
- `frontend/.forge/version`
- legacy `src/lib/forge/runtime/*` if you encounter an older project during migration

When change is needed, modify Rust source or Forge config, then regenerate.

## Safe Edit Zones

- `src/functions/*`
- `src/schema/*`
- `src/utils/*`
- `frontend/src/routes/*`
- `frontend/src/lib/*` excluding generated Forge paths

## Migration Cleanup

When creating real migration files for a project, check `migrations/` for any scaffolded files from `forge new`:
- `--minimal` creates `0001_initial.sql.example` (a commented placeholder). Delete it before creating your real migration.
- `--demo` creates `0001_initial.sql` (a real migration with tables). This will run and create tables that conflict with your own migration. Delete it and, if the database already ran it, drop those tables and remove the row from `forge_migrations` before running your own migration.

Do not use `CREATE TABLE IF NOT EXISTS` in migrations. It silently skips creation if a conflicting table from the scaffold migration already exists, leading to schema mismatches that are hard to debug.

## Project Layout Standard

Prefer this structure for app code:
- `src/functions/` for Forge handlers
- `src/schema/` for domain structs, enums, and data contracts
- `src/utils/` for pure helper logic

Type placement: all input/output structs, domain models, and enums belong in `src/schema/`, not inline in function files. Handlers import from `schema`.

Function locality: inside `src/functions/`, keep validation, orchestration, and function-specific helper logic nearby. Only move logic to `src/utils/` when reuse is real.

See `references/project-structure.md` for full layout details.

## Common Mistakes

- Mistake: patch generated TS client manually
  - Fix: update Rust function/schema signatures and rerun `forge generate`

- Mistake: update backend contracts but skip generation
  - Fix: run `forge generate` immediately after backend changes

- Mistake: create custom fetch wrappers that bypass generated client
  - Fix: use generated API/stores/runes wrappers unless special constraint exists

- Mistake: add handwritten types that drift from Rust source of truth
  - Fix: keep Rust as type source and regenerate frontend types

## Common `forge check` Issues

### Prettier finding test artifacts

Playwright generates HTML reports and test result files that Prettier will try to format, causing `forge check` to fail. Add these to `.prettierignore`:

```
test-results/
playwright-report/
```

### `#[forge::model]` info warning

When using standard derives on model structs, `forge check` may show an informational message about `#[forge::model]`. This is not an error. The macro provides additional functionality, but standard derives work fine if you don't need it.

### `[database].mode not set`

This warning is fine for local development. Forge defaults to a sensible mode when not explicitly configured. Set it in `forge.toml` when deploying to production.

### Clippy flakiness on first run

If clippy produces inconsistent results on the first run, it's typically a stale incremental cache issue. Forge sets `CARGO_INCREMENTAL=0` internally to avoid this. If you still see it, run `cargo clean` and retry.
