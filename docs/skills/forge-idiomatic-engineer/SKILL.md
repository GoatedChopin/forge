---
name: forge-idiomatic-engineer
description: "Forge-focused engineering workflow for Rust + Svelte apps. Use for Forge/forgex projects, or whenever the codebase includes `forge.toml`, Forge macros, generated Forge frontend bindings, or Forge CLI-driven backend/frontend generation. Optimize for correct project discovery, authenticated scope handling, safe code generation, migrations, testing, and a clean final `forge check`."
---

# Forge Idiomatic Engineer

Produce code that is easy to test, secure by default, observable in production, simple to extend, and idiomatic to Forge + Rust + Svelte 5.

Default assumptions unless the project clearly says otherwise: Rust 2024 workspace, `forgex` imported as `forge`, PostgreSQL, macro-first function model, generated frontend bindings, strict linting, no `unwrap`/`expect` in runtime paths.

## First Move: Establish Project Reality

Before editing anything:

1. Find the app root (`forge.toml`) and run Forge commands from there.
2. Inspect `forge.toml`, `Cargo.toml`, `src/main.rs`, `frontend/package.json` (or lockfile), and `migrations/`.
3. Resolve the actual Forge command available in this environment.
4. Confirm the generated frontend paths and existing tooling before you run generation or checks.

Never assume:
- `forge` is on `PATH`
- the app binary itself supports Forge CLI subcommands
- `bun`, `psql`, or Docker are installed
- generated files live in a guessed directory

Prefer project discovery over speculation. If the environment disagrees with the skill, trust the environment.

## Non-Negotiable Rules

### 1) Discover the Actual Toolchain Before Running Commands

Resolve, then use:
- Forge CLI command (`forge`, repo-local binary, wrapper script, or other documented entrypoint)
- frontend package manager (`bun`, `pnpm`, `npm`, etc.)
- local database workflow

Do not guess command names. Do not invent fallback subcommands on the app binary.
If `forge` is not on `PATH`, search project-local binaries, checked-in toolchains, or build outputs before treating generation as unavailable.

### 2) Playwright Tests for UI Changes

Every UI change requires Playwright integration coverage. Write or update the tests and run them. The task is not complete until they pass.

For auth work specifically: cover register, login, authenticated state, and logout.

Do not send a completion summary for a UI task unless you can state the Playwright result explicitly. If Playwright is blocked, report the task as blocked, not complete.

### 3) `forge check` Is the Final Gate

`forge check` is the absolute last step before delivery. Run it from the app root. Fix every error and actionable warning. Rerun until clean.

### 4) All Tests Must Pass

Backend and frontend test failures are blockers. Diagnose the root cause, fix it, rerun, and only then deliver.

Adding zero tests after changing backend behavior or UI is also a failure. `forge check`, lint, `svelte-check`, and manual clicking do not count as test coverage.

Minimum expectation:
- backend behavior change => add backend tests
- UI change => add or update Playwright coverage
- bug fix => add a regression test

Pure helper tests alone are not enough when handlers changed. Add handler-level or integration coverage for the changed backend behavior.

### 5) Backend Contracts Before Frontend Integration

Complete backend behavior and tests first. Run generation. Then wire the frontend against the generated contract. Do not start speculative frontend work against unstable backend signatures.

### 6) Run `forge generate` After Backend Contract Changes

After schema/function/macro changes, run `forge generate` before touching dependent frontend code.

Never hand-edit generated files:
- `frontend/src/lib/forge/*`
- `frontend/.forge/*`

Never hand-create replacement files inside generated directories either. Placeholder or missing generated files are not permission to author them manually.

If generation is required and the Forge CLI command is still unresolved, stop and resolve the tooling problem first. Do not defer `forge generate` to the user and do not fake generated bindings.

If generation writes to an unexpected nested location, stop and fix the cwd/config issue instead of patching the duplicate output.

### 7) Migration Discipline

Before adding or editing a migration:
- inspect the existing scaffold/template in `migrations/`
- preserve the required format used by this project
- delete placeholder example migrations when replacing them

Do not:
- manually insert rows into `forge_migrations`
- manually apply SQL first and try to reconcile the state later
- use `CREATE TABLE IF NOT EXISTS` to mask scaffold conflicts
- pipe Forge-formatted migration files straight into `psql` unless you are certain the format and helper functions are compatible with that path

If the project expects markers such as `-- @up` / `-- @down`, keep them.

