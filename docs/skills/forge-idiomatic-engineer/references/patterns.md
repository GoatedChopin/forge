# Patterns Reference

Backend patterns, auth, integrations, testing, and operations.

## 1. Backend Patterns

### DbConn for Shared Helpers

Write helpers that work across query, mutation, MCP, and tests:

```rust
pub async fn list_items(db: DbConn<'_>) -> Result<Vec<Item>> {
    db.fetch_all(sqlx::query_as!(Item, "SELECT * FROM items ORDER BY created_at DESC"))
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

### Jobs

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

- Backoff: `fixed`, `linear` (base * attempt), `exponential` (base * 2^(attempt-1)). Default max_backoff: 5m.
- `progress(percent, msg)` is sync. `heartbeat()`, `check_cancelled()` are async.
- SKIP LOCKED prevents thundering herd. Stale reclaim after 5 min with no heartbeat.
- Idempotency: `idempotent(key = "request_id")` prevents duplicate processing.

Dispatch from mutations (requires `transactional`):
```rust
#[forge::mutation(transactional)]
pub async fn start_export(ctx: &MutationContext, input: ExportInput) -> Result<Uuid> {
    let job_id = ctx.dispatch_job("export_data", json!({"format": input.format})).await?;
    Ok(job_id)
}
```

Jobs dispatched in `transactional` mutations are buffered and inserted after commit. If the mutation fails, no jobs are created.

### Crons

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

Exactly-once via `UNIQUE(cron_name, scheduled_time)`. Leader-only execution. 5-part cron expressions.

### Workflows

```rust
#[forge::workflow(name = "order_fulfillment", version = "2026-05", timeout = "30d")]
pub async fn order_fulfillment(ctx: &WorkflowContext, order_id: Uuid) -> Result<()> {
    let payment = ctx.step("charge", || async {
        charge_card(order_id).await
    })
    .timeout(Duration::from_secs(30))
    .retry(3, Duration::from_secs(5))
    .compensate(|result| async move { refund(result.charge_id).await })
    .run()
    .await?;

    ctx.sleep(Duration::from_days(1)).await?;  // durable, survives restarts

    ctx.step("ship", || async { ship_order(order_id).await })
        .optional()  // failure returns Ok(None), no compensation
        .run()
        .await?;

    Ok(())
}
```

- Steps are cached: on resume, completed steps return cached result without re-executing.
- Compensation runs in reverse order. Step names must be stable across deploys (cache keys).
- `ctx.workflow_time()` for deterministic replay (not wall clock).
- Sequential steps use `Fn` (supports retry). Parallel steps use `FnOnce` (no retry).

#### Versioning

Every workflow definition has a `name`, `version`, and a derived `signature`. The signature is computed from step keys, wait keys, event names, timeout, and type shapes. On startup, the runtime persists all definitions to `forge_workflow_definitions` and stamps each new run with `workflow_version` and `workflow_signature`.

Two versions of the same workflow can coexist: one `active` (receives new runs) and one `deprecated` (drains in-flight runs). At most one active version per name.

```rust
// Current version: new dispatches go here
#[forge::workflow(name = "user_onboarding", version = "2026-05", active)]
pub async fn user_onboarding_v2(ctx: &WorkflowContext, input: OnboardingInput) -> Result<()> {
    ctx.step("verify_email", || async { verify(input.email).await }).run().await?;
    ctx.step("provision", || async { provision(input.user_id).await }).run().await?;
    Ok(())
}

// Previous version: finishes what's already running, no new runs
#[forge::workflow(name = "user_onboarding", version = "2026-03", deprecated)]
pub async fn user_onboarding_v1(ctx: &WorkflowContext, input: OnboardingInputV1) -> Result<()> {
    ctx.step("send_welcome", || async { welcome(input.user_id).await }).run().await?;
    Ok(())
}
```

Once all runs for the deprecated version complete, remove the handler entirely. Until then, keep it deployed or the readiness endpoint will report `BlockedMissingHandler`.

**Breaking vs safe changes.** Adding a new step at the end of a workflow is safe within the same version because in-flight runs haven't reached it yet. Renaming, reordering, or removing steps changes the signature and requires a new version. Changing the input type shape also changes the signature. When in doubt, bump the version.

**Strict mode and readiness.** The `/_api/ready` endpoint checks for blocked workflow runs. If any run references a version or signature that no registered handler matches, readiness returns 503 with the blocked run details. This prevents deploys from completing when a handler was removed prematurely or its contract was accidentally changed.

#### Event waits

`ctx.wait_for_event(key, timeout)` suspends the workflow until an external signal arrives or the timeout elapses. Returns `Some(T)` on signal, `None` on timeout.

```rust
let decision: Option<ApprovalDecision> = ctx.wait_for_event("approval", Duration::from_days(3)).await?;
match decision {
    Some(d) if d.approved => { /* proceed */ }
    _ => { /* timed out or rejected, compensate */ }
}
```

The `key` participates in the workflow signature. Renaming a wait key is a breaking change.

#### Parallel steps

```rust
let (payment, inventory) = ctx.parallel()
    .step("charge", || async { charge_card(order_id).await })
    .step("reserve", || async { reserve_stock(order_id).await })
    .run()
    .await?;
