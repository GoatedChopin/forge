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

Stop exploring once you know which files need changes and which command should verify them.

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

If the user asks for UI work but not for visual exploration, default to a simple UI: clear labels, obvious states, minimal motion, no invented brand system, no dashboard complexity unless the task needs it.

## Test style

Tests are part of the implementation. Keep them close to the code they prove:

- handlers: same file, `#[cfg(test)] mod tests` at the bottom
- pure helpers: same file, near the helper
- test-only helpers: inside that module's test block unless multiple modules truly need them

Good test code keeps special cases visible. Name and test the weird cases directly.

For Playwright tests, always import `test` from the generated `tests/fixtures.ts`.

Minimum bar: backend behavior change => add backend tests. Bug fix => regression test. UI change => Playwright coverage.

## Runtime blockers

Before runtime verification, check the configured port:

```bash
lsof -iTCP:<port> -sTCP:LISTEN -n -P
```

If the port is occupied: tell the user, stop runtime verification there. Do not kill the process, silently change ports, or continue against a guessed alternative. Same for missing DB access, missing package managers, missing Playwright browsers, or unresolved Forge CLI.

## Forge rules

### Use the real toolchain

Resolve and use the actual Forge CLI command, frontend package manager, test commands, and local database workflow. Do not invent subcommands on the app binary or guess generated paths.

### Backend before generated frontend

Finish the backend behavior and its tests first. Then run `forge generate`. Then wire the frontend against the generated contract. Never hand-edit generated files (`frontend/src/lib/forge/*` or `frontend/src/forge/*`).

### Handler registration matters

Macros alone do not make handlers reachable. Each must be registered in `src/main.rs`:

| Function | Generated struct | Register call |
|---|---|---|
| `list_todos` | `ListTodosQuery` | `.register_query::<f::ListTodosQuery>()` |
| `create_order` | `CreateOrderMutation` | `.register_mutation::<f::CreateOrderMutation>()` |
| `send_email` | `SendEmailJob` | `.register_job::<f::SendEmailJob>()` |
| `daily_cleanup` | `DailyCleanupCron` | `.register_cron::<f::DailyCleanupCron>()` |
| `onboarding` | `OnboardingWorkflow` | `.register_workflow::<f::OnboardingWorkflow>()` |
| `heartbeat` | `HeartbeatDaemon` | `.register_daemon::<f::HeartbeatDaemon>()` |
| `stripe` | `StripeWebhook` | `.register_webhook::<f::StripeWebhook>()` |
| `export_data` | `ExportDataMcpTool` | `.register_mcp_tool::<f::ExportDataMcpTool>()` |

Naming: PascalCase(fn_name) + type suffix. Avoid naming functions with the suffix (e.g. `heartbeat_daemon` → `HeartbeatDaemonDaemon`).

### Migrations

Use `-- @up` / `-- @down` markers. Do not edit `forge_migrations` directly. Access DB via `docker compose exec db psql`.

### Auth and ownership

Derive identity from context, never trust client-supplied ownership fields. Forge's scope enforcement validates keys (`user_id`, `owner_id`, `subject`, `tenant_id` and camelCase variants) at runtime. Do not manually compare.

### No fake inputs

If a handler has no business input, omit the parameter. No `Option<()>`, `()`, or dummy structs.

## Verification order

1. focused backend tests
2. `forge generate` if the contract changed
3. frontend checks if frontend changed
4. runtime verification if runtime behavior matters
5. `forge test` if UI changed
6. `forge check` last

## Reference loading

Most small tasks need only this file. Load one when the task needs it:

| Signal | Load |
|---|---|
| Macro attributes, context methods, error types, config fields | `references/api.md` |
| Auth, JWT, login/register, protected routes | `references/auth.md` |
| Frontend patterns (shared principles) | `references/frontend.md` |
| SvelteKit: stores, runes, generated bindings | `references/frontend/svelte.md` |
| Dioxus: hooks, signals, generated bindings | `references/frontend/dioxus.md` |
| Test contexts, assertions, mocking, DB testing | `references/testing.md` |
| Jobs, workflows, crons, daemons, webhooks | `references/patterns.md` |
| File uploads, MCP tools, external APIs, custom routes | `references/integrations.md` |
| Production: deploy, scaling, observability | `references/operations.md` |
| Review checklist, anti-patterns, security | `references/quality.md` |

## Output contract

For implementation tasks: what changed, tests added, what you ran, runtime success or blocker, Playwright result, `forge check` result, risks.

For review tasks: findings with file references, assumptions, short summary.
