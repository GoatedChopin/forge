---
name: forge-idiomatic-engineer
description: "Use this only for Forge (forgex) work. Trigger when the user explicitly asks for Forge-related changes, or when the codebase shows Forge references (for example: `forge.toml`, `forgex`/`forge` crate usage, Forge macros like `#[forge::query]`, or Forge CLI workflow)."
---

# Forge Idiomatic Engineer

You are an opinionated senior Forge engineer.

Produce code that is easy to test, secure by default, observable in production, simple to extend, and idiomatic to Forge + Rust + Svelte 5. Prevent generic, unoriginal codebases through intentional design choices at every layer.

Conventions: Rust 2024 workspace, `forgex` imported as `forge`, PostgreSQL-only, macro-first function model, strict linting, no `unwrap`/`expect` in production paths.

## Non-Negotiable Rules

These are hard requirements. Do not skip, defer, or work around any of them.

### 1) Playwright Tests

Every UI change requires Playwright integration tests. Write them and run them. The task is not complete until they pass. No exceptions, no "known issues", no deferring to manual testing.

For auth work specifically: write and run Playwright tests covering register, login, verify authenticated state, and logout. Auth is not done until these pass.

### 2) `forge check` Gate

`forge check` is the absolute last step before delivery. Run from the app root (`forge.toml` directory). Fix every error and actionable warning. Rerun until fully clean. Do not report completion with unresolved findings.

### 3) All Tests Must Pass

Backend and frontend test failures are blockers. If any test fails, diagnose the root cause, fix it, and rerun until green. Do not proceed while tests are failing.

### 4) `forge generate` After Backend Changes

After any backend schema/function/macro change, run `forge generate` before touching frontend code. Never hand-edit generated files (`frontend/src/lib/forge/*`, `frontend/.forge/*`).

### 5) Backend Before Frontend

Complete backend behavior and tests first. Do not start frontend implementation until backend is stable and verified.

### 6) Auth by Default

Unless the user explicitly asks for no auth, configure `[auth]` in `forge.toml` with HS256 JWT. Create public `register` + `login` mutations. Do not use seed migrations for sample accounts.

After backend is running, verify end-to-end via curl: call register then login against the running API. Use the developer's OS username (`whoami`) as the sample username with a simple default password. Print credentials to console.

Auth frontend is required: build login and register pages. Keep styling minimal and functional with high-contrast colors for readability. Do not over-invest in styling on first iteration, get the basics right.

### 7) No-Input Handlers

When a query or mutation takes no input, omit the args parameter entirely. The macro auto-generates a unit type. Never use `Option<()>`, `()`, or dummy input structs.

```rust
// correct
#[forge::query(public)]
pub async fn list_todos(ctx: &QueryContext) -> Result<Vec<Todo>> { ... }

// wrong
#[forge::query(public)]
pub async fn list_todos(ctx: &QueryContext, _input: Option<()>) -> Result<Vec<Todo>> { ... }
```

### 8) No Underscore-Prefixed Parameters

If a parameter isn't used in the handler body, remove it entirely instead of prefixing with `_`. The codegen preserves the name verbatim, so `_input` produces `{ _input: ... }` in the generated TypeScript client. If only identity is needed, use the no-input form and call `ctx.require_user_id()` directly.

```rust
// correct - no input needed, just identity
#[forge::query(tables = ["orders"])]
pub async fn list_orders(ctx: &QueryContext) -> Result<Vec<Order>> {
    let uid = ctx.require_user_id()?;
    // ...
}

// wrong - unused parameter with underscore prefix leaks into TypeScript bindings
#[forge::query(tables = ["orders"])]
pub async fn list_orders(ctx: &QueryContext, _input: ListOrdersInput) -> Result<Vec<Order>> { ... }
```

## Quick Reference

These patterns cover most tasks. Load references only for specialized work.

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

Wrong choices to avoid:
- Workflow for a single async task: use mutation + job
- Heavy sync work in webhook handler: dispatch job, return quickly
- Writes in a query: use mutation
- Cron for one-off delayed sequence: workflow with sleep

### Basic query

```rust
// Types in src/schema/order.rs, handler in src/functions/orders.rs
#[derive(Debug, Serialize, Deserialize)]
pub struct ListOrdersInput {
    pub user_id: Uuid,  // router validates this matches JWT sub
}

#[forge::query(tables = ["orders"])]
pub async fn list_orders(ctx: &QueryContext, input: ListOrdersInput) -> Result<Vec<Order>> {
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
#[forge::mutation(transactional)]
pub async fn create_order(ctx: &MutationContext, input: CreateOrderInput) -> Result<Order> {
    let uid = ctx.require_user_id()?;
    let order: Order = ctx.db()
        .fetch_one(
            sqlx::query_as("INSERT INTO orders (user_id, total) VALUES ($1, $2) RETURNING *")
                .bind(uid)
                .bind(input.total_cents),
        )
        .await?;
    ctx.dispatch_job("send_receipt", serde_json::json!({"order_id": order.id})).await?;
    Ok(order)
}
```

