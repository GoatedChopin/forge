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

## Common Mistakes

- Mistake: patch generated TS client manually
  - Fix: update Rust function/schema signatures and rerun `forge generate`

- Mistake: update backend contracts but skip generation
  - Fix: run `forge generate` immediately after backend changes

- Mistake: create custom fetch wrappers that bypass generated client
  - Fix: use generated API/stores/runes wrappers unless special constraint exists

- Mistake: add handwritten types that drift from Rust source of truth
  - Fix: keep Rust as type source and regenerate frontend types