```

#### Dispatch

Dispatch from mutations or daemons (requires `transactional` on mutations):
```rust
#[forge::mutation(transactional)]
pub async fn start_onboarding(ctx: &MutationContext, input: Input) -> Result<Uuid> {
    ctx.start_workflow("onboarding", json!({"user_id": input.user_id})).await
}
```

### Daemons

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

- `leader_elected = true`: cluster-wide singleton via `pg_try_advisory_lock`. Auto failover on disconnect.
- `leader_elected = false`: one instance per node (use for metrics, health monitoring).
- Daemons can dispatch jobs/workflows (no auth context, owner = None).

### Boundary Validation

Validate at the handler boundary, not deep in helpers:
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

## 2. Authentication & Authorization

### Config

```toml
# HS256 (self-issued JWT)
[auth]
jwt_algorithm = "HS256"
jwt_secret = "${JWT_SECRET}"
access_token_ttl = "1h"
refresh_token_ttl = "30d"

# RS256 (external provider)
[auth]
jwt_algorithm = "RS256"
jwks_url = "https://provider.com/.well-known/jwks.json"
jwt_issuer = "https://provider.com/"
jwt_audience = "my-app"
```

### Self-Issued Auth (HS256)

```rust
#[forge::mutation(public)]
pub async fn register(ctx: &MutationContext, input: AuthInput) -> Result<AuthResponse> {
    let hash = bcrypt::hash(&input.password, bcrypt::DEFAULT_COST)
        .map_err(|e| ForgeError::Internal(e.to_string()))?;
    let user: User = sqlx::query_as!(User, "INSERT INTO users ...").fetch_one(ctx.pool()).await?;

    let pair = ctx.issue_token_pair(user.id, &["user"]).await?;
    Ok(AuthResponse {
        access_token: pair.access_token,
        refresh_token: pair.refresh_token,
        user: user.into(),  // User -> PublicUser (omit password_hash)
    })
}

#[forge::mutation(public)]
pub async fn refresh(ctx: &MutationContext, input: RefreshInput) -> Result<RefreshResponse> {
    let pair = ctx.rotate_refresh_token(&input.refresh_token).await?;
    Ok(RefreshResponse { access_token: pair.access_token, refresh_token: pair.refresh_token })
}

#[forge::mutation]
pub async fn logout(ctx: &MutationContext, input: LogoutInput) -> Result<()> {
    ctx.revoke_refresh_token(&input.refresh_token).await
}
```

Token methods (MutationContext, HMAC only): `issue_token_pair()`, `rotate_refresh_token()`, `revoke_refresh_token()`, `revoke_all_refresh_tokens()`.

### Access Control

- All queries/mutations require auth by default.
- `#[forge::mutation(public)]` for unauthenticated access.
- `require_role("admin")` on handlers for role-based access.

### AuthContext Methods

```rust
ctx.auth.require_user_id() -> Result<Uuid>
ctx.auth.require_role("admin") -> Result<()>  // returns Forbidden
ctx.auth.has_role("admin") -> bool
ctx.auth.subject() -> Option<&str>
ctx.auth.tenant_id() -> Option<Uuid>
ctx.auth.claim("org_id") -> Option<&Value>
ctx.auth.roles() -> &[String]
```

### Identity Scope Enforcement

Input args with recognized identity keys (`user_id`, `owner_id`, `subject`, etc.) are validated at runtime against the authenticated principal. Admins bypass scope checks. Do not add redundant manual checks.

### Claims Builder

```rust
let claims = Claims::builder()
    .user_id(uuid)
    .role("admin")
    .claim("org_id", json!("org-123"))
    .duration_secs(3600)
    .build()?;
```

### OAuth 2.1 for MCP

```toml
[mcp]
enabled = true
oauth = true
```

Forge acts as OAuth 2.1 Authorization Server with PKCE. Endpoints: `/.well-known/oauth-authorization-server`, `/_api/oauth/authorize`, `/_api/oauth/token`, `/_api/oauth/register`.

## 3. Integrations

### File Uploads