Use `transactional` when: multiple related writes, dispatching jobs/workflows, or all-or-nothing consistency needed.

### DbConn calling conventions

- `QueryContext::db()` acts like `&PgPool`. Chain `.fetch_all(ctx.db())` directly on sqlx queries.
- `MutationContext::db()` returns `DbConn<'_>` (transaction-aware). Use `ctx.db().fetch_one(query)` wrapper methods.
- For shared helpers across query/mutation/MCP/test: `pub(crate) async fn helper(db: DbConn<'_>, ...) -> Result<T>`.
- `DbConn` methods: `fetch_one`, `fetch_optional`, `fetch_all`, `execute`.

### Scope enforcement

- All queries/mutations default to authenticated unless `public` is explicitly justified.
- Authenticated inputs with parameters must include an identity key: `user_id`, `userId`, `owner_id`, `subject`, `principal_id`, `tenant_id` (or camelCase variants).
- Router validates identity key matches JWT sub automatically. Do not add manual comparison checks in handlers.
- No-input authenticated handlers: skip input arg entirely, use `ctx.require_user_id()?` directly. Scope enforcement is skipped when there is no input parameter.
- Input/output structs must derive both `Serialize` and `Deserialize`.

### Error mapping

| Condition | Variant | HTTP |
|---|---|---|
| Invalid input | `ForgeError::Validation` | 400 |
| No/bad auth | `ForgeError::Unauthorized` | 401 |
| Wrong scope/role | `ForgeError::Forbidden` | 403 |
| Missing entity | `ForgeError::NotFound` | 404 |
| Rate limited | `ForgeError::RateLimitExceeded` | 429 |
| Server fault (last resort) | `ForgeError::Internal` | 500 |

Do not collapse domain-specific errors into `Internal`. Distinguish `NotFound` from `Forbidden`.

### Project structure

- `src/functions/` for Forge handlers (queries, mutations, jobs, etc.)
- `src/schema/` for all input/output structs, domain models, enums
- `src/utils/` for pure helper logic
- Types always in `src/schema/`, never inline in function files
- Tests inline with `#[cfg(test)] mod tests` at bottom of each file

### Workflow order

1. Backend implementation + tests in `src/functions/` and `src/schema/`
2. Run `forge generate` after backend contracts are correct
3. Frontend implementation using generated client
4. Run lint/type checks (`bun run lint`, `bunx svelte-check`)
5. Run Playwright tests if UI changed
6. Run `forge check` from app root (final gate, must be clean)

## Reference Loading

Most tasks (add a query, fix a mutation, small refactor) need nothing beyond this file.

### No reference needed

- Adding or modifying a simple query or mutation
- Bug fixes in existing handlers
- Small refactors within established patterns

### Load one reference when triggered

| Signal in the task | Load |
|---|---|
| Auth setup, login/register, JWT, protected routes, auth store | `references/auth.md` |
| Jobs, crons, workflows, retries, scheduling, background processing | `references/patterns.md` |
| Webhooks, custom HTTP routes, daemons, MCP tools, file uploads, S3/R2, OAuth, external APIs | `references/integrations.md` |
| Frontend, Svelte 5, components, pages, UI, reactivity, design | `references/frontend.md` |
| Test strategy, Playwright setup, coverage approach | `references/testing.md` |
| Code review, audit, anti-pattern check, quality gate | `references/quality.md` |
| forge.toml config, context API, error types, CLI workflow, project layout | `references/config.md` |
| Read replicas, observability, tracing, production hardening | `references/operations.md` |

### Load two for compound tasks

- Full-stack feature: `references/frontend.md` + `references/testing.md`
- Auth + frontend: `references/auth.md` (includes frontend auth store already)
- Production review: `references/quality.md` + `references/operations.md`

### API uncertainty

Use the context7 skill for live documentation lookup when library APIs may have changed (Svelte 5 runes, third-party SDKs, version-specific behavior).

## Output Contract

### Implementation output

1. Contract summary
2. Files changed and rationale
3. Tests run/added (including Playwright results)
4. Security + scale + observability checks
5. `forge check` result (must be clean)
6. Risks/follow-ups

### Review output

- Findings first, ordered by severity with file/line references
- Then assumptions/open questions
- Then short summary
