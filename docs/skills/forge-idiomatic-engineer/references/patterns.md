# Backend Patterns

Advanced Forge patterns for background processing, observability, and design.

## Contents

1. [Shared DbConn helper](#1-shared-dbconn-helper)
2. [Boundary validation](#2-boundary-validation)
3. [Job with retry, progress, cancellation](#3-job-with-retry-progress-cancellation)
4. [Cron with timezone and late-run handling](#4-cron-with-timezone-and-late-run-handling)
5. [Workflow with compensation](#5-workflow-with-compensation)
6. [Edge-case elimination through design](#6-edge-case-elimination-through-design)
7. [Trace propagation to external HTTP](#7-trace-propagation-to-external-http)
8. [Read consistency (strict vs eventual)](#8-read-consistency-strict-vs-eventual)
9. [Structured logging](#9-structured-logging)

---

## 1) Shared DbConn helper

Use one helper for query/mutation/MCP/test reuse.

```rust
use forge::forge_core::DbConn;
use forge::prelude::*;

pub(crate) async fn get_order_by_id(db: DbConn<'_>, id: uuid::Uuid) -> Result<Order> {
    db.fetch_optional(sqlx::query_as("SELECT * FROM orders WHERE id = $1").bind(id))
        .await?
        .ok_or_else(|| ForgeError::NotFound("Order not found".into()))
}

#[forge::query]
pub async fn order(ctx: &QueryContext, id: uuid::Uuid) -> Result<Order> {
    get_order_by_id(ctx.db_conn(), id).await
}

#[forge::mutation]
pub async fn order_for_update(ctx: &MutationContext, id: uuid::Uuid) -> Result<Order> {
    get_order_by_id(ctx.db(), id).await
}
```

## 2) Boundary validation

```rust
fn normalized_title(raw: &str) -> Result<String> {
    let v = raw.trim();
    if v.is_empty() {
        return Err(ForgeError::Validation("Title cannot be empty".into()));
    }
    if v.len() > 120 {
        return Err(ForgeError::Validation("Title must be <= 120 chars".into()));
    }
    Ok(v.to_string())
}
```

## 3) Job with retry, progress, cancellation

```rust
#[forge::job(
    priority = "high",
    retry(max_attempts = 5, backoff = "exponential", max_backoff = "10m"),
    timeout = "30m",
    idempotent(key = "invoice_id")
)]
pub async fn generate_invoice_pdf(ctx: &JobContext, input: GenerateInvoicePdfInput) -> Result<()> {
    ctx.progress(0, "Starting")?;

    for (idx, item) in input.items.iter().enumerate() {
        ctx.check_cancelled().await?;
        render_line_item(item).await?;
        let pct = ((idx + 1) * 100 / input.items.len()) as u8;
        ctx.progress(pct, "Rendering")?;
    }

    Ok(())
}
```

## 4) Cron with timezone and late-run handling

```rust
#[forge::cron("0 9 * * *", timezone = "America/New_York", catch_up, catch_up_limit = 3)]
pub async fn daily_digest(ctx: &CronContext) -> Result<()> {
    if ctx.is_late() {
        ctx.log.warn(
            "Digest run delayed",
            serde_json::json!({
                "run_id": ctx.run_id,
                "delay_secs": ctx.delay().num_seconds()
            }),
        );
    }

    run_digest(ctx.db()).await
}
```

## 5) Workflow with compensation

```rust
#[forge::workflow(version = 2, timeout = "14d")]
pub async fn account_closure(ctx: &WorkflowContext, input: CloseAccountInput) -> Result<()> {
    ctx.step("revoke_sessions", || async {
        revoke_sessions(input.user_id).await
    })
    .compensate(|_| async move {
        restore_sessions(input.user_id).await
    })
    .run()
    .await?;

    ctx.step("archive_data", || async { archive_data(input.user_id).await })
        .run()
        .await?;

    ctx.sleep(std::time::Duration::from_secs(7 * 24 * 60 * 60)).await?;

    ctx.step("hard_delete", || async { hard_delete_user(input.user_id).await })
        .run()
        .await?;

    Ok(())
}
```

## 6) Edge-case elimination through design

Don't patch symptoms with special-case branches. Redesign so the problem becomes impossible.

- Normalize optional state once at the top.
- Use one loop/operation path.
- Commit exactly once.
- In SQL, use CTE/upsert patterns to keep one write path instead of split logic.

## 7) Trace propagation to external HTTP

```rust
#[forge::mutation]
pub async fn sync_partner(ctx: &MutationContext, input: SyncInput) -> Result<()> {
    let req_id = ctx.request.request_id.to_string();
    let trace_id = ctx.request.trace_id.clone();

    ctx.http()
        .post("https://partner.example/api/sync")
        .header("x-request-id", req_id)
        .header("x-trace-id", trace_id)
        .json(&input)
        .send()
        .await?;

    Ok(())
}
```

## 8) Read consistency (strict vs eventual)

```rust
#[forge::query(consistent)]
pub async fn invoice_after_checkout(ctx: &QueryContext, id: uuid::Uuid) -> Result<Invoice> {
    // `consistent` forces primary read, bypassing replicas.
    sqlx::query_as!(Invoice, "SELECT * FROM invoices WHERE id = $1", id)
        .fetch_one(ctx.db())
        .await
        .map_err(Into::into)
}

#[forge::query(cache = "15s")]
pub async fn invoice_dashboard(ctx: &QueryContext) -> Result<Vec<InvoiceSummary>> {
    // Replica-safe: dashboard tolerates lag. Explicit columns enable column-aware invalidation.
    sqlx::query_as!(InvoiceSummary, "SELECT status, count(*) AS count FROM invoices GROUP BY status")
        .fetch_all(ctx.db())
        .await
        .map_err(Into::into)
}
```

## 9) Structured logging

```rust
#[forge::job(retry(max_attempts = 5, backoff = "exponential"))]
pub async fn send_receipt(ctx: &JobContext, input: ReceiptInput) -> Result<()> {
    tracing::info!(
        job_id = %ctx.job_id,
        attempt = ctx.attempt,
        order_id = %input.order_id,
        "Sending receipt"
    );

    // ...

    Ok(())
}
```

Prefer fields over interpolated strings. For retries/failures include attempt number, max attempts, dependency name, and error class.
