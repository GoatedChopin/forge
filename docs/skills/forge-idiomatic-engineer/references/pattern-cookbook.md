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

### DbConn calling conventions

`MutationContext::db()` returns `DbConn<'_>` (transaction-aware), not `&PgPool`. The calling pattern differs from `QueryContext`:

```rust
// QueryContext: sqlx query builder chained directly, ctx.db() acts like &PgPool
sqlx::query_as::<_, Todo>("SELECT * FROM todos WHERE user_id = $1")
    .bind(uid)
    .fetch_all(ctx.db())
    .await?;

// MutationContext: DbConn has its own fetch methods, pass sqlx query as argument
ctx.db()
    .fetch_one(
        sqlx::query_as::<_, Todo>("INSERT INTO todos ... RETURNING *")
            .bind(uid)
            .bind(&title),
    )
    .await?;
```

`DbConn` methods: `fetch_one`, `fetch_optional`, `fetch_all`, `execute`. Use these when working with `MutationContext::db()`. For `QueryContext`, you can chain `.fetch_all(ctx.db())` directly on the sqlx query builder.

## 2) Self-issued auth (register + login)

Use `ctx.issue_token()` with `Claims::builder()` for HMAC-based auth. No extra JWT dependencies needed.

```rust
use forge::forge_core::Claims;

#[forge::mutation(public)]
pub async fn register(ctx: &MutationContext, input: AuthInput) -> Result<AuthResponse> {
    let hash = bcrypt::hash(&input.password, 12)
        .map_err(|e| ForgeError::Internal(format!("hash error: {e}")))?;

    let user: User = ctx.db()
        .fetch_one(
            sqlx::query_as::<_, User>(
                "INSERT INTO users (username, password_hash) VALUES ($1, $2) RETURNING *"
            )
            .bind(&input.username)
            .bind(&hash),
        )
        .await
        .map_err(|e| {
            if e.to_string().contains("unique") {
                ForgeError::Validation("Username already taken".into())
            } else {
                ForgeError::Internal(format!("database error: {e}"))
            }
        })?;

    let claims = Claims::builder()
        .user_id(user.id)
        .duration_secs(7 * 24 * 3600)
        .build()
        .map_err(|e| ForgeError::Internal(e))?;

    Ok(AuthResponse {
        token: ctx.issue_token(&claims)?,
        user_id: user.id,
        username: user.username,
    })
}
```

## 3) Boundary validation function

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

The router validates `input.user_id` matches the JWT subject before the handler runs. No manual comparison needed — just use `ctx.require_user_id()?` for the verified ID.

```rust
// src/schema/order.rs — type definitions live here
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct ListOrdersInput {
    pub user_id: uuid::Uuid,  // router validates this matches JWT sub
}

// src/functions/orders.rs — handler imports from schema
use crate::schema::order::ListOrdersInput;

#[forge::query(tables = ["orders"])]
pub async fn list_orders(ctx: &QueryContext, _input: ListOrdersInput) -> Result<Vec<Order>> {
    let me = ctx.require_user_id()?;

    sqlx::query_as("SELECT * FROM orders WHERE user_id = $1 ORDER BY created_at DESC")
        .bind(me)
        .fetch_all(ctx.db())
        .await
        .map_err(Into::into)
}
```

## 4) Transactional mutation + outbox side effects

```rust
// src/schema/invoice.rs
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct CreateInvoiceInput {
    pub user_id: uuid::Uuid,
    pub total_cents: i64,
}

// src/functions/invoices.rs
use crate::schema::invoice::CreateInvoiceInput;

#[forge::mutation(transactional)]
pub async fn create_invoice(ctx: &MutationContext, input: CreateInvoiceInput) -> Result<Invoice> {
    let me = ctx.require_user_id()?;

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

## 14) Custom HTTP handlers

For endpoints that don't fit the query/mutation model (OAuth callbacks, file serving, custom health checks), mount Axum routes directly via `builder.custom_routes()`.

### Basic handler

```rust
use forge::prelude::*;
use forge::prelude::axum::{Router, routing::{get, post}, response::IntoResponse, extract::State, Json};
use std::sync::Arc;

struct AppState {
    pool: sqlx::PgPool,
}

async fn custom_health(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let row: (i64,) = sqlx::query_as("SELECT 1")
        .fetch_one(&state.pool)
        .await
        .unwrap_or((0,));

    Json(serde_json::json!({ "db": row.0 == 1 }))
}

// In main.rs, after building the pool:
let state = Arc::new(AppState { pool: pool.clone() });
let custom = Router::new()
    .route("/healthz", get(custom_health))
    .with_state(state);

