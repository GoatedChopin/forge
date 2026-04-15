# Patterns Reference

This reference documents recommended architectural patterns for backend logic, authentication, system integrations, and testing.

## 1. Backend Design Patterns

### Using `DbConn` for Shared Logic
Extract shared logic into utility functions that accept `DbConn<'_>` to allow your code to be used across queries, mutations, and tests without duplication.

```rust
pub async fn list_active_items(db: DbConn<'_>) -> Result<Vec<Item>> {
    db.fetch_all(sqlx::query_as!(Item, "SELECT * FROM items WHERE status = 'active'"))
      .await
      .map_err(Into::into)
}
// Access via ctx.db_conn() in Queries or ctx.db() in Mutations.
```

### Background Job Implementation
Background jobs are durable, retryable tasks managed by the framework.

```rust
#[forge::job(priority = "high", retry(max_attempts = 5, backoff = "exponential"), timeout = "30m")]
pub async fn process_video(ctx: &JobContext, args: Args) -> Result<Res> {
    ctx.progress(0, "Initializing...")?; // Non-blocking status update
    ctx.check_cancelled().await?; // Safe exit if the job was cancelled
    ctx.heartbeat().await?; // Extend lease to prevent another worker from reclaiming
    Ok(res)
}
```

- **Dispatch within Transactions**: Always use `ctx.dispatch_job` inside a `transactional` mutation to ensure the job is only queued if the database transaction commits successfully.
- **Idempotency**: Use `idempotent(key = "...")` to prevent duplicate processing of the same entity.
- **Lease Reclaim**: Jobs that fail to send a heartbeat within 5 minutes are automatically reclaimed by the framework.

### Scheduled Tasks (Crons)
Forge crons use advisory locks to ensure they execute exactly once across the cluster.

```rust
#[forge::cron("0 */6 * * *", catch_up)]
pub async fn sync_external_data(ctx: &CronContext) -> Result<()> {
    if ctx.is_late() {
        // Log or adjust logic if the task is behind schedule.
    }
    Ok(())
}
```

### Durable Multi-Step Workflows
Workflows are versioned to ensure long-running processes (lasting days or months) can complete even after application updates.

```rust
#[forge::workflow(name = "onboarding", version = "2026-05", timeout = "30d")]
pub async fn onboarding_wf(ctx: &WorkflowContext, user_id: Uuid) -> Result<()> {
    let result = ctx.step("welcome_email", || async { send_email(user_id).await })
        .timeout(Duration::from_secs(30))
        .retry(3, Duration::from_secs(5))
        .compensate(|id| async move { rollback_action(id).await })
        .run().await?;

    ctx.sleep(Duration::from_secs(24 * 60 * 60)).await?; // Survives system restarts
    ctx.wait_for_event("profile_completed", Some(Duration::from_secs(3 * 24 * 60 * 60))).await?;
    Ok(())
}
```

- **Workflow Rules**: Step results are cached by name. Always bump the workflow version if you rename, reorder, or remove steps to avoid state corruption.
- **Readiness Checks**: The `/_api/ready` endpoint will return 503 if existing workflow runs lack a matching version handler in the current binary.

## 2. Authentication and Authorization

### Social Login (OAuth Bridge Pattern)
Always exchange OAuth codes on the server to prevent exposing provider secrets or tokens to the browser.

1. **Frontend**: Obtains a code from the provider (Google, GitHub, etc.) and sends it to a public mutation.
2. **Backend**: Exchanges the code for provider tokens using `ctx.http().post()`.
3. **Identity Mapping**: Fetch the provider's user info, find the matching record in `user_identities`, and link to a `user_id`.
4. **Token Issuance**: Generate a Forge JWT using `ctx.issue_token_pair(user_id, roles)`. **IMPORTANT:** Always drop your database connection (`ctx.conn()`) before calling this to prevent pool exhaustion, as token issuance requires its own connection.

| Provider | Token Exchange Endpoint | User Info Endpoint |
|---|---|---|
| Google | `oauth2.googleapis.com/token` | `googleapis.com/oauth2/v2/userinfo` |
| GitHub | `github.com/login/oauth/access_token` | `api.github.com/user` |

### Authorization Utilities
- **Role Enforcement**: Use `ctx.auth.require_role("admin")` to immediately return a 403 Forbidden error if the principal lacks the necessary role.
- **Principal Access**: Use `ctx.user_id()` as a shortcut for the current user's UUID.
- **Scope Checks**: Build-time checks ensure all private queries filter by `user_id` or `owner_id` unless explicitly marked `unscoped`.

## 3. Integrations

