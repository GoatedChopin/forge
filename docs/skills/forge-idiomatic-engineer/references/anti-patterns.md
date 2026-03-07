# Anti-Patterns and Corrections

## 1) Redundant manual identity comparison

The router's `check_identity_args()` validates identity scope args (e.g. `user_id`) match the JWT subject before the handler runs. Manual comparison is dead code.

Bad:
```rust
let uid = ctx.require_user_id()?;
if input.user_id != uid {
    return Err(ForgeError::Forbidden("Access denied".into()));
}
```

Good:
```rust
// Router already verified input.user_id == JWT sub
let uid = ctx.require_user_id()?;
// use uid directly in queries
sqlx::query_as("SELECT * FROM orders WHERE user_id = $1")
    .bind(uid)
    .fetch_all(ctx.db())
    .await
    .map_err(Into::into)
```

## 2) Dispatch side effect in non-transactional mutation

Bad:
```rust
#[forge::mutation]
pub async fn checkout(ctx: &MutationContext, input: CheckoutInput) -> Result<Order> {
    let order = insert_order(ctx.db(), &input).await?;
    ctx.dispatch_job("send_receipt", json!({"order_id": order.id})).await?;
    Ok(order)
}
```

Good:
```rust
#[forge::mutation(transactional)]
```

## 3) Generic error swallowing

Bad:
```rust
.map_err(|_| ForgeError::Internal("failed".into()))
```

Good:
```rust
.map_err(|e| ForgeError::Database(e.to_string()))
```
Or map to `Validation`/`NotFound` where domain semantics are known.

## 4) `unwrap` in runtime logic

Bad:
```rust
let subject = ctx.auth.subject().unwrap();
```

Good:
```rust
let subject = ctx.require_subject()?;
```

## 5) Dynamic SQL without explicit table dependencies

Bad:
```rust
#[forge::query]
pub async fn search(ctx: &QueryContext, table: String) -> Result<Vec<Row>> { ... }
```

Good:
```rust
#[forge::query(tables = ["users", "orders"])]
```

## 6) Business logic duplicated across handlers

Bad: same SQL and validation repeated in query/mutation/MCP.

Good: move to `pub(crate) async fn helper(db: DbConn<'_>, ...)`.

## 7) Unbounded list endpoints

Bad:
```rust
SELECT * FROM events ORDER BY created_at DESC
```

Good:
- add pagination or limit
- keep deterministic ordering

## 8) Weak async observability

Bad:
```rust
tracing::info!("job failed");
```

Good:
```rust
tracing::warn!(job_id=%ctx.job_id, attempt=ctx.attempt, "job failed");
```

## 9) Editing generated Forge frontend files

Bad:
```text
Edit frontend/src/lib/forge/api.ts by hand
```

Good:
```text
Change Rust schema/functions, then run forge generate
```

## 9.5) Backend changed but generation skipped

Bad:
```text
Add/update backend contracts, then forget to regenerate frontend/runtime bindings
```

Good:
```text
After backend schema/function changes, run forge generate before frontend integration or delivery
```

## 9.6) Shipping without root-level forge check

Bad:
```text
Assume code is fine and skip forge check
```

Good:
```text
Run forge check from the app root (forge.toml directory), fix findings, rerun until clean
```

## 9.7) UI changed without Playwright integration execution

Bad:
```text
Only run lint/type checks after UI changes
```

Good:
```text
Add or update a basic Playwright integration path and execute it before delivery
```

## 10) Manual refetch loops in reactive UI

Bad:
```typescript
await updateTask(args);
await refetchTasks();
```

Good:
```text
Use Forge reactivity + tracked stores; let invalidation update subscribers.
```

## 11) Ignoring trace/correlation metadata in external calls

Bad:
```rust
ctx.http().post(url).json(&payload).send().await?;
```

Good:
```rust
ctx.http()
  .post(url)
  .header("x-request-id", ctx.request.request_id.to_string())
  .header("x-trace-id", ctx.request.trace_id.clone())
  .json(&payload)
  .send()
  .await?;
```

## 12) Missing `consistent` on the rare read-right-after-write path

Most queries belong on replicas. This only matters for the few endpoints where a user
writes something and immediately reads it back in the same flow (e.g. checkout then receipt).

