# Pattern Cookbook

Concrete, Forge-native patterns.

## 1) Shared helper with `DbConn<'_>`

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

## 2) Boundary validation function

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

## 3) Scope-safe query

```rust
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct ListOrdersInput {
    pub user_id: uuid::Uuid,
}

#[forge::query(tables = ["orders"])]
pub async fn list_orders(ctx: &QueryContext, input: ListOrdersInput) -> Result<Vec<Order>> {
    let me = ctx.require_user_id()?;
    if input.user_id != me {
        return Err(ForgeError::Forbidden("User scope mismatch".into()));
    }

    sqlx::query_as("SELECT * FROM orders WHERE user_id = $1 ORDER BY created_at DESC")
        .bind(me)
        .fetch_all(ctx.db())
        .await
        .map_err(Into::into)
}
```

## 4) Transactional mutation + outbox side effects

```rust
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct CreateInvoiceInput {
    pub user_id: uuid::Uuid,
    pub total_cents: i64,
}

#[forge::mutation(transactional)]
pub async fn create_invoice(ctx: &MutationContext, input: CreateInvoiceInput) -> Result<Invoice> {
    let me = ctx.require_user_id()?;
    if input.user_id != me {
        return Err(ForgeError::Forbidden("User scope mismatch".into()));
    }

    let invoice: Invoice = ctx
        .db()
        .fetch_one(
            sqlx::query_as("INSERT INTO invoices (user_id, total_cents) VALUES ($1, $2) RETURNING *")
                .bind(me)
                .bind(input.total_cents),
        )
        .await?;

    ctx.dispatch_job(
        "send_invoice_email",
        serde_json::json!({ "invoice_id": invoice.id }),
    )
    .await?;

    Ok(invoice)
}
```

## 5) Job with retry, progress, cancellation

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

## 6) Cron with explicit timezone and late-run logging

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

## 7) Workflow with compensation and stable step names

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

## 8) Webhook with signature + idempotency

```rust
#[forge::webhook(
    path = "/webhooks/stripe",
    signature = WebhookSignature::hmac_sha256("Stripe-Signature", "STRIPE_WEBHOOK_SECRET"),
    idempotency = "body:$.id"
)]
pub async fn stripe_webhook(ctx: &WebhookContext, payload: serde_json::Value) -> Result<WebhookResult> {
    let event_type = payload.get("type").and_then(|v| v.as_str()).unwrap_or("unknown");

    ctx.dispatch_job(
        "process_stripe_event",
        serde_json::json!({
            "event_type": event_type,
            "payload": payload,
        }),
    )
    .await?;

    Ok(WebhookResult::Accepted)
}
```

## 9) MCP tool least privilege

```rust
#[forge::mcp_tool(
    name = "tickets.list",
    title = "List Tickets",
    description = "Read-only support ticket listing",
    require_role("support"),
    read_only,
    rate_limit(requests = 60, per = "1m", key = "user")
)]
pub async fn mcp_list_tickets(ctx: &McpToolContext) -> Result<Vec<Ticket>> {
    list_tickets(ctx.db_conn()).await
}
```

## 10) Design to remove edge-case branching

Bad pattern: separate head/first/initial branch then second branch for rest.

Better pattern:
- Normalize optional state once.
- Use one loop/operation path.
- Commit exactly once.

In SQL, use CTE/upsert patterns to keep one write path instead of split logic.

## 11) Request trace propagation to external HTTP

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

## 12) Read consistency split (strict vs eventual)

```rust
#[forge::query(consistent)]
pub async fn invoice_after_checkout(ctx: &QueryContext, id: uuid::Uuid) -> Result<Invoice> {
    // Strict path: `consistent` forces primary read, bypassing replicas.
    sqlx::query_as("SELECT * FROM invoices WHERE id = $1")
        .bind(id)
        .fetch_one(ctx.db())
        .await
        .map_err(Into::into)
}

#[forge::query(cache = "15s")]
pub async fn invoice_dashboard(ctx: &QueryContext) -> Result<Vec<InvoiceSummary>> {
    // Replica-safe: dashboard tolerates lag. Explicit columns enable column-aware invalidation.
    sqlx::query_as("SELECT status, count(*) AS count FROM invoices GROUP BY status")
        .fetch_all(ctx.db())
        .await
        .map_err(Into::into)
}
```

## 13) Structured async logging pattern

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