### 8) Register New Handlers in `src/main.rs`

Forge macros do not make handlers reachable by themselves. After adding queries, mutations, jobs, or workflows:
- update `src/main.rs`
- register each new handler on the builder
- verify the route is actually callable before claiming success

Do not assume `mod.rs` exports are enough.

### 9) Auth by Default for User-Owned Apps

For greenfield apps with user-owned data, default to auth unless the user explicitly wants a single-user or no-auth build.

Silence on auth is not permission to ship public CRUD for user-owned records.

When auth is in scope:
- configure `[auth]` in `forge.toml`
- create public `register` and `login` mutations
- verify end-to-end against the running API
- build the minimal login/register frontend needed to exercise the app

### 10) Identity Scope Rule for Authenticated Inputs

If an authenticated query or mutation accepts any input struct, that Rust input type must include an identity or tenant scope field (`user_id`, `userId`, `owner_id`, `subject`, `principal_id`, `tenant_id`, or camelCase variants).

Only use the no-input form when the handler truly has no business inputs.

Important:
- the router validates the scope field against the JWT before the handler runs
- handler code should still use `ctx.require_user_id()?` / `ctx.require_subject()?`
- generated TypeScript bindings may omit the injected scope field from client-facing call signatures; trust the generated client contract instead of forcing extra frontend args

### 11) Never Trust Frontend Data

Frontend payloads are convenience, not authority.

Always validate on the backend:
- required fields, lengths, formats, enums, bounds, and state transitions
- ownership and authorization for every user-visible record
- derived or sensitive values such as `user_id`, roles, tenant, and timestamps

Rules:
- never trust a client-supplied `user_id` for SQL scoping; derive the acting principal from `ctx.require_user_id()?`
- never trust frontend booleans, prices, statuses, or permissions without backend validation
- use frontend-provided ids only as lookup candidates, then verify ownership in SQL or domain logic
- treat router scope validation as one layer, not the whole security model

### 12) No Dummy or Underscore Input Parameters

When a query or mutation takes no input, omit the args parameter entirely. Never use `Option<()>`, `()`, or dummy structs.

If a parameter is unused, remove it instead of prefixing it with `_`. Generated TypeScript names preserve the Rust parameter name verbatim.

```rust
// correct
#[forge::query(public)]
pub async fn list_todos(ctx: &QueryContext) -> Result<Vec<Todo>> { ... }

// wrong
#[forge::query(public)]
pub async fn list_todos(ctx: &QueryContext, _input: Option<()>) -> Result<Vec<Todo>> { ... }
```

## Quick Reference

### Primitive selection

| Need | Use |
|---|---|
| Read data | `#[forge::query]` |
| Write data | `#[forge::mutation]` |
| Async work | `#[forge::job]` |
| Recurring schedule | `#[forge::cron]` |
| Multi-step durable process | `#[forge::workflow]` |
| External callback | `#[forge::webhook]` |
| AI-callable tool | `#[forge::mcp_tool]` |
| Long-running background process | `#[forge::daemon]` |

Avoid:
- workflow for a single async task: use mutation + job
- heavy sync work in a webhook: dispatch a job, return quickly
- writes in a query: use mutation
- cron for a one-off delayed sequence: use workflow with sleep

### Basic query

```rust
#[forge::query(tables = ["orders"])]
pub async fn list_orders(ctx: &QueryContext) -> Result<Vec<Order>> {
    let uid = ctx.require_user_id()?;

    sqlx::query_as("SELECT id, status, total FROM orders WHERE user_id = $1 ORDER BY created_at DESC")
        .bind(uid)
        .fetch_all(ctx.db())
        .await
        .map_err(Into::into)
}
```

### Basic mutation

```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateOrderInput {
    pub user_id: Uuid,
    pub total_cents: i64,
}

#[forge::mutation(transactional)]
pub async fn create_order(ctx: &MutationContext, input: CreateOrderInput) -> Result<Order> {
    let uid = ctx.require_user_id()?;
    debug_assert_eq!(input.user_id, uid);

    let order: Order = ctx.db()
        .fetch_one(
            sqlx::query_as("INSERT INTO orders (user_id, total) VALUES ($1, $2) RETURNING *")
                .bind(uid)
                .bind(input.total_cents),
        )
        .await?;

    ctx.dispatch_job("send_receipt", serde_json::json!({ "order_id": order.id })).await?;
    Ok(order)
}
```

