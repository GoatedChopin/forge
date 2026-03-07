# External Services and File Uploads

Forge handles execution infrastructure. External services (storage, email, payment, push notifications) are called from within functions using the provided HTTP client.

## Built-in Upload Support

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
    let file_name = input.file.name();

    // Upload to external storage (S3, R2, GCS, etc.)
    let storage_url = upload_to_storage(ctx.http(), uid, bytes, content_type).await?;

    // Store the URL in Postgres, not the file bytes
    let avatar: AvatarUrl = ctx.db()
        .fetch_one(
            sqlx::query_as("UPDATE users SET avatar_url = $1 WHERE id = $2 RETURNING avatar_url")
                .bind(&storage_url)
                .bind(uid),
        )
        .await?;

    Ok(avatar)
}
```

The `Upload` type provides `name()`, `content_type()`, `bytes()`, `into_bytes()`, `len()`, and `is_empty()`. It serializes as base64 for JSON compatibility. Never store `Upload` directly in the database.

### Frontend: sending uploads

Codegen maps `Upload` to `File | Blob` in TypeScript. The generated API uses `callWithFiles()` automatically when it detects upload fields.

```typescript
// Generated API handles multipart automatically
import { uploadAvatar } from "$lib/forge"

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

    // batch insert into Postgres
    // ...

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

## External Storage (S3, R2, GCS)

Forge provides the HTTP client but not a storage abstraction. Use `ctx.http()` for simple cases or add a crate like `aws-sdk-s3` or `object_store` for full SDK support.

### Presigned URL pattern (recommended for large files)

Skip the Forge upload endpoint entirely for large files. Generate a presigned URL in a mutation, let the client upload directly to storage, then confirm via a second mutation.

```rust
#[forge::mutation]
pub async fn request_upload_url(ctx: &MutationContext, input: RequestUploadInput) -> Result<PresignedUpload> {
    let uid = ctx.require_user_id()?;

    // Generate presigned PUT URL (valid 15 minutes)
    let presigned = generate_presigned_url(
        &input.file_name,
        &input.content_type,
        uid,
    ).await?;

    // Track pending upload in Postgres
    ctx.db()
        .execute(
            sqlx::query("INSERT INTO pending_uploads (id, user_id, storage_key, expires_at) VALUES ($1, $2, $3, now() + interval '15 minutes')")
                .bind(presigned.upload_id)
                .bind(uid)
                .bind(&presigned.storage_key),
        )
        .await?;

    Ok(presigned)
}

#[forge::mutation]
pub async fn confirm_upload(ctx: &MutationContext, input: ConfirmUploadInput) -> Result<FileRecord> {
    let uid = ctx.require_user_id()?;

    // Verify the pending upload exists and belongs to this user
    let pending: PendingUpload = ctx.db()
        .fetch_one(
            sqlx::query_as("DELETE FROM pending_uploads WHERE id = $1 AND user_id = $2 RETURNING *")
                .bind(input.upload_id)
                .bind(uid),
        )
        .await?;

    // Move from pending to confirmed in your files table
    let file: FileRecord = ctx.db()
        .fetch_one(
            sqlx::query_as("INSERT INTO files (user_id, storage_key, name) VALUES ($1, $2, $3) RETURNING *")
                .bind(uid)
                .bind(&pending.storage_key)
                .bind(&input.file_name),
        )
        .await?;

    Ok(file)
}
```

Use a cron to clean up expired pending uploads:

```rust
#[forge::cron("*/15 * * * *")]
pub async fn cleanup_expired_uploads(ctx: &CronContext) -> Result<()> {
    sqlx::query("DELETE FROM pending_uploads WHERE expires_at < now()")
        .execute(ctx.db())
        .await?;
    Ok(())
}
```

## Integrating External Auth Providers (OAuth)

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
    // Verify the Google ID token
    let google_claims = verify_google_token(ctx.http(), &input.id_token).await?;
    let email = google_claims.email;

    // Upsert user
    let user: User = ctx.db()
        .fetch_one(
            sqlx::query_as(
                "INSERT INTO users (email, provider, provider_id) VALUES ($1, 'google', $2)
                 ON CONFLICT (provider, provider_id) DO UPDATE SET email = $1
                 RETURNING *"
            )
            .bind(&email)
            .bind(&google_claims.sub),
        )
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

## Custom Axum Routes

For endpoints that don't fit the query/mutation model (OAuth callbacks, file serving, health checks for external services):

```rust
use axum::{Router, routing::get, extract::Path};

let custom = Router::new()
    .route("/auth/google/callback", get(google_oauth_callback))
    .route("/files/:key", get(serve_file));

builder.custom_routes(custom);
```

Custom routes mount outside `/_api`, so they bypass the standard RPC middleware (auth, rate limiting, tracing). Handle auth yourself if needed.

## Push Notifications (FCM, APNs)

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

## When to use what

| Need | Approach |
|---|---|
| Files < 10 MB | Built-in `Upload` type through `/_api/rpc/{fn}/upload` |
| Files > 10 MB | Presigned URL pattern, client uploads directly to storage |
| Auth with external provider | JWKS validation (Option A) or token exchange (Option B) |
| Email, SMS, push | Call from jobs with retry for reliability |
| Payment webhooks | `#[forge::webhook]` with signature validation |
| Any external API | `ctx.http()` from mutations/jobs, circuit breaker included |
