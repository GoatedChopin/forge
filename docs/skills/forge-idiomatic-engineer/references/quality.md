# Quality Gates

Review criteria, anti-patterns, security checklists, and clarity refinement.

## Contents

1. [Review rubric](#1-review-rubric)
2. [Anti-patterns and corrections](#2-anti-patterns-and-corrections)
3. [Security checklist](#3-security-checklist)
4. [Data integrity checklist](#4-data-integrity-checklist)
5. [Performance and scaling checklist](#5-performance-and-scaling-checklist)
6. [Observability checklist](#6-observability-checklist)
7. [Default safe attribute sets](#7-default-safe-attribute-sets)
8. [Clarity refinement gate](#8-clarity-refinement-gate)

---

## 1) Review rubric

### Severity levels

- P0: security breach/data corruption/tenant isolation break
- P1: correctness bug likely in normal usage
- P2: reliability/operability/maintainability issue with meaningful impact
- P3: style or minor quality issue

### Review order

1. Auth/authz and scope isolation
2. Transaction and side-effect atomicity
3. Data correctness and edge-case design
4. Read consistency (primary vs replica assumptions)
5. Retry/timeouts/idempotency for async surfaces
6. Observability and trace correlation quality
7. Test coverage: wide case variety, boundary values, pure logic extracted and unit tested
8. `forge check` executed from app root and findings resolved
9. Coverage measurement evidence with 100% line coverage for changed modules (or explicit blocker)
10. Type placement: input/output structs in `schema/`, not inline in function files
11. Readability and future change cost
12. Frontend a11y/reactivity quality when UI changes are in scope
13. Playwright integration coverage exists and was executed when UI changed

### Required checks by primitive

**Query**: scope-safe? explicit `tables` when SQL dynamic? bounded data path? `consistent` on read-after-write paths? explicit column list on hot-path queries? consistency expectation documented when replicas in play?

**Mutation**: should be transactional? precise error mapping? side effects asserted in tests?

**Job**: retry policy appropriate? cancellation/progress for long-running work? idempotency and observability context present?

**Cron**: timezone intentional? catch-up intentional? late-run behavior visible in logs/metrics?

**Workflow**: step names stable? compensation needed/present? timeout realistic?

**Webhook**: signature + idempotency configured? minimal sync work and safe async dispatch?

**MCP**: least privilege? authz + rate limit + timeout present?

**Frontend** (if changed): generated code untouched? runes/reactivity idiomatic? accessible states and keyboard support present?

### Review output template

- Finding title
- Severity
- File and line reference
- Why it matters
- Concrete fix suggestion

## 2) Anti-patterns and corrections

### AP-1: Redundant manual identity comparison

The router's `check_identity_args()` validates identity scope args match the JWT subject before the handler runs. Manual comparison is dead code.

```rust
// WRONG: redundant, the router already enforces this
let uid = ctx.require_user_id()?;
if input.user_id != uid {
    return Err(ForgeError::Forbidden("Access denied".into()));
}

// RIGHT: router already verified input.user_id == JWT sub
let uid = ctx.require_user_id()?;
sqlx::query_as("SELECT * FROM orders WHERE user_id = $1")
    .bind(uid)
    .fetch_all(ctx.db())
    .await
    .map_err(Into::into)
```

Do not take this to mean frontend data is trusted. The backend must still derive the acting user from auth context and validate every other client-supplied field.

### AP-2: Dispatch side effect in non-transactional mutation

```rust
// WRONG
#[forge::mutation]
pub async fn checkout(ctx: &MutationContext, input: CheckoutInput) -> Result<Order> {
    let order = insert_order(ctx.db(), &input).await?;
    ctx.dispatch_job("send_receipt", json!({"order_id": order.id})).await?;
    Ok(order)
}

// RIGHT: add transactional
#[forge::mutation(transactional)]
```

### AP-3: Generic error swallowing

```rust
// WRONG
.map_err(|_| ForgeError::Internal("failed".into()))

// RIGHT: preserve cause and use specific variant
.map_err(|e| ForgeError::Database(e.to_string()))
```

### AP-4: `unwrap` in runtime logic

```rust
// WRONG
let subject = ctx.auth.subject().unwrap();

// RIGHT
let subject = ctx.require_subject()?;
```

### AP-5: Dynamic SQL without explicit table dependencies

```rust
// WRONG: reactive invalidation won't work
#[forge::query]
pub async fn search(ctx: &QueryContext, table: String) -> Result<Vec<Row>> { ... }

// RIGHT
#[forge::query(tables = ["users", "orders"])]
```

### AP-6: Business logic duplicated across handlers

Same SQL and validation repeated in query/mutation/MCP. Move to `pub(crate) async fn helper(db: DbConn<'_>, ...)`.

### AP-7: Unbounded list endpoints

Add pagination or limit. Keep deterministic ordering.

### AP-8: Weak async observability

```rust
// WRONG
tracing::info!("job failed");

// RIGHT
tracing::warn!(job_id=%ctx.job_id, attempt=ctx.attempt, "job failed");
```

### AP-9: Editing generated Forge frontend files

Change Rust schema/functions, then run `forge generate`. Never hand-edit `frontend/src/lib/forge/*` or `frontend/src/forge/*`.

Do not hand-create fake generated bindings in those directories either.

### AP-10: Backend changed but generation skipped

After backend schema/function changes, run `forge generate` before frontend integration or delivery.

### AP-10b: Deferring required tooling steps to the user

If the task depends on `forge generate`, migrations, or the project dev flow, resolve and execute those steps during the task when feasible. "You can run this later" is not a complete implementation when the deliverable is supposed to work out of the box.

### AP-10c: Handlers added but not registered in `main.rs`

Forge handler macros do not make endpoints reachable on their own. If new queries or mutations are added but `src/main.rs` is not updated to register them, the code can compile while the runtime returns "Function '...' not found".

### AP-11: Shipping without root-level `forge check`

Run `forge check` from the app root (`forge.toml` directory), fix findings, rerun until clean.

### AP-12: UI changed without Playwright execution

Add or update a basic Playwright integration path and execute it before delivery.

### AP-12b: Behavior changed without new tests

`forge check`, lint, and type checks are not tests. If handlers, validation rules, migrations, or UI behavior changed, add or update tests instead of treating clean tooling output as sufficient evidence.

### AP-12c: Declaring success before end-to-end boot verification

For app tasks that are supposed to work immediately, do not claim the work is clean, complete, or ready until the real dev flow boots and the primary user path has been exercised. Static checks and unit tests alone do not prove the app actually works.

### AP-13: Manual refetch loops in reactive UI

```typescript
// WRONG
await updateTask(args);
await refetchTasks();

// RIGHT: use Forge reactivity + tracked stores; let invalidation update subscribers
```

### AP-14: Ignoring trace/correlation metadata in external calls

```rust
// WRONG
ctx.http().post(url).json(&payload).send().await?;

// RIGHT
ctx.http()
  .post(url)
  .header("x-request-id", ctx.request.request_id.to_string())
  .header("x-trace-id", ctx.request.trace_id.clone())
  .json(&payload)
  .send()
  .await?;
```

### AP-15: Missing `consistent` on read-right-after-write path

Most queries belong on replicas. Only use `consistent` for the few endpoints where a user writes something and immediately reads it back (e.g. checkout then receipt).

### AP-16: `SELECT *` on hot-path reactive queries

Prefer explicit column lists. `SELECT *` defeats column-aware invalidation and triggers re-execution on every UPDATE.

### AP-17: Frontend before backend is stable

Complete backend behavior + tests first, then build frontend integration.

### AP-18: Input/output types inline in function files

Keep all type definitions in `src/schema/`. Handlers import from there.

### AP-19: Overusing `$effect`

Prefer `$derived` for computed state and explicit event handlers for user actions. Use `$effect` only for unavoidable imperative side effects.

### AP-20: Skipping frontend quality gates

Run the real frontend checks for the active target. For SvelteKit that usually means `eslint` and `svelte-check`; for Dioxus it usually means `cargo fmt`, `cargo check`, and the target's Rust-side UI checks.

### AP-21: Generic AI-sounding UI copy

Use concise, domain-specific copy that helps users complete tasks quickly. No "Welcome to our amazing platform where innovation meets excellence."

### AP-22: Dummy input on no-arg handlers

The macro auto-generates a unit type. Do not force a second parameter like `_input: Option<()>`.

### AP-23: Defaulting to public endpoints

Configure auth by default. Only make endpoints public when explicitly justified.

### AP-24: Claiming coverage without evidence

Run coverage tool (`cargo llvm-cov`), report numbers, enforce 100% line coverage on changed modules.

### AP-25: Reporting completion with failing tests

Fix the failing tests first. Only report completion when everything passes.

## 3) Security checklist

- Authentication: default auth retained unless endpoint intentionally public. `require_role("...")` for sensitive operations.
- Authorization: user/tenant scope checks in code and SQL. Never trust client-provided IDs without principal check.
- Secrets: read with `ctx.env_require(...)`. Never log secret values.
- Webhooks: signature verification enabled. Idempotency configured.
- MCP tools: private by default. Timeout and rate limits on external-facing tools.

## 4) Data integrity checklist

- multi-write mutation with side effects is `transactional`
- explicit not-found vs forbidden semantics
- idempotency keys where duplicate execution can cause damage
- workflow step names stable across versions

## 5) Performance and scaling checklist

- Queries: bounded results or pagination. Avoid N+1 DB patterns. Right indexes for common filters. Explicit column lists on hot-path queries (enables column-aware invalidation). `consistent` only where read-after-write is actually needed.
- Connection pools: pool isolation configured when mixed workloads risk starvation (`database.pools.default/jobs/analytics/observability`). Pool sizes tuned to actual workload ratios.
- Jobs: retries tuned to dependency reliability. Long jobs report progress and check cancellation. Worker capability routing for specialized workloads.
- Crons: timezone explicit for business schedules. Catch-up behavior intentional.
- Workflows: only for truly durable multi-step orchestration. Timeout aligned to process horizon.

## 6) Observability checklist

- OTLP config intentional (`enabled`, endpoint, sampling)
- structured logs include key IDs (`request_id`, `trace_id`, entity/run/job IDs)
- async retries/failures include attempt metadata
- quiet routes configured intentionally (health probes usually excluded)

## 7) Default safe attribute sets

### External mutation
```rust
#[forge::mutation(
    timeout = 15,
    rate_limit(requests = 30, per = "1m", key = "user")
)]
```

### External MCP tool
```rust
#[forge::mcp_tool(
    timeout = 15,
    rate_limit(requests = 60, per = "1m", key = "user")
)]
```

### External webhook
```rust
#[forge::webhook(
    path = "/webhooks/provider",
    signature = WebhookSignature::hmac_sha256("X-Signature", "PROVIDER_WEBHOOK_SECRET"),
    idempotency = "header:X-Request-Id",
    timeout = "30s"
)]
```

### Durable job
```rust
#[forge::job(
    timeout = "30m",
    retry(max_attempts = 5, backoff = "exponential", max_backoff = "10m"),
    priority = "normal"
)]
```

## 8) Clarity refinement gate

Run this after implementation and tests, before final delivery. Focus on recently touched files only.

### Goal

Improve clarity, consistency, and maintainability without changing functionality. No behavior changes, no contract changes, no hidden side-effect changes.

### Checks

1. **Complexity and nesting**: reduce unnecessary nesting with early returns/guards. Flatten deeply nested conditional trees. Avoid branching that duplicates the same end behavior.
2. **Clarity over brevity**: avoid dense one-liners that reduce readability. Avoid clever-but-opaque constructions. Prefer explicit and debuggable control flow.
3. **Conditional hygiene**: avoid nested ternary operators in TS/Svelte. Use clear `if/else` or `switch` for multi-condition logic.
4. **Redundancy**: remove duplicate logic introduced during edits. Consolidate repeated code into focused helpers only when it improves clarity. Avoid premature abstraction for one-off code.
5. **Naming and intent**: use meaningful names. Avoid generic placeholders (`handle`, `data`, `tmp`) when domain names exist.
6. **Comments quality**: remove comments that describe obvious code. Keep comments that explain rationale, invariants, and tradeoffs.
7. **Balance check**: do not "simplify" in ways that hurt maintainability. Don't merge unrelated concerns, don't delete useful structure, don't optimize for fewer lines over readability.
8. **Error handling clarity**: keep error mapping explicit and precise. Avoid broad catch-all patterns. Preserve structured context in error logs/messages.
9. **Forge boundary safety**: preserve generated-code boundaries. Keep handlers in `functions`, contracts in `schema`, pure helpers in `utils`. Extract to `utils` only when reuse is demonstrated.

### Practical pass

1. Confirm behavior unchanged (re-check tests/expected contracts).
2. Inspect recently edited blocks only.
3. Remove unnecessary nesting and duplicate branches.
4. Replace nested ternaries (frontend) with clearer control flow.
5. Improve names where meaning is ambiguous.
6. Delete obvious comments; keep rationale comments.
7. Ensure no over-compaction harmed debuggability.
8. Re-run relevant checks/tests.
