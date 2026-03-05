# Read Replicas, Observability, and Trace Context

## 1) Read Replicas: Proper Usage

## Enablement

```toml
[database]
url = "${DATABASE_URL}"
replica_urls = ["${DATABASE_REPLICA_1}", "${DATABASE_REPLICA_2}"]
read_from_replica = true
```

## Routing behavior
- Queries route to healthy replicas via round-robin when `read_from_replica = true`.
- Mutations always target primary.
- Health monitor pings each replica every 15s with `SELECT 1`. Unhealthy replicas are skipped automatically.
- If all replicas fail, reads fall back to primary with no config changes needed.

## Consistent reads

Use `#[forge::query(consistent)]` to force reads from primary, bypassing replicas:

```rust
#[forge::query(consistent)]
pub async fn get_order_receipt(ctx: &QueryContext, order_id: Uuid) -> Result<Order> {
    sqlx::query_as("SELECT * FROM orders WHERE id = $1")
        .bind(order_id)
        .fetch_one(ctx.db())
        .await
        .map_err(Into::into)
}
```

Use `consistent` when you need read-your-write consistency:
- immediate post-mutation confirmation screens
- permission checks that depend on just-written state
- idempotency checks that must observe latest writes

Replica-safe cases (no `consistent` needed):
- dashboards/analytics tolerant to slight lag
- non-critical listing screens
- background reporting

## Pool isolation (bulkhead)

Separate connection pools prevent workload starvation:

```toml
[database.pools.default]
size = 30         # queries, mutations, reactor, rate limiter
timeout_secs = 30

[database.pools.jobs]
size = 15         # job workers, cron, daemons, workflows
timeout_secs = 60
statement_timeout_secs = 300

[database.pools.analytics]
size = 5          # user code via db.analytics_pool()
timeout_secs = 120
statement_timeout_secs = 600

[database.pools.observability]
size = 3          # internal metrics collection
timeout_secs = 5
```

Unconfigured pools fall back to primary. Configure them when workloads risk starving each other.

## Column-aware invalidation

Forge extracts selected columns at compile time. When an UPDATE notification arrives with changed columns, queries whose selected columns don't overlap skip re-execution entirely.

```rust
// Selected columns: [title, completed]. An UPDATE to only `updated_at` won't trigger re-execution.
#[forge::query]
pub async fn list_todos(ctx: &QueryContext) -> Result<Vec<Todo>> {
    sqlx::query_as("SELECT title, completed FROM todos")
        .fetch_all(ctx.db())
        .await
        .map_err(Into::into)
}
```

For `SELECT *` queries, all UPDATEs trigger re-execution. Prefer explicit column lists in hot-path queries for best performance.

## Design pattern
- Use `consistent` on strict read-after-write paths; leave it off for eventual-consistency-tolerant reads.
- Size pool isolation to match actual workload ratios.
- Use explicit column lists in performance-critical queries to benefit from column-aware invalidation.

## 2) Observability Configuration

```toml
[observability]
enabled = true
otlp_endpoint = "http://localhost:4318"
service_name = "my-app"
enable_traces = true
enable_metrics = true
enable_logs = true
sampling_ratio = 0.5
log_level = "info"
```

## What should be observable
- HTTP request lifecycle
- function execution durations
- job execution outcomes and retries
- cron and workflow lifecycle events
- DB pool pressure and slow queries

## Gateway noise control
- `quiet_routes` suppresses telemetry for selected paths.
- Keep health checks quiet by default unless you need full probe telemetry.

## 3) Trace Context and Request Correlation

Use request metadata for correlation:
- `ctx.request.request_id`
- `ctx.request.trace_id`

For outgoing HTTP, forward context where possible:

```rust
let req_id = ctx.request.request_id.to_string();
let trace_id = ctx.request.trace_id.clone();

ctx.http()
    .post("https://api.partner.example/events")
    .header("x-request-id", req_id)
    .header("x-trace-id", trace_id)
    .json(&payload)
    .send()
    .await?;
```

For jobs/workflows/crons where request metadata may not exist the same way, always log run identifiers:
- `job_id`
- `run_id`
- workflow ID + current step

## 4) Structured Logging Guidelines

Prefer fields over interpolated strings:

```rust
tracing::info!(
    request_id = %ctx.request.request_id,
    trace_id = %ctx.request.trace_id,
    user_id = ?ctx.auth.user_id(),
    order_id = %order.id,
    "Order created"
);
```

For retries/failures include:
- attempt number
- max attempts
- external endpoint or dependency name
- error class/message

## 5) Operational Checklist

- replica routing enabled only when intended
- strict-consistency endpoints do not rely on replica freshness
- OTLP endpoint reachable and sampled properly
- identifiers present across logs for every async boundary
- slow-path operations emit enough detail to debug production incidents
