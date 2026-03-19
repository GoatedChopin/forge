# Backend Patterns

Jobs, workflows, crons, daemons, and shared design patterns.

## Shared DbConn Pattern

Write helpers that work across query, mutation, MCP, and tests:

```rust
pub async fn list_items(db: DbConn<'_>) -> Result<Vec<Item>> {
    db.fetch_all(sqlx::query_as("SELECT * FROM items ORDER BY created_at DESC"))
        .await
        .map_err(Into::into)
}

#[forge::query]
pub async fn get_items(ctx: &QueryContext) -> Result<Vec<Item>> {
    list_items(ctx.db_conn()).await
}

#[forge::mutation]
pub async fn items_snapshot(ctx: &MutationContext, input: Input) -> Result<Vec<Item>> {
    list_items(ctx.db()).await  // MutationContext.db() returns DbConn
}
```

## Jobs

```rust
#[forge::job(
    priority = "high",
    retry(max_attempts = 5, backoff = "exponential", max_backoff = "10m"),
    worker_capability = "media",
    timeout = "30m"
)]
pub async fn process_video(ctx: &JobContext, args: ProcessArgs) -> Result<VideoResult> {
    ctx.progress(0, "Starting").unwrap();

    for (i, chunk) in chunks.iter().enumerate() {
        ctx.check_cancelled().await?;  // exit early if cancelled
        process_chunk(chunk).await?;
        ctx.heartbeat().await?;        // prevent stale reclaim
        ctx.progress(((i + 1) * 100 / total) as u8, format!("Chunk {}", i + 1)).unwrap();
    }

    Ok(result)
}
```

Key behaviors:
- Backoff: `fixed` (constant), `linear` (base * attempt), `exponential` (base * 2^(attempt-1))
- Default max_backoff: 5m. Stale reclaim: 5 minutes with no heartbeat.
- `progress(percent, msg)` is sync. `heartbeat()`, `save()`, `saved()`, `is_cancel_requested()`, `check_cancelled()` are async.
- SKIP LOCKED prevents thundering herd. Workers claim with `FOR UPDATE SKIP LOCKED`.

Dispatch from mutations (requires `transactional`):
```rust
#[forge::mutation(transactional)]
pub async fn start_export(ctx: &MutationContext, input: ExportInput) -> Result<Uuid> {
    let job_id = ctx.dispatch_job("export_data", json!({"format": input.format})).await?;
    Ok(job_id)
}
```

## Crons

```rust
#[forge::cron("0 */6 * * *", timezone = "America/New_York", catch_up, catch_up_limit = 3)]
pub async fn sync_external(ctx: &CronContext) -> Result<()> {
    if ctx.is_late() {
        ctx.log.warn("Running late", json!({"delay_secs": ctx.delay().num_seconds()}));
    }

    if ctx.is_catch_up {
        // lightweight version for catch-up runs
    }

    Ok(())
}
```

Exactly-once via `UNIQUE(cron_name, scheduled_time)`. Leader-only execution. 5-part cron expressions (seconds added automatically).

## Workflows

```rust
#[forge::workflow(version = 1, timeout = "30d")]
pub async fn order_fulfillment(ctx: &WorkflowContext, order_id: Uuid) -> Result<()> {
    let payment = ctx.step("charge", || async {
        charge_card(order_id).await
    })
    .timeout(Duration::from_secs(30))
    .retry(3, Duration::from_secs(5))    // 3 retries = 4 total attempts
    .compensate(|result| async move {
        refund(result.charge_id).await
    })
    .run()
    .await?;  // Result<Option<PaymentResult>>

    // Durable sleep survives restarts
    ctx.sleep(Duration::from_days(1)).await?;

    ctx.step("ship", || async { ship_order(order_id).await })
        .optional()  // failure returns Ok(None), no compensation triggered
        .run()
        .await?;

    Ok(())
}
```

Key behaviors:
- Steps are cached: on resume, completed steps return cached result without re-executing
- Compensation runs in reverse order on failure
- `ctx.workflow_time()` for deterministic replay (not wall clock)
- `ctx.parallel().step("a", ...).step("b", ...).run().await?` for parallel execution
- Parallel steps use `FnOnce` (no retry). Sequential steps use `Fn` (supports retry).
- `wait_for_event::<T>(name, timeout)` for external event correlation

## Daemons

```rust
#[forge::daemon(leader_elected = true, restart_delay = "10s", max_restarts = 5)]
pub async fn queue_processor(ctx: &DaemonContext) -> Result<()> {
    loop {
        tokio::select! {
            _ = process_batch(&ctx) => {}
            _ = ctx.shutdown_signal() => break,
        }
        ctx.heartbeat().await?;
    }
    Ok(())
}
```

Advisory lock-based leadership via `pg_try_advisory_lock`. Lock released on connection drop (automatic failover). Dispatch jobs/workflows from daemons (no auth context, owner = None).

## Design Patterns

### Boundary Validation

Validate and normalize at the handler boundary, not deep in helpers:
```rust
#[forge::mutation]
pub async fn create_item(ctx: &MutationContext, input: CreateItemInput) -> Result<Item> {
    let title = input.title.trim();
    if title.is_empty() {
        return Err(ForgeError::Validation("Title required".into()));
    }
    insert_item(ctx.db(), title).await
}
```

### Transactional Outbox

Jobs dispatched in `transactional` mutations are buffered in memory and inserted after commit. If the mutation fails, no jobs are created. No 2PC needed.

### Idempotency

Jobs: `idempotent(key = "request_id")` prevents duplicate processing.
Webhooks: `idempotency = "header:X-Request-Id"` or `"body:$.id"` with 24h TTL.

### Structured Logging

```rust
tracing::info!(
    job_id = %ctx.job_id,
    attempt = ctx.attempt,
    entity_id = %args.id,
    "Processing entity"
);
```

Use context fields, not string interpolation. Cron has `ctx.log.info("msg")` for structured output.

## Trace Propagation

Pass request context to external HTTP calls:
```rust
let response = ctx.http()
    .post(url)
    .header("x-request-id", ctx.request.request_id.to_string())
    .header("x-trace-id", &ctx.request.trace_id)
    .json(&body)
    .send()
    .await?;
```
