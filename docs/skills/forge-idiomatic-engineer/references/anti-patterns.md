# Anti-Patterns and Corrections

## 1) Trusting user_id from input

Bad:
```rust
#[forge::query]
pub async fn my_orders(ctx: &QueryContext, input: ListOrdersInput) -> Result<Vec<Order>> {
    sqlx::query_as("SELECT * FROM orders WHERE user_id = $1")
        .bind(input.user_id)
        .fetch_all(ctx.db())
        .await
        .map_err(Into::into)
}
```

Good:
```rust
let me = ctx.require_user_id()?;
if input.user_id != me {
    return Err(ForgeError::Forbidden("User scope mismatch".into()));
}
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

## 15) Overusing `$effect`

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

## 15) Skipping frontend quality gates

Bad:
```text
Ship UI without lint/type checks, SEO metadata, or accessibility verification
```

Good:
```text
Run CLI checks (`eslint`, `svelte-check`, prefer `bun` when available) and verify SEO + accessibility baseline.
```

## 16) Generic AI-sounding UI copy

Bad:
```text
"Welcome to our amazing platform where innovation meets excellence"
```

Good:
```text
Use concise, domain-specific copy that helps users complete tasks quickly.
```