builder.custom_routes(custom);
```

### Where custom routes mount

```
/_api/                    # Reserved for Forge internals
  ├── /rpc                # RPC endpoint
  ├── /rpc/{fn}/upload    # Multipart uploads
  ├── /subscribe, /events # SSE
  └── /webhooks/*         # Webhook handlers
{custom_routes}           # YOUR ROUTES (merged here)
{frontend_fallback}       # SPA catch-all (last)
```

Custom routes sit outside `/_api`, after webhooks but before the frontend fallback. They take precedence over the SPA catch-all.

### What custom routes bypass

Custom routes do not get Forge's standard middleware stack. You handle these yourself if needed:

- Authentication (no JWT validation)
- Rate limiting
- CORS
- Request tracing/logging
- Timeouts

### OAuth callback example

```rust
use axum::{extract::Query, response::Redirect};

#[derive(Deserialize)]
struct OAuthCallback {
    code: String,
    state: String,
}

async fn google_callback(
    State(state): State<Arc<AppState>>,
    Query(params): Query<OAuthCallback>,
) -> Result<Redirect, StatusCode> {
    // Exchange code for tokens with Google
    let token_resp = reqwest::Client::new()
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("code", params.code.as_str()),
            ("client_id", &std::env::var("GOOGLE_CLIENT_ID").unwrap()),
            ("client_secret", &std::env::var("GOOGLE_CLIENT_SECRET").unwrap()),
            ("redirect_uri", "http://localhost:3000/auth/google/callback"),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;

    // Verify and extract identity, upsert user, issue Forge JWT...
    // Redirect to frontend with token
    Ok(Redirect::to(&format!("/login?token={}", forge_token)))
}

let custom = Router::new()
    .route("/auth/google/callback", get(google_callback))
    .with_state(state);

builder.custom_routes(custom);
```

### File serving example

```rust
use axum::{extract::Path, body::Body, http::header};

async fn serve_file(
    State(state): State<Arc<AppState>>,
    Path(key): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    // Fetch from S3/R2/GCS using the key
    let bytes = fetch_from_storage(&key)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    Ok((
        [(header::CONTENT_TYPE, "application/octet-stream")],
        Body::from(bytes),
    ))
}

let custom = Router::new()
    .route("/files/{key}", get(serve_file))
    .with_state(state);
```

## 15) Daemon with leader election

Long-running background processes that stay alive for the lifetime of the node. Use for connection pools to external systems, queue consumers, or continuous sync loops.

```rust
#[forge::daemon(leader_elected = true)]
pub async fn sync_external_feed(ctx: &DaemonContext) -> Result<()> {
    loop {
        if ctx.is_shutdown_requested() {
            break;
        }

        match fetch_and_process_feed(ctx.db()).await {
            Ok(count) => {
                tracing::info!(processed = count, "Feed sync complete");
            }
            Err(e) => {
                tracing::error!(error = %e, "Feed sync failed");
            }
        }

        // Wait before next iteration, but respect shutdown
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_secs(60)) => {}
            _ = ctx.shutdown_signal() => break,
        }
    }

    Ok(())
}
```

Key attributes:
- `leader_elected = true`: only one instance runs across the cluster (via PG advisory locks)
- `restart_on_panic = true`: auto-restart if the daemon panics
- `restart_delay = "5s"`: delay between restarts
- `max_restarts = 10`: stop restarting after this many failures

Always check `ctx.is_shutdown_requested()` or `ctx.shutdown_signal()` in loops to allow graceful shutdown.

## 16) Frontend auth store with SSE reconnection

Every Forge app with auth needs a reactive store that persists credentials to localStorage and reconnects SSE when auth state changes. This uses Svelte 5 runes in a class-based pattern.

```typescript
// src/lib/auth.svelte.ts
import { getContext, setContext } from "svelte"

const AUTH_KEY = Symbol("auth")

export class AuthStore {
  token = $state<string | null>(null)
  userId = $state<string | null>(null)
  username = $state<string | null>(null)

  isAuthenticated = $derived(this.token !== null)

  getToken(): string | null {
    return this.token
  }

  setAuth(data: { token: string; user_id: string; username: string }) {
    this.token = data.token
    this.userId = data.user_id
    this.username = data.username

    localStorage.setItem("auth_token", data.token)
    localStorage.setItem("auth_user_id", data.user_id)
    localStorage.setItem("auth_username", data.username)

    // reconnect SSE so subscriptions pick up the new identity
    window.dispatchEvent(new Event("forge:reconnect"))
  }

  logout() {
    this.token = null
    this.userId = null
    this.username = null

    localStorage.removeItem("auth_token")
    localStorage.removeItem("auth_user_id")
    localStorage.removeItem("auth_username")

    window.dispatchEvent(new Event("forge:reconnect"))
  }

  hydrate() {
    this.token = localStorage.getItem("auth_token")
    this.userId = localStorage.getItem("auth_user_id")
    this.username = localStorage.getItem("auth_username")
  }
}

export function createAuthStore(): AuthStore {
  const store = new AuthStore()
  setContext(AUTH_KEY, store)
  return store
}

export function getAuthStore(): AuthStore {
  return getContext<AuthStore>(AUTH_KEY)
}
```

Wire it into the root layout so every page can access it via context:

```svelte
<!-- +layout.svelte -->
<script lang="ts">
  import { ForgeProvider } from "$lib/forge"
  import { PUBLIC_API_URL } from "$env/static/public"
  import { createAuthStore } from "$lib/auth.svelte"

  let { children } = $props()

  const auth = createAuthStore()
  auth.hydrate()
</script>

<ForgeProvider url={PUBLIC_API_URL} getToken={() => auth.getToken()}>
  {@render children()}
</ForgeProvider>
```

Pages access the store with `getAuthStore()` and call `setAuth()` after login/register or `logout()` to clear credentials. The `forge:reconnect` event tells ForgeProvider to tear down and re-establish the SSE connection with updated (or cleared) auth headers.
