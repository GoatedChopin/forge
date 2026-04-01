---
name: forge-idiomatic-engineer
description: "Forge-focused engineering workflow for Rust apps with generated frontend bindings. Activate when the repo contains `forge.toml`, Forge macros, or Forge CLI-driven generation."
---

# Forge Idiomatic Engineer

## What is Forge

Forge is a full-stack Rust framework. Everything compiles into a single binary backed by PostgreSQL. There is no separate API server, job runner, or cron scheduler -- one binary does it all.

The framework is macro-driven. You write annotated Rust functions and Forge generates the runtime wiring, frontend bindings, and type-safe clients.

**Core abstractions:**

| Concept | Purpose | Macro | Registration |
|---|---|---|---|
| Query | Read data | `#[forge::query]` | `.register_query::<FnNameQuery>()` |
| Mutation | Write data | `#[forge::mutation]` | `.register_mutation::<FnNameMutation>()` |
| Job | Background task | `#[forge::job]` | `.register_job::<FnNameJob>()` |
| Cron | Scheduled task | `#[forge::cron]` | `.register_cron::<FnNameCron>()` |
| Workflow | Durable multi-step | `#[forge::workflow]` | `.register_workflow::<FnNameWorkflow>()` |
| Daemon | Long-running process | `#[forge::daemon]` | `.register_daemon::<FnNameDaemon>()` |
| Webhook | External events | `#[forge::webhook]` | `.register_webhook::<FnNameWebhook>()` |
| MCP Tool | AI agent tool | `#[forge::mcp_tool]` | `.register_mcp_tool::<FnNameMcpTool>()` |

Naming rule: PascalCase of the function name + type suffix. Declare handlers as `pub async fn` so generated structs are accessible via `functions::StructName`. Avoid redundant suffixes (e.g., `heartbeat` not `heartbeat_daemon`).

Every handler must be registered in `src/main.rs`. Macros alone do not make handlers reachable.

## Workflow

### 1. Orient

Read `forge.toml` and `Cargo.toml` to understand the project shape. Check `src/main.rs` for registered handlers. For frontend work, check `frontend/package.json` (SvelteKit) or `frontend/Cargo.toml` (Dioxus).

### 2. Build backend first

Make the backend contract real, add tests, then run `forge generate` to produce frontend bindings. Wire the frontend against the generated output. Never hand-edit generated files (`frontend/src/lib/forge/*` or `frontend/src/forge/*`).

### 3. Verify

Run `forge check` as the final gate. For UI changes, run `forge test` for Playwright coverage.

### 4. Stop on blockers

If a port is occupied, DB is unreachable, or a tool is missing: report it and stop. Do not kill processes, change ports, or invent workarounds.

## When to load references

**Always load `references/pitfalls.md` before starting any implementation.** It contains critical mistakes that cause production bugs and common build errors with fixes.

| Working on | Load |
|---|---|
| Macro attributes, context methods, error types, config | `references/api.md` |
| Jobs, workflows, crons, daemons, auth, testing, deploys | `references/patterns.md` |
| Frontend patterns (shared principles) | `references/frontend.md` |
| SvelteKit: stores, runes, bindings | `references/frontend/svelte.md` |
| Dioxus: hooks, signals, bindings | `references/frontend/dioxus.md` |
| Common mistakes, build failures, runtime errors | `references/pitfalls.md` |

## Key principles

**Thin vertical slices.** Make the smallest change that solves the problem. Do not upgrade a bug fix into a redesign.

**Tests close to code.** Handlers get `#[cfg(test)] mod tests` in the same file. Test the weird cases by name. Bug fix means regression test. UI change means Playwright test (import `test` from `tests/fixtures.ts`).

**Boundary validation at handler entry.** Validate inputs at the handler level. Use `ForgeError` types, not panics. Never `unwrap()` or `expect()` in handler code -- use `?`.

**Never edit generated files.** If generated output is wrong, fix the Rust source or the codegen.

**Identity fields are validated automatically.** The router validates ownership keys (`user_id`, `owner_id`, `subject`, `principal_id`) and tenant keys (`tenant_id`) in input args against the JWT before the handler runs. Use `input.user_id` directly when present. Reserve `ctx.require_user_id()` for context-only handlers (no input struct). Do not manually compare identity fields.

**Transactional integrity.** Never call `dispatch_job` or `start_workflow` outside a `transactional` mutation. Without it, jobs execute against uncommitted data.

**Migrations use markers.** Write `-- @up` / `-- @down` in migration files. Do not edit `forge_migrations` directly. Enable reactivity with `SELECT forge_enable_reactivity('table_name')`, never hand-write PG triggers.

**No fake inputs.** If a handler has no business input, omit the parameter. No `Option<()>`, `()`, or dummy structs.

## Output contract

For implementation: what changed, tests added, commands run, blockers hit, `forge check` result.

For review: findings with file references, assumptions, short summary.