```rust
#[forge::mutation]
pub async fn upload_avatar(ctx: &MutationContext, user_id: Uuid, file: Upload) -> Result<String> {
    let bytes = file.bytes();
    let name = file.name();
    let content_type = file.content_type();
    // Store bytes...
    Ok(url)
}
```

Limits: 10 MB per file. For larger files, use presigned URLs (mutation returns URL, client uploads directly to storage, client confirms).

### Webhooks

```rust
#[forge::webhook(
    path = "/hooks/stripe",
    signature = WebhookSignature::hmac_sha256("Stripe-Signature", "STRIPE_WEBHOOK_SECRET"),
    idempotency = "header:Stripe-Idempotency-Key",
    timeout = "30s"
)]
pub async fn stripe(ctx: &WebhookContext, payload: Value) -> Result<WebhookResult> {
    ctx.dispatch_job("process_payment", payload.clone()).await?;
    Ok(WebhookResult::Accepted)
}
```

Webhooks skip JWT auth (use signature verification). Return `Ok` (200), `Accepted` (202), or `Custom { status_code, body }`.

### MCP Tools

```rust
#[forge::mcp_tool(
    title = "Export Data",
    description = "Export project data as CSV",
    require_role("admin"),
    read_only,
    rate_limit(requests = 10, per = "1m", key = "user")
)]
pub async fn export_data(ctx: &McpToolContext, project_id: Uuid) -> Result<String> {
    let data = sqlx::query_as!(Row, "SELECT * FROM data WHERE project_id = $1", project_id)
        .fetch_all(&*ctx.db()).await?;
    Ok(format_csv(&data))
}
```

Annotations: `read_only`, `destructive`, `idempotent`, `open_world`. Use `#[schemars(...)]` on params for JSON Schema metadata. McpToolContext has no HTTP client -- dispatch jobs for external calls.

### External APIs

```rust
// Circuit-breaker-backed (default: 5 failures -> open, 30s backoff)
let response = ctx.http().post(url).json(&body).send().await?;

// Raw reqwest escape hatch
let response = ctx.raw_http().post(url).json(&body).send().await?;
```

### Custom HTTP Routes

```rust
use forge::prelude::axum::{Router, routing::get, Json};

let custom = Router::new()
    .route("/healthz", get(|| async { Json(json!({"ok": true})) }));

Forge::builder().config(config).custom_routes(custom).build()?.run().await
```

Custom routes bypass Forge middleware (no JWT, no rate limiting, no CORS). Do not use `/_api` prefix.

## 4. Testing

### Test Location

Tests live inline with the code they prove:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use forge::testing::*;

    #[tokio::test]
    async fn test_create_order() { ... }
}
```

### Test Contexts

Every handler type has a matching test context with builder pattern:

```rust
// Query
let ctx = TestQueryContext::builder()
    .as_user(Uuid::new_v4())
    .with_role("admin")
    .with_pool(pool)
    .build();

// Mutation (adds HTTP mocking, job/workflow dispatch)
let ctx = TestMutationContext::builder()
    .as_user(uuid)
    .with_pool(pool)
    .mock_http_json("api.stripe.com/*", json!({"id": "ch_123"}))
    .build();

// Job
let ctx = TestJobContext::builder("send_email")
    .as_retry(3)
    .with_cancellation_requested()
    .with_pool(pool)
    .build();

// Cron
let ctx = TestCronContext::builder("daily_cleanup")
    .as_catch_up()
    .build();

// Workflow
let ctx = TestWorkflowContext::builder("onboarding")
    .with_completed_step("step_a", json!({...}))
    .build();

// Daemon
let ctx = TestDaemonContext::builder("heartbeat").build();

// MCP Tool
let ctx = TestMcpToolContext::builder("export_data")
    .as_user(uuid)
    .with_role("admin")
    .build();
```

All builders support: `as_user()`, `as_subject()`, `with_role()`, `with_claim()`, `with_pool()`, `with_env()`.

### HTTP Mocking

```rust
.mock_http("api.stripe.com/*", |req| {
    if req.body["amount"].as_i64().unwrap() > 10000 {
        MockResponse::error(400, "Amount exceeds limit")
    } else {
        MockResponse::json(json!({"id": "ch_123"}))
    }
})
```

Verification:
```rust
ctx.http().assert_called("api.stripe.com/*");
ctx.http().assert_called_times("api.stripe.com/*", 1);
ctx.http().assert_not_called("api.paypal.com/*");
ctx.http().assert_called_with_body("api.stripe.com/*", |body| body["amount"] == 1000);
```

### Assertion Macros

```rust
assert_ok!(result);
assert_err!(result);
assert_err_variant!(result, ForgeError::NotFound(_));

