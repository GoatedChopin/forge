# Integrations

Patterns for external-facing endpoints, file handling, and third-party services.

## Contents

1. [Webhook with signature + idempotency](#1-webhook-with-signature--idempotency)
2. [MCP tool with least privilege](#2-mcp-tool-with-least-privilege)
3. [Custom HTTP handlers (Axum)](#3-custom-http-handlers-axum)
4. [Daemon with leader election](#4-daemon-with-leader-election)
5. [Built-in file uploads](#5-built-in-file-uploads)
6. [External storage (S3, R2, GCS)](#6-external-storage-s3-r2-gcs)
7. [OAuth integration](#7-oauth-integration)
8. [Push notifications](#8-push-notifications)
9. [Decision matrix](#9-decision-matrix)

---

## 1) Webhook with signature + idempotency

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

## 2) MCP tool with least privilege

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

## 3) Custom HTTP handlers (Axum)

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

## 4) Daemon with leader election

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

Always check `ctx.is_shutdown_requested()` or `ctx.shutdown_signal()` in loops for graceful shutdown.

## 5) Built-in file uploads

Forge has native multipart upload handling. No extra crates or middleware needed.

### How it works

1. Frontend sends `multipart/form-data` to `/_api/rpc/{function}/upload`
2. Gateway parses the multipart body, validates sizes, separates files from JSON metadata
3. Mutation receives `Upload` objects in its args alongside regular fields
4. Codegen detects `Upload` fields and generates `callWithFiles()` calls automatically

### Limits

| Constraint | Default |
|---|---|
| Max single file | 10 MB |
| Max total payload | 20 MB |
| Max upload fields | 20 |
| Max field name length | 255 chars |
| Concurrent uploads | 32 |

### Backend: receiving uploads

```rust
use forge::forge_core::Upload;

#[derive(Debug, Serialize, Deserialize)]
pub struct UploadAvatarInput {
    pub user_id: Uuid,
    pub file: Upload,
}

#[forge::mutation]
pub async fn upload_avatar(ctx: &MutationContext, input: UploadAvatarInput) -> Result<AvatarUrl> {
    let uid = ctx.require_user_id()?;
    let bytes = input.file.bytes();
    let content_type = input.file.content_type();

    let storage_url = upload_to_storage(ctx.http(), uid, bytes, content_type).await?;

    let mut conn = ctx.conn().await?;
    let avatar: AvatarUrl = sqlx::query_as!(AvatarUrl, "UPDATE users SET avatar_url = $1 WHERE id = $2 RETURNING avatar_url", &storage_url, uid)
        .fetch_one(&mut *conn)
        .await?;

    Ok(avatar)
}
```

The `Upload` type provides `name()`, `content_type()`, `bytes()`, `into_bytes()`, `len()`, and `is_empty()`. It serializes as base64 for JSON compatibility. Never store `Upload` directly in the database.

### Frontend: sending uploads

Codegen maps `Upload` to `File | Blob` in TypeScript. The generated API uses `callWithFiles()` automatically when it detects upload fields.

```typescript
const file = fileInput.files[0]
const result = await uploadAvatar({ user_id: auth.userId, file })
```

### Multiple file uploads

Use `Vec<Upload>` for batch uploads:

```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct UploadPhotosInput {
    pub user_id: Uuid,
    pub photos: Vec<Upload>,
}

#[forge::mutation]
pub async fn upload_photos(ctx: &MutationContext, input: UploadPhotosInput) -> Result<Vec<PhotoUrl>> {
    let uid = ctx.require_user_id()?;

    let mut urls = Vec::with_capacity(input.photos.len());
    for photo in &input.photos {
        let url = upload_to_storage(ctx.http(), uid, photo.bytes(), photo.content_type()).await?;
        urls.push(url);
    }

    // batch insert into Postgres...

    Ok(urls)
}
```

### Optional uploads

Use `Option<Upload>` when the file is not required:

```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateProfileInput {
    pub user_id: Uuid,
    pub display_name: String,
    pub avatar: Option<Upload>,
}
```

## 6) External storage (S3, R2, GCS)

Forge provides the HTTP client but not a storage abstraction. Use `ctx.http()` for simple cases or add a crate like `aws-sdk-s3` or `object_store` for full SDK support.

### Presigned URL pattern (recommended for large files)

Skip the Forge upload endpoint for large files. Generate a presigned URL in a mutation, let the client upload directly to storage, then confirm via a second mutation.

```rust
#[forge::mutation]
pub async fn request_upload_url(ctx: &MutationContext, input: RequestUploadInput) -> Result<PresignedUpload> {
    let uid = ctx.require_user_id()?;

    let presigned = generate_presigned_url(
        &input.file_name,
        &input.content_type,
        uid,
    ).await?;

    let mut conn = ctx.conn().await?;
    sqlx::query!("INSERT INTO pending_uploads (id, user_id, storage_key, expires_at) VALUES ($1, $2, $3, now() + interval '15 minutes')", presigned.upload_id, uid, &presigned.storage_key)
        .execute(&mut *conn)
        .await?;

    Ok(presigned)
}

#[forge::mutation]
pub async fn confirm_upload(ctx: &MutationContext, input: ConfirmUploadInput) -> Result<FileRecord> {
    let uid = ctx.require_user_id()?;

    let mut conn = ctx.conn().await?;
    let pending: PendingUpload = sqlx::query_as!(PendingUpload, "DELETE FROM pending_uploads WHERE id = $1 AND user_id = $2 RETURNING *", input.upload_id, uid)
        .fetch_one(&mut *conn)
        .await?;

    let file: FileRecord = sqlx::query_as!(FileRecord, "INSERT INTO files (user_id, storage_key, name) VALUES ($1, $2, $3) RETURNING *", uid, &pending.storage_key, &input.file_name)
        .fetch_one(&mut *conn)
        .await?;

    Ok(file)
}
```

Clean up expired uploads with a cron:

```rust
#[forge::cron("*/15 * * * *")]
pub async fn cleanup_expired_uploads(ctx: &CronContext) -> Result<()> {
    sqlx::query!("DELETE FROM pending_uploads WHERE expires_at < now()")
        .execute(ctx.db())
        .await?;
    Ok(())
}
```

## 7) OAuth integration

Forge validates JWTs but doesn't run OAuth flows. Two approaches:

### Option A: External provider issues JWTs, Forge validates

Configure Forge with the provider's JWKS endpoint. No custom auth code needed.

```toml
# forge.toml
[auth]
algorithm = "RS256"
jwks_url = "https://www.googleapis.com/oauth2/v3/certs"
audience = "your-app-id"
issuer = "https://accounts.google.com"
```

Works with Firebase Auth, Auth0, Supabase Auth, Clerk, or any OIDC provider.

### Option B: Forge issues its own JWTs after verifying OAuth tokens

For HMAC mode where Forge controls token issuance:

```rust
#[forge::mutation(public)]
pub async fn login_with_google(ctx: &MutationContext, input: GoogleLoginInput) -> Result<AuthResponse> {
    let google_claims = verify_google_token(ctx.http(), &input.id_token).await?;
    let email = google_claims.email;

    let mut conn = ctx.conn().await?;
    let user: User = sqlx::query_as!(User,
            "INSERT INTO users (email, provider, provider_id) VALUES ($1, 'google', $2)
             ON CONFLICT (provider, provider_id) DO UPDATE SET email = $1
             RETURNING *",
            &email, &google_claims.sub)
        .fetch_one(&mut *conn)
        .await?;

    let claims = Claims::builder()
        .user_id(user.id)
        .duration_secs(7 * 24 * 3600)
        .build()
        .map_err(|e| ForgeError::Internal(e))?;

    Ok(AuthResponse {
        token: ctx.issue_token(&claims)?,
        user_id: user.id,
        email: user.email,
    })
}
```

## 8) Push notifications (FCM, APNs)

No built-in support. Send from mutations or jobs via the HTTP client:

```rust
#[forge::job(retry(max_attempts = 3, backoff = "exponential"))]
pub async fn send_push_notification(ctx: &JobContext, input: PushInput) -> Result<()> {
    ctx.http()
        .post("https://fcm.googleapis.com/v1/projects/my-project/messages:send")
        .bearer_auth(&get_fcm_token().await?)
        .json(&serde_json::json!({
            "message": {
                "token": input.device_token,
                "notification": {
                    "title": input.title,
                    "body": input.body,
                }
            }
        }))
        .send()
        .await?;

    Ok(())
}
```

## 9) Decision matrix

| Need | Approach |
|---|---|
| Files < 10 MB | Built-in `Upload` type through `/_api/rpc/{fn}/upload` |
| Files > 10 MB | Presigned URL pattern, client uploads directly to storage |
| Auth with external provider | JWKS validation (Option A) or token exchange (Option B) |
| Email, SMS, push | Call from jobs with retry for reliability |
| Payment webhooks | `#[forge::webhook]` with signature validation |
| Any external API | `ctx.http()` from mutations/jobs, circuit breaker included |