### Webhook Handling
- **Signature Verification**: Always configure a `signature` check for webhooks to verify the source. Choose the right constructor for your provider — see the full table in [API Reference](./api.md#forge::webhook). Never use `allow_unsigned` in production.
- **Idempotency**: Configure `idempotency` to prevent double-processing on retries. Use `"header:webhook-id"` when the provider supplies a delivery ID header, otherwise use `"body:$.id"` to pull the event ID from the payload.
- **Asynchronous Processing**: Immediately return `WebhookResult::Accepted` and dispatch a background job for any work that takes more than a few hundred milliseconds. Webhook senders have short timeout windows and will retry on slow responses.
- **Race Conditions**: Payment webhooks and checkout confirmation can arrive in any order. Use `COALESCE($1, column)` in updates so a slow webhook doesn't overwrite data that a faster path already set correctly.

#### Provider Quick-Reference

| Provider | Constructor | Idempotency |
|---|---|---|
| Polar / Svix / Clerk | `standard_webhooks("ENV")` | `"header:webhook-id"` |
| Stripe | `stripe_webhooks("ENV")` | `"header:stripe-request-id"` |
| Shopify | `shopify_webhooks("ENV")` | `"body:$.id"` |
| GitHub | `hmac_sha256("X-Hub-Signature-256", "ENV")` | `"header:X-GitHub-Delivery"` |
| Ed25519-based services | `ed25519("X-Signature", "PUBLIC_KEY_ENV")` | varies |

### MCP (Model Context Protocol) Tools
- **Read-Only vs Destructive**: Annotate tools with their intended behavior to help AI models select the correct tool.
- **Authorization**: MCP tools require authentication by default. Use `require_role` to restrict access to specific agents.

## 4. Testing

All tests use `#[tokio::test]`. Unit tests live inline in `#[cfg(test)] mod tests {}`. DB integration tests are gated behind the `testcontainers` feature flag.

### Context Builders

Each handler type has a test context builder. All builders support `.with_pool(db)`, `.with_env("KEY", "value")`, and `.mock_http(mock)`.

```rust
// Query — read-only, auth-aware
let ctx = TestQueryContext::builder()
    .as_user(user_id)
    .with_role("admin")
    .with_pool(db.pool())
    .build();

// Mutation — transactional, can dispatch jobs/workflows
let ctx = TestMutationContext::builder()
    .as_user(user_id)
    .with_pool(db.pool())
    .mock_http(MockHttpClient::new().expect_post("https://api.example.com/send", json!({"ok": true})))
    .build();

// Job — progress, cancellation, heartbeat
let ctx = TestJobContext::builder()
    .with_pool(db.pool())
    .build();

// Cron — minimal, just pool + env
let ctx = TestCronContext::builder().with_pool(db.pool()).build();
```

### Isolated Test Database

`IsolatedTestDb` creates a fresh database, runs all migrations, and tears down on drop. Required for any test that touches the DB.

```rust
#[cfg(feature = "testcontainers")]
mod tests {
    use forge::testing::IsolatedTestDb;
    use std::path::Path;

    #[tokio::test]
    async fn test_creates_user() {
        let db = IsolatedTestDb::setup("test_creates_user", "", Path::new("migrations")).await.unwrap();
        let ctx = TestMutationContext::builder().with_pool(db.pool()).build();
        let result = CreateUserMutation::execute(&ctx, Args { name: "Alice".into() }).await;
        assert_ok!(result);
    }
}
```

### Assertion Macros

| Macro | What it checks |
|---|---|
| `assert_ok!(result)` | Result is `Ok(_)` |
| `assert_err!(result)` | Result is `Err(_)` |
| `assert_err_variant!(result, ForgeError::NotFound(_))` | Result is a specific error variant |
| `assert_job_dispatched!(ctx, "job_name")` | A job was queued during the mutation |
| `assert_workflow_started!(ctx, "workflow_name")` | A workflow was started during the mutation |
| `assert_http_called!(ctx, "POST", "https://...")` | An HTTP call was made via `ctx.http()` |

### Testing HTTP Dependencies

Use `.mock_http(pattern, handler)` on the context builder. `pattern` is a URL glob. Use `.mock_http_json(pattern, json_value)` as a shorthand when the response body is static.

```rust
let ctx = TestMutationContext::builder()
    .mock_http_json("https://oauth2.googleapis.com/token", json!({ "access_token": "tok" }))
    .mock_http_json("https://googleapis.com/oauth2/v2/userinfo", json!({ "id": "g123", "email": "a@b.com" }))
    .build();
```

### Testing Error Paths

Test failure modes explicitly — they are just as important as the happy path.

```rust
#[tokio::test]
async fn returns_not_found_for_missing_item() {
    let db = IsolatedTestDb::setup("test_not_found", "", Path::new("migrations")).await.unwrap();
    let ctx = TestQueryContext::builder().as_user(Uuid::new_v4()).with_pool(db.pool()).build();
    let result = GetItemQuery::execute(&ctx, Args { id: Uuid::new_v4() }).await;
    assert_err_variant!(result, ForgeError::NotFound(_));
}
```

## 5. Operational Readiness

- **Isolated Test DB**: Use `IsolatedTestDb` in your test suite to ensure each test case runs against a clean, migrated database instance.
- **Read Replicas**: Use `#[query(consistent)]` to force a read from the primary database when eventual consistency is unacceptable.
- **Observability**: Enable OTLP exports in `forge.toml` to send traces and metrics to your telemetry collector.
- **Signal Correlation**: Forge automatically propagates `x-correlation-id` headers across RPC calls to simplify debugging across distributed logs.
