# CLI and Generated Code

Forge CLI is the default workflow.

## Golden Rule

Generate first, edit second.

## Go-To Commands

```bash
forge new my-app --demo
forge add query list_orders
forge add mutation create_order
forge add job process_order
forge add workflow onboarding
forge add cron daily_digest
forge check
forge generate
```

## Required Behavior

- Prefer `forge add ...` when creating handlers.
- After changing schema/functions/macros, run `forge generate`.
- Use `forge check` before completion.
- For frontend changes, run CLI quality gates (`eslint`, `svelte-check`), preferring `bun` tooling when available.

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
- `src/lib/forge/*`
- `frontend/.forge/*`

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

- Mistake: create custom fetch wrappers that bypass generated client
  - Fix: use generated API/stores/runes wrappers unless special constraint exists

- Mistake: add handwritten types that drift from Rust source of truth
  - Fix: keep Rust as type source and regenerate frontend types
