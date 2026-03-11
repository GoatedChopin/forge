---
name: forge-idiomatic-engineer
description: "Forge-focused engineering workflow for Rust apps with generated frontend bindings. Use this for Forge/forgex projects, or whenever the repo contains `forge.toml`, Forge macros, generated Forge frontend bindings, or Forge CLI-driven backend/frontend generation. Stay grounded in repo reality, prefer small vertical slices, keep tests beside the code they prove, avoid speculative environment chases, and stop cleanly on blocked runtime prerequisites such as occupied ports."
---

# Forge Idiomatic Engineer

Work like a pragmatic maintainer: read the repo in front of you, make the smallest change that solves the user's stated problem, prove it with nearby tests, and stop cleanly when the environment blocks further progress.

Default assumptions unless the repo says otherwise: Rust 2024 workspace, `forgex` imported as `forge`, PostgreSQL, generated frontend bindings, and a frontend target of either SvelteKit or Dioxus.

## Operating style

- Prefer direct evidence from this repo over general Forge lore.
- Prefer one good local check over broad exploration.
- Prefer a thin vertical slice over a broad rewrite.
- Prefer explicit blockers over workaround cascades.
- Prefer plain, nearby tests over clever shared abstractions.

Do not quietly upgrade a small bug fix into a redesign, auth rebuild, or environment investigation.

## Start shallow

Before editing, read only what defines the task surface:

1. `forge.toml`
2. `Cargo.toml`
3. `src/main.rs`
4. the exact handler/schema/frontend files you expect to touch
5. `frontend/package.json` for SvelteKit work or `frontend/Cargo.toml` for Dioxus work
6. `migrations/` only if schema or DB work is in scope

Resolve the real Forge CLI entrypoint, then start working.

Stop exploring once you know:

- which files need changes
- which command should verify them

Avoid overanalysis. Do not:

- tour unrelated parts of the codebase
- generate a demo app just to infer patterns already visible here
- spelunk crate internals before checking local code and `forge --help`
- stack fallback after fallback once a blocker is already clear

## Build order

For most work:

1. make the backend contract real
2. add tests for the changed behavior
3. run `forge generate` if the contract changed
4. add the thinnest frontend needed to exercise the contract
5. verify the requested user path
6. run `forge check` last

Do not start with the polished version.

## UI default

If the user asks for UI work but not for visual exploration, default to a simple UI:

- clear labels
- obvious states
- minimal motion
- no invented brand system
- no dashboard complexity unless the task needs it

Do not go above and beyond on visuals without user buy-in.

## Test style

Tests are part of the implementation.

Keep them close to the code they prove:

- handlers: same file, `#[cfg(test)] mod tests` at the bottom
- pure helpers: same file, near the helper
- test-only helpers: inside that module's test block unless multiple modules truly need them

Good test code keeps special cases visible. Name and test the weird cases directly instead of hiding them behind clever fixtures or giant tables. The goal is not “clean-looking” tests. The goal is that a future change breaks loudly when behavior changes.

For Playwright tests, always import `test` from the generated `tests/fixtures.ts` instead of `@playwright/test`. The fixtures provide `rpc`, `gotoReady`, `uniqueId`, and `ACTION_TIMEOUT` so tests stay focused on behavior, not setup boilerplate.

Minimum bar:

- backend behavior change => add backend tests
- bug fix => add a regression test
- UI change => add or update Playwright coverage

Pure helper tests do not replace handler-level tests when handler behavior changed.

## Runtime blockers

Treat clear environment blockers as blockers.

Before runtime verification, check the configured port from `forge.toml` or env files:

```bash
lsof -iTCP:<port> -sTCP:LISTEN -n -P
```

If the port is occupied:

- tell the user which port is blocked and by what
- stop runtime verification there
- do not kill the other process
- do not silently move the app to another port
- do not continue to Playwright against a guessed alternative

Apply the same principle to missing database access, missing package managers, missing Playwright browsers, or an unresolved Forge CLI.

## Forge rules

### Use the real toolchain

Resolve and use the actual:

- Forge CLI command
- frontend package manager
- test commands
- local database workflow

Do not invent subcommands on the app binary. Do not guess generated paths.

### Backend before generated frontend

Finish the backend behavior and its tests first. Then run `forge generate`. Then wire the frontend against the generated contract.

Never hand-edit generated files:

- `frontend/src/lib/forge/*`
- `frontend/src/forge/*`

### Handler registration matters

After adding handlers, verify they are actually wired through `src/main.rs` and the local module exports. Do not assume macros alone made them reachable.

### Migrations must match the repo

Before editing migrations, inspect the local format and preserve required markers such as `-- @up` and `-- @down`.

Do not manually reconcile Forge migration state in the database.

### Auth and ownership

For user-owned data, default to secure behavior unless the user explicitly asked for no auth.

- derive acting identity from context
- never trust client-supplied ownership fields as authority
- validate ownership on the backend
- treat frontend payloads as hints, not truth

### No fake inputs

If a query or mutation has no real business input, omit the input parameter entirely. Do not use `Option<()>`, `()`, dummy structs, or underscore-prefixed unused inputs.

## Verification order

Run verification in this order when not blocked:

1. focused backend tests
2. `forge generate` if the contract changed
3. frontend checks if frontend changed
4. runtime verification if runtime behavior matters
5. `forge test` if UI changed (or `forge test --ui` to debug)
6. `forge check` last

Do not claim completion if tests were not run, Playwright failed, runtime boot is blocked, or `forge check` still fails.

## Reference loading

Most small tasks need only this file.

Load one supporting reference when the task needs it:

| Signal | Load |
|---|---|
| Auth, JWT, login/register, protected routes | `references/auth.md` |
| Frontend structure or SvelteKit patterns | `references/frontend.md` |
| Dioxus frontend structure, hooks, or binding usage | `references/dioxus.md` |
| Test design or Playwright expectations | `references/testing.md` |
| Forge config, CLI flow, migrations, generated paths | `references/config.md` |
| Jobs, workflows, crons, background work | `references/patterns.md` |
| Webhooks, daemons, external APIs, file uploads | `references/integrations.md` |
| Production hardening or observability | `references/operations.md` |
| Review or anti-pattern checks | `references/quality.md` |

Use live documentation only when the local repo does not answer the question or the API may have changed.

## Output contract

For implementation tasks, report:

1. what changed
2. which tests were added or updated
3. what you ran
4. whether runtime verification succeeded or what blocked it
5. Playwright result for UI work
6. `forge check` result
7. real risks or follow-ups

For review tasks, give findings first with file references, then assumptions or open questions, then a short summary.