Use `transactional` when multiple related writes, job/workflow dispatch, or all-or-nothing consistency are involved.

### DbConn calling conventions

- `QueryContext::db()` behaves like `&PgPool`; chain sqlx fetches directly on it.
- `MutationContext::db()` returns `DbConn<'_>`; prefer its wrapper methods in mutations.
- `MutationContext::pool()` is the direct pool escape hatch when needed.
- For shared helpers: `pub(crate) async fn helper(db: DbConn<'_>, ...) -> Result<T>`.

### Scope enforcement

- Authenticated handlers default to protected unless `public` is explicitly justified.
- Authenticated handlers with input parameters must include an identity or tenant key.
- No-input authenticated handlers may omit the input and rely on `ctx.require_user_id()?`.
- Do not add redundant manual `if input.user_id != ctx.require_user_id()?` checks in production logic.
- Never trust frontend payload fields as authority for ownership, permissions, prices, or other security-sensitive values.
- Input/output structs derive both `Serialize` and `Deserialize`.

### Error mapping

| Condition | Variant | HTTP |
|---|---|---|
| Invalid input | `ForgeError::Validation` | 400 |
| No/bad auth | `ForgeError::Unauthorized` | 401 |
| Wrong scope/role | `ForgeError::Forbidden` | 403 |
| Missing entity | `ForgeError::NotFound` | 404 |
| Rate limited | `ForgeError::RateLimitExceeded` | 429 |
| Server fault (last resort) | `ForgeError::Internal` | 500 |

Do not collapse domain-specific failures into `Internal`.

### Project structure

- `src/functions/` for Forge handlers
- `src/schema/` for all input/output structs and domain models
- `src/utils/` for pure helpers
- tests inline with `#[cfg(test)] mod tests` at the bottom of the relevant file

## Execution Order

1. Discover the app root, CLI entrypoints, package manager, migration format, and generated paths.
2. Implement backend behavior and tests in `src/functions/`, `src/schema/`, and `src/utils/`.
3. Run backend tests before generation. Do not delete scaffolded test examples without replacing them with real tests.
4. Run `forge generate` after backend contracts are correct.
5. Implement the thinnest usable frontend slice against the generated client.
6. Run frontend quality gates (`lint`, `svelte-check`, types, formatting) using the project's actual package manager.
7. Prove the app boots through the project's real dev flow (`forge dev` or the discovered equivalent) when the task is supposed to work out of the box.
8. Run Playwright if UI changed.
9. Run `forge check` from the app root as the final clean gate.

For greenfield builds, deliver a narrow vertical slice first, then polish. Do not jump straight into a large one-file UI.
Do not present “everything passes”, “clean”, or “ready” language before steps 7-9 are actually complete for UI work.

## Reference Loading

Most tasks (add a query, fix a mutation, small refactor) need nothing beyond this file.

### No reference needed

- adding or modifying a simple query or mutation
- bug fixes in existing handlers
- small refactors inside established patterns

### Load one reference when triggered

| Signal in the task | Load |
|---|---|
| Auth setup, login/register, JWT, protected routes, auth store | `references/auth.md` |
| Jobs, crons, workflows, retries, scheduling, background processing | `references/patterns.md` |
| Webhooks, custom HTTP routes, daemons, MCP tools, file uploads, OAuth, external APIs | `references/integrations.md` |
| Frontend, Svelte 5, components, pages, UI, reactivity, design | `references/frontend.md` |
| Test strategy, Playwright setup, coverage approach | `references/testing.md` |
| Code review, audit, anti-pattern check, quality gate | `references/quality.md` |
| `forge.toml`, context API, CLI workflow, migrations, generated paths | `references/config.md` |
| Read replicas, observability, tracing, production hardening | `references/operations.md` |

### Load two for compound tasks

- full-stack feature: `references/frontend.md` + `references/testing.md`
- auth + frontend: `references/auth.md` + `references/testing.md`
- production review: `references/quality.md` + `references/operations.md`

### API uncertainty

Use live documentation lookup when Forge, Svelte 5, or third-party APIs may have changed.

## Output Contract

### Implementation output

1. Contract summary
2. Files changed and rationale
3. Backend tests run or added
4. App boot verification result
5. Playwright result for UI tasks
6. Security, scale, and observability checks
7. `forge check` result
8. Risks or follow-ups

### Review output

- findings first, ordered by severity with file/line references
- then assumptions or open questions
- then a short summary