Bad:
```rust
// Mutation writes the order, frontend navigates to receipt page.
// Without `consistent`, the receipt query may hit a replica that hasn't replicated yet.
#[forge::query]
pub async fn get_receipt(ctx: &QueryContext, order_id: Uuid) -> Result<Order> {
    sqlx::query_as("SELECT * FROM orders WHERE id = $1")
        .bind(order_id).fetch_one(ctx.db()).await.map_err(Into::into)
}
```

Good:
```rust
// Only needed here because the user literally just created this order.
// Dashboards, listings, search, reports should all stay on replicas.
#[forge::query(consistent)]
pub async fn get_receipt(ctx: &QueryContext, order_id: Uuid) -> Result<Order> {
    sqlx::query_as("SELECT * FROM orders WHERE id = $1")
        .bind(order_id).fetch_one(ctx.db()).await.map_err(Into::into)
}
```

## 13) `SELECT *` on hot-path reactive queries

Bad:
```rust
#[forge::query]
pub async fn list_tasks(ctx: &QueryContext) -> Result<Vec<Task>> {
    // SELECT * defeats column-aware invalidation. Every UPDATE triggers re-execution.
    sqlx::query_as("SELECT * FROM tasks").fetch_all(ctx.db()).await.map_err(Into::into)
}
```

Good:
```rust
sqlx::query_as("SELECT id, title, status FROM tasks")
```

## 14) Frontend before backend is stable

Bad:
```text
Start UI before backend contract/tests are finalized
```

Good:
```text
Complete backend behavior + tests first, then build frontend integration.
```

## 15) Defining input/output types inline in function files

Bad:
```rust
// src/functions/orders.rs
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct CreateOrderInput {
    pub user_id: uuid::Uuid,
    pub total_cents: i64,
}

#[forge::mutation(transactional)]
pub async fn create_order(ctx: &MutationContext, input: CreateOrderInput) -> Result<Order> {
    // ...
}
```

Good:
```rust
// src/schema/order.rs
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct CreateOrderInput {
    pub user_id: uuid::Uuid,
    pub total_cents: i64,
}

// src/functions/orders.rs
use crate::schema::order::CreateOrderInput;

#[forge::mutation(transactional)]
pub async fn create_order(ctx: &MutationContext, input: CreateOrderInput) -> Result<Order> {
    // ...
}
```

Keep all type definitions in `src/schema/` so handlers stay focused on behavior.

## 16) Overusing `$effect`

Bad:
```svelte
$effect(() => {
  // complex derived logic and network orchestration
});
```

Good:
```text
Prefer `$derived` for computed state and explicit event handlers for user actions.
Use `$effect` only for unavoidable imperative side effects.
```

## 17) Skipping frontend quality gates

Bad:
```text
Ship UI without lint/type checks, SEO metadata, or accessibility verification
```

Good:
```text
Run CLI checks (`eslint`, `svelte-check`, prefer `bun` when available) and verify SEO + accessibility baseline.
```

## 18) Generic AI-sounding UI copy

Bad:
```text
"Welcome to our amazing platform where innovation meets excellence"
```

Good:
```text
Use concise, domain-specific copy that helps users complete tasks quickly.
```

## 19) Dummy input on no-arg handlers

The macro auto-generates a unit type when the handler takes no input. Do not force a second parameter.

Bad:
```rust
#[forge::query(public)]
pub async fn list_todos(ctx: &QueryContext, _input: Option<()>) -> Result<Vec<Todo>> {
    // ...
}
```

Good:
```rust
#[forge::query(public)]
pub async fn list_todos(ctx: &QueryContext) -> Result<Vec<Todo>> {
    // ...
}
```

## 20) Defaulting to public endpoints

Bad:
```text
Skip [auth] config or make endpoints public when the user didn't explicitly ask for no auth
```

Good:
```text
Configure auth by default. Only make endpoints public when explicitly justified.
```

## 21) Claiming coverage without evidence

Bad:
```text
"Tests cover all cases" with no measurement or coverage report
```

Good:
```text
Run coverage tool (cargo llvm-cov), report numbers, enforce 100% line coverage on changed modules.
```

## 22) Reporting completion with failing tests

Bad:
```text
"Task complete. Note: two Playwright tests are failing."
```

Good:
```text
Fix the failing tests first. Only report completion when everything passes.
```