assert_job_dispatched!(ctx, "send_email");
assert_job_dispatched!(ctx, "send_email", |args| args["to"] == "x");
assert_job_not_dispatched!(ctx, "send_sms");

assert_workflow_started!(ctx, "onboarding");
assert_http_called!(ctx, "api.stripe.com/*");
```

### Database Testing

```rust
let db = IsolatedTestDb::setup("test_name", &forge::get_internal_sql(), Path::new("migrations")).await?;
let ctx = TestMutationContext::builder().as_user(uuid).with_pool(db.pool().clone()).build();
db.cleanup().await?;  // optional, orphans GC'd on next run
```

### Testing Auth

```rust
// UUID auth with roles
let ctx = TestQueryContext::builder().as_user(Uuid::new_v4()).with_role("admin").build();

// Non-UUID auth (Firebase, Clerk)
let ctx = TestMutationContext::builder().as_subject("firebase-uid-abc123").build();

// Unauthenticated
let ctx = TestQueryContext::minimal();
```

### Testing Daemon Shutdown

```rust
#[tokio::test]
async fn daemon_shuts_down_gracefully() {
    let ctx = TestDaemonContext::builder("queue_processor").build();
    let ctx_clone = ctx.clone();
    let handle = tokio::spawn(async move { queue_processor(&ctx_clone).await });
    tokio::time::sleep(Duration::from_millis(100)).await;
    ctx.request_shutdown();
    let result = tokio::time::timeout(Duration::from_secs(5), handle).await;
    assert!(result.is_ok());
}
```

### Execution Order

1. Backend unit tests (`cargo test`)
2. `forge generate` if contract changed
3. Frontend lint/build checks
4. Playwright (`forge test --skip-backend`)
5. `forge check`

## 5. Operations & Deployment

### Deploy

Single binary: `cargo build --release`. Frontend embedded via `embedded-frontend` feature flag. Migrations loaded from filesystem at runtime -- copy `migrations/` into the image.

Health endpoints: `/_api/health` (liveness, 200 always), `/_api/ready` (readiness, checks DB + reactor).

Graceful shutdown on SIGTERM: stop accepting, drain in-flight (30s), release leadership, close connections. Set `terminationGracePeriodSeconds: 45` in Kubernetes.

### Scaling

All coordination through PostgreSQL (no separate service mesh):
- Leader election: `pg_try_advisory_lock`
- Job claiming: `FOR UPDATE SKIP LOCKED`
- Node roles: `gateway`, `function`, `worker`, `scheduler`

Worker pools route jobs to specialized workers:
```rust
#[forge::job(worker_capability = "gpu")]
```
```toml
[node]
roles = ["worker"]
worker_capabilities = ["gpu", "general"]
```

### Read Replicas

```toml
[database]
replica_urls = ["postgres://replica1...", "postgres://replica2..."]
read_from_replica = true
```

Use `#[forge::query(consistent)]` for read-after-write paths. Dashboards and analytics are replica-safe.

### Pool Isolation

```toml
[database.pools.default]
size = 30
[database.pools.jobs]
size = 10
[database.pools.analytics]
size = 5
statement_timeout_secs = 600
```

### Observability

```toml
[observability]
enabled = true
otlp_endpoint = "http://localhost:4318"  # HTTP only, not gRPC
sampling_ratio = 1.0
log_level = "info"
```

Key metrics: `http_requests_total`, `http_request_duration_seconds`, `job_executions_total`, `job_duration_seconds`, `active_connections`.

Trace correlation: every request gets `request_id` and `trace_id` via `ctx.request`. Pass to external calls with headers.

Structured logging:
```rust
tracing::info!(job_id = %ctx.job_id, attempt = ctx.attempt, "Processing");
```

Quiet routes (exclude health checks from logs):
```toml
[gateway]
quiet_routes = ["/_api/health", "/_api/ready"]
```

### Monitoring Queries

```sql
-- Job queue depth
SELECT status, count(*) FROM forge_jobs GROUP BY status;

-- Stuck jobs
SELECT id, job_type, started_at, now() - started_at as duration
FROM forge_jobs WHERE status = 'running' AND started_at < now() - interval '30 minutes';

-- Workflow runs
SELECT status, count(*) FROM forge_workflow_runs GROUP BY status;
```

### Operational Checklist

- `DATABASE_URL` set (not embedded PG in production)
- `JWT_SECRET` rotated and not in source control
- Health endpoints accessible from load balancer
- Pool isolation for mixed workloads
- `quiet_routes` excludes health endpoints
- `RUST_LOG=info` (not `debug` in production)
- Migrations tested with rollback (`forge migrate down`)
- Rate limits on public endpoints
- Circuit breaker enabled for external API calls
