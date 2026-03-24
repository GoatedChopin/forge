# Integrations

File uploads, webhooks, MCP tools, custom routes, and external APIs.

## File Uploads

Backend: add `Upload` parameter to a mutation.

```rust
#[forge::mutation]
pub async fn upload_avatar(ctx: &MutationContext, user_id: Uuid, file: Upload) -> Result<String> {
    let bytes = file.bytes();        // &Bytes
    let name = file.name();          // &str (filename)
    let content_type = file.content_type();  // &str
    let size = file.len();           // usize

    // Store bytes...
    Ok(url)
}
```

Frontend: pass `File` from `<input>` directly. The generated client auto-detects `File`/`Blob` and routes to `/_api/rpc/{fn}/upload` with multipart/form-data.

Additional `Upload` methods: `into_bytes()` (consumes and returns `Bytes`), `is_empty()`. Use `Vec<Upload>` for batch uploads, `Option<Upload>` for optional files. Upload serializes as base64 for JSON compatibility. Never store `Upload` directly in the database.

Limits: 10 MB per file (`MAX_UPLOAD_SIZE`), 20 fields max, 1 MB max JSON field, 255 char max field name.

For files > 10 MB, use presigned URLs:
1. Mutation returns a presigned upload URL
2. Client uploads directly to storage
3. Client calls a confirm mutation with the storage key

## Webhooks

```rust
#[forge::webhook(
    path = "/hooks/stripe",
    signature = WebhookSignature::hmac_sha256("Stripe-Signature", "STRIPE_WEBHOOK_SECRET"),
    idempotency = "header:Stripe-Idempotency-Key",
    timeout = "30s"
)]
pub async fn stripe(ctx: &WebhookContext, payload: Value) -> Result<WebhookResult> {
    ctx.dispatch_job("process_payment", payload.clone()).await?;
    Ok(WebhookResult::Accepted)  // 202
}
```

- Webhooks mount under `/_api/webhooks` (path is appended)
- `WebhookResult::Ok` (200), `Accepted` (202), `Custom { status_code, body }`
- Signature algorithms: `hmac_sha256` (prefix `sha256=`), `hmac_sha1` (`sha1=`), `hmac_sha512` (`sha512=`)
- Idempotency: `"header:X-Id"` or `"body:$.id"` (JSONPath dot notation). 24h TTL.
- `allow_unsigned` flag skips signature check
- Webhooks skip JWT auth (use signature verification instead)
- Context has `dispatch_job` and `cancel_job` but no `start_workflow`

## MCP Tools

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
        .fetch_all(&*ctx.db())
        .await?;
    Ok(format_csv(&data))
}
```

Config in `forge.toml`:
```toml
[mcp]
enabled = true
path = "/mcp"
session_ttl_secs = 3600
```

Annotation flags: `read_only`, `destructive`, `idempotent`, `open_world`. These set MCP tool hints for client UIs.

### Parameter Metadata

Use `#[schemars(...)]` on the generated Args struct's fields (applied via function parameters) for rich JSON Schema output:

```rust
#[forge::mcp_tool(
    title = "Search Projects",
    description = "Search projects by name or tag",
    read_only
)]
pub async fn search_projects(
    ctx: &McpToolContext,
    /// The search query string
    #[schemars(description = "Search query (min 2 chars)", min_length = 2)]
    query: String,
    #[schemars(description = "Maximum results to return")]
    #[serde(default = "default_limit")]
    limit: Option<u32>,
) -> Result<Vec<Project>> { ... }

fn default_limit() -> Option<u32> { Some(20) }
```

The `#[schemars(...)]` and `#[serde(...)]` attributes are preserved on the generated `SearchProjectsParams` struct. `schemars` generates the JSON Schema that MCP clients use for input validation and UI generation. Doc comments on parameters also become schema descriptions.

McpToolContext has no HTTP client. Dispatch jobs for external API work.
MCP tools are authenticated by default. Only mark a tool `public` when the requirement is explicit, and use `require_role("...")` for sensitive tools.

## Custom HTTP Routes

```rust
use forge::prelude::axum::{Router, routing::get, Json};

let custom = Router::new()
    .route("/healthz", get(|| async { Json(json!({"ok": true})) }))
    .route("/metrics", get(metrics_handler));

Forge::builder()
    .config(config)
    .custom_routes(custom)  // consumes self, chain it
    .build()?
    .run()
    .await
```

Route mounting order: `/_api/*` (Forge internals) → `/_api/webhooks/*` (webhook handlers) → custom routes → frontend fallback (SPA catch-all). Custom routes sit outside `/_api`, after webhooks but before the frontend fallback.

Custom routes bypass Forge's entire middleware stack: no JWT auth, no rate limiting, no CORS, no request tracing, no timeouts. Handle these yourself if needed.

- Do not use `/_api` prefix (reserved, causes runtime panic)
- `custom_routes()` takes `self` by value (builder pattern, must chain)

## External APIs

Use `ctx.http()` for circuit-breaker-backed requests. Reach for `ctx.raw_http()` only when you intentionally need bare `reqwest`.

Circuit breaker defaults: 5 failures → open, 30s initial backoff, 1.5x multiplier, 10min max, 2 successes to close from half-open. Tracks per host.
If the handler declares an explicit `timeout`, that timeout also becomes the default outbound HTTP timeout for `ctx.http()` unless the request overrides it.

```rust
let response = ctx.http().post(url).json(&body).send().await?;

// Raw reqwest escape hatch
let response = ctx.raw_http().post(url).json(&body).send().await?;
```

## OAuth Integration

Option A (external JWKS): configure `jwks_url` + `jwt_issuer` + `jwt_audience` in `forge.toml`. Forge validates tokens from the external provider directly.

Option B (self-issued): verify provider token in a public mutation, then issue a Forge JWT via `ctx.issue_token(claims)`.

## Push Notifications / Email

Use jobs with retry for unreliable external services:
```rust
#[forge::job(retry(max_attempts = 5, backoff = "exponential"))]
pub async fn send_email(ctx: &JobContext, args: EmailArgs) -> Result<()> {
    ctx.http().post("https://api.sendgrid.com/v3/mail/send")
        .bearer_auth(ctx.env_require("SENDGRID_KEY")?)
        .json(&build_email(&args))
        .send()
        .await?;
    Ok(())
}
```

## Decision Matrix

| Scenario | Use |
|---|---|
| File < 10 MB | `Upload` type in mutation |
| File > 10 MB | Presigned URL pattern |
| External webhook | `#[forge::webhook]` with signature |
| AI tool | `#[forge::mcp_tool]` with hints |
| Custom HTTP semantics | `custom_routes()` with Axum |
| Unreliable external API | Job with retry + circuit breaker |
| External auth provider | JWKS config or self-issued JWT |
