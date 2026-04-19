# API Reference

Macros, contexts, config, errors, CLI.

## Macro Attributes

### `#[forge::query]`
Read-only operation. Generates `{PascalCase}Query` + `ForgeQuery` impl. Private queries must filter by the principal unless `unscoped` is set.

| Attribute | Description and Rationale |
|---|---|
| `public` | Disables authentication requirements for the query. |
| `consistent` | Forces the query to read from the primary database to ensure data consistency after a recent write. |
| `require_role("x")` | Returns a 403 Forbidden error if the user lacks the specified role. |
| `cache = "30s"` | Enables a per-identity cache with the specified TTL to reduce database load. |
| `timeout = 30` | Sets the maximum execution time in seconds. This also sets the default timeout for `ctx.http()`. |
| `rate_limit(...)` | Configures rate limiting based on `requests`, `per` duration, and a specified `key` (e.g., user, IP, or global). |
| `log = "info"` | Sets the log level for handler execution. |
| `unscoped` | Skips mandatory scope enforcement checks at compile time. |
| `tables = [...]` | Manually specifies table dependencies to trigger reactive cache invalidation. |

### `#[forge::mutation]`
Data-modifying operation. Generates `{PascalCase}Mutation` + `ForgeMutation` impl.

| Attribute | Description and Rationale |
|---|---|
| `public` | Allows unauthenticated access to the mutation. |
| `require_role("x")` | Restricts access to users with the specified role. |
| `transactional` | Wraps the entire operation in a PostgreSQL transaction. This is required if you use `dispatch_job()` or `start_workflow()`. |
| `timeout = 30` | Sets the handler timeout in seconds. |
| `max_size = "200mb"` | Defines the maximum allowable request body size for this mutation. |
| `rate_limit(...)` | Configures rate limiting for the mutation. |
| `unscoped` | Disables compile-time scope validation. |

### `#[forge::job]`
Durable background task with automatic retry. Dispatched via `ctx.dispatch_job()` from a `transactional` mutation.

| Attribute | Description and Rationale |
|---|---|
| `name = "x"` | Overrides the default job name. |
| `timeout = "1h"` | Sets the maximum execution duration. **Default: `"1h"`**. |
| `priority = "normal"` | Priority level. Values: `background`(0), `low`(25), `normal`(50), `high`(75), `critical`(100). **Default: `"normal"`**. |
| `retry(max_attempts = 3, backoff = "exponential")` | Retry config. `backoff` accepts `"exponential"`, `"linear"`, or `"fixed"`. **Defaults: `max_attempts = 3`, `backoff = "exponential"`**. |
| `worker_capability` | Specifies a capability string required by the worker node to execute this job. |
| `idempotent` | Marks the job as idempotency-aware. Pair with `key = "args.field_name"` to name a uniqueness key (a path into the args struct). Bare `idempotent` without a `key` marks intent but does not deduplicate — call `dispatcher.dispatch_idempotent::<J>(key, args)` from the runtime dispatcher when you need actual dedup. |
| `ttl = "24h"` | Defines how long the job record persists in the database after completion. |
| `compensate = "fn"` | Specifies a cleanup function to run if the job ultimately fails after all retries. |

### `#[forge::cron("0 9 * * *")]`
Exactly-once scheduled task (cluster-wide via advisory lock + UNIQUE constraint).

| Attribute | Description and Rationale |
|---|---|
| `timezone = "UTC"` | Sets the schedule's timezone. |
| `group = "default"` | Groups crons for concurrency management. |
| `timeout = "1h"` | Sets the maximum allowed execution time. |
| `catch_up` | Executes missed intervals if the system was offline. **Default limit: 10 catch-up executions**. |

### `#[forge::workflow]`
Durable, multi-step process. Versioned so in-flight runs complete even when code changes. Step results cached by name; renaming a step breaks resume.

| Attribute | Description and Rationale |
|---|---|
| `name = "x"` | Provides a logical ID shared across different versions of the workflow. |
| `version = "..."` | A unique version string. Changes to steps require a version bump. |
| `active` | Marks this version as the one responsible for handling new workflow runs. |
| `deprecated` | Marks the version as inactive; it will only finish existing runs. |
| `timeout = "24h"` | Sets the maximum time a workflow run is allowed to execute. |

### `#[forge::webhook]`
`POST /webhooks/{path}` endpoint for external events. The macro deserialises the body into your parameter type — declare a typed struct, not `serde_json::Value`.

| Attribute | Description and Rationale |
|---|---|
| `path = "/webhooks/stripe"` | The URL path this webhook listens on. Must start with `/`. |
| `signature = WebhookSignature::...` | Configures signature verification. Omitting this attribute causes the handler to reject all requests unless `allow_unsigned` is set. |
| `allow_unsigned` | Accept requests with no signature. Only use this during local development or for sources that cannot sign requests. |
| `idempotency = "header:X-Id"` | Extracts a deduplication key from the given header. Use `"body:$.id"` to extract from the request body via JSONPath. |
| `timeout = "30s"` | Sets the handler timeout. Also applies to `ctx.http()` calls within the handler. |

#### Signature Constructors

`WebhookSignature` (in `forge::prelude::*`). Each constructor sets algorithm + header + env var holding the secret.

| Constructor | Algorithm | Notes |
|---|---|---|
| `WebhookSignature::hmac_sha256("Header", "ENV")` | HMAC-SHA256, hex-encoded | GitHub, most generic providers |
| `WebhookSignature::hmac_sha1("Header", "ENV")` | HMAC-SHA1, hex-encoded | Legacy GitHub |
| `WebhookSignature::hmac_sha512("Header", "ENV")` | HMAC-SHA512, hex-encoded | Uncommon |
| `WebhookSignature::standard_webhooks("ENV")` | HMAC-SHA256, base64, `{id}\n{ts}\n{body}` | Polar, Svix, Clerk — header always `webhook-signature` |
| `WebhookSignature::stripe_webhooks("ENV")` | HMAC-SHA256, hex, `{ts}.{body}`, 5-min replay guard | Stripe — header always `Stripe-Signature` |
| `WebhookSignature::shopify_webhooks("ENV")` | HMAC-SHA256, base64-encoded | Shopify — header always `X-Shopify-Hmac-Sha256` |
| `WebhookSignature::ed25519("Header", "ENV")` | Ed25519 asymmetric verification | For services that publish a public key instead of a shared secret |

For `ed25519`, the `ENV` variable holds a **base64-encoded Ed25519 public key** (32 bytes), not a shared secret.

```rust
// Polar / Standard Webhooks
#[forge::webhook(
    path = "/webhooks/polar",
    signature = WebhookSignature::standard_webhooks("POLAR_WEBHOOK_SECRET"),
    idempotency = "header:webhook-id"
)]
pub async fn polar_webhook(ctx: &WebhookContext, payload: Value) -> Result<WebhookResult> { ... }

// Stripe
#[forge::webhook(
    path = "/webhooks/stripe",
    signature = WebhookSignature::stripe_webhooks("STRIPE_WEBHOOK_SECRET"),
    idempotency = "header:stripe-request-id"
)]
pub async fn stripe_webhook(ctx: &WebhookContext, payload: Value) -> Result<WebhookResult> { ... }

// Shopify
#[forge::webhook(
    path = "/webhooks/shopify",
    signature = WebhookSignature::shopify_webhooks("SHOPIFY_WEBHOOK_SECRET"),
    idempotency = "body:$.id"
)]
pub async fn shopify_webhook(ctx: &WebhookContext, payload: Value) -> Result<WebhookResult> { ... }

// Ed25519 (e.g., a service that publishes a public key)
#[forge::webhook(
    path = "/webhooks/custom",
    signature = WebhookSignature::ed25519("X-Webhook-Signature", "WEBHOOK_PUBLIC_KEY")
)]
pub async fn custom_webhook(ctx: &WebhookContext, payload: Value) -> Result<WebhookResult> { ... }
```

## Environment Variables

Use the context, not `std::env::var()` (mockable in tests, fails fast on boot).

| Method | Behavior |
|---|---|
| `ctx.env_require("KEY")` | Value or `ForgeError::Config`. Required secrets. |
| `ctx.env_or("KEY", "default")` | Value or fallback string. Optional config. |

## HTTP Client

`ctx.http()` — circuit-breaker-wrapped `reqwest`, default timeout matches the handler's `timeout`. Use this for RPC-style calls so breaker + tracing work.

`ctx.raw_http()` — bare `reqwest::Client`. Reach for it when you need streaming (`.bytes_stream()`), custom redirect policies, or other features the wrapper hides.

```rust
let resp: MyResponse = ctx.http()
    .post("https://api.example.com/action")
    .json(&payload)
    .send().await
    .map_err(|e| ForgeError::Internal(e.to_string()))?
    .json().await
    .map_err(|e| ForgeError::Deserialization(e.to_string()))?;
```

## `forge.toml` Key Configuration

`forge check` validates the full schema. Most-tuned keys below — everything else defaults sensibly.

```toml
[auth]
# jwt_secret is required for issue_token_pair(). TTLs default to 1h / 30d if omitted.
jwt_secret = "${JWT_SECRET}"
access_token_ttl = "15m"         # optional, default "1h"
refresh_token_ttl = "7d"         # optional, default "30d"

[database]
url = "${DATABASE_URL}"
pool_size = 50                   # default 50
min_pool_size = 0                # default 0

[gateway]
max_body_size = "20mb"        # total multipart body cap (default)
max_file_size = "10mb"        # per-file cap when mutation has no max_size (default)

[worker]
max_concurrent_jobs = 50         # default 50; parallel job slots per node
poll_interval_ms = 100           # default 100
job_timeout_secs = 3600          # default 1h

[gateway]
port = 9081                      # HTTP port, default 9081
max_body_size = "20mb"           # default; applies to all requests incl. uploads
max_connections = 4096           # max concurrent HTTP connections

[observability]
enabled = false                  # default off
otlp_endpoint = "http://localhost:4318"

[signals]
enabled = true                   # default; set false to disable analytics
anonymize_ip = false
```

### Upload Size Limits

`gateway.max_body_size` caps the total HTTP body. `gateway.max_file_size` caps any single file when the target mutation does not declare its own `max_size`. When a mutation sets `max_size = "200mb"`, that value becomes both the total and per-file limit for that endpoint (explicit opt-in). Validation requires `max_file_size <= max_body_size`.

## Custom Axum Routes

`ForgeBuilder::custom_routes(|pool| Router)` registers additional HTTP routes that inherit the gateway's middleware stack. The factory runs once during `run()` after the pool is connected.

```rust
builder.custom_routes(|pool| {
    Router::new()
        .route("/export/csv", get(csv_export))
        .with_state(Arc::new(pool))
})
```

- Factory receives `sqlx::PgPool`. Ignore it with `|_|` if not needed.
- Returned router is merged into the gateway's `/_api` namespace, so `/export/csv` is reachable at `/_api/export/csv`.
- Full middleware applies automatically: JWT auth, CORS, tracing, concurrency limits, request timeouts.
- Handlers read `Extension<AuthContext>` to access the authenticated user. Unauthenticated requests still arrive with an unauthenticated context — check `auth.user_id()` if login is required.
- Avoid paths that conflict with built-ins: `/health`, `/ready`, `/rpc`, `/rpc/*`, `/events`, `/subscribe`, `/unsubscribe`, `/subscribe-job`, `/subscribe-workflow`, `/signal/*`, `/webhooks/*`, `/mcp`, `/oauth/*`. Conflicts panic at startup.

### Pool Routing

Isolated pools so jobs and analytics never starve web traffic.

| Pool | Used by |
|---|---|
| `default` | Queries, mutations, crons, webhooks |
| `jobs` | Job worker polling and execution |
| `observability` | OTLP metric writes |
| `analytics` | Signals / `forge_signals_events` writes |

Size each pool under `[database.pools.<name>]` (e.g. `[database.pools.jobs]`). Omitted sections fall back to the default pool size.

## Duration Formats
Most time attributes accept `500ms`, `30s`, `5m`, `2h`, `7d`, or a bare number (seconds). **Exception**: `timeout` on `query`, `mutation`, and `mcp_tool` is a bare `u64` (seconds) only — `"30s"` fails to parse.

## Context Capability Matrix

Each handler type receives a typed context providing framework services.

| Feature | Query | Mut | Job | Cron | WF | Dmn | Web | MCP |
|---|---|---|---|---|---|---|---|---|
| `db()` | yes | yes | yes | yes | yes | yes | yes | yes |
| `db_conn()` | yes | yes | yes | yes | yes | yes | yes | yes |
| `conn()` (txn-aware) | — | yes | yes | yes | yes | yes | yes | yes |
| `http()` / `raw_http()` | — | yes | yes | yes | yes | yes | yes | — |
| `auth` (Session info) | yes | yes | yes | yes | yes | — | — | yes |
| `dispatch_job` | — | yes | — | — | — | yes | yes | yes |
| `start_workflow` | — | yes | — | — | — | yes | yes | yes |
| `issue_token_pair` | — | yes | — | — | — | — | — | — |
| `step()` / `sleep()` / `wait_for_event()` | — | — | — | — | yes | — | — | — |
| `heartbeat()` / `save()` / `progress()` | — | — | yes | — | — | yes* | — | — |
| `check_cancelled()` | — | — | yes | — | — | — | — | — |
| `shutdown_signal()` | — | — | — | — | — | yes | — | — |
| `is_late()` | — | — | — | yes | — | — | — | — |
| `EnvAccess` (`env_require` / `env_or`) | yes | yes | yes | yes | yes | yes | yes | yes |

\* Daemons have `heartbeat()` but not `progress()` / `save()`.

### Database Access — Three Shapes

Picking the wrong handle is the most common mistake.

| Call | Returns | Use for | sqlx calling convention |
|---|---|---|---|
| `ctx.db()` | `ForgeDb` (queries) / `DbConn<'_>` (mutations) | Reads or non-transactional writes (queries, jobs, crons, daemons, webhooks) | `query.fetch_*(ctx.db())` — `ForgeDb` implements `sqlx::Executor` |
| `ctx.conn().await?` | `ForgeConn<'_>` | Transactional reads and writes in mutations | `query.fetch_*(&mut conn)` — bind to `let mut conn = ...` first |
| `ctx.db_conn()` | `DbConn<'_>` | Shared helpers that must work in both query and mutation contexts | `db.fetch_*(query)` — **inverted**: call `.fetch_*` on the `DbConn`, passing the query as the argument |

```rust
// Query: ForgeDb + standard sqlx convention
#[forge::query]
pub async fn get_user(ctx: &QueryContext, id: Uuid) -> Result<Option<User>> {
    sqlx::query_as!(User, "SELECT * FROM users WHERE id = $1", id)
        .fetch_optional(ctx.db())
        .await
        .map_err(Into::into)
}

// Mutation: ForgeConn inside a transaction
#[forge::mutation(transactional)]
pub async fn create_user(ctx: &MutationContext, email: String) -> Result<User> {
    let mut conn = ctx.conn().await?;
    sqlx::query_as!(User, "INSERT INTO users (email) VALUES ($1) RETURNING *", email)
        .fetch_one(&mut conn)
        .await
        .map_err(Into::into)
}

// Shared helper: DbConn with inverted convention
pub async fn find_user(db: DbConn<'_>, id: Uuid) -> Result<Option<User>> {
    db.fetch_optional(
        sqlx::query_as!(User, "SELECT * FROM users WHERE id = $1", id)
    ).await.map_err(Into::into)
}
// Called as: find_user(ctx.db_conn(), id).await
```

### Extra Context Notes
- `sqlx::Error` auto-converts to `ForgeError::Sql` via `?`; explicit `.map_err(Into::into)` is idiomatic but optional.
- Use `ctx.user_id()?` in private handlers to pull the principal UUID.
- `ctx.auth.require_role("admin")` short-circuits with 403.
- Workflow dispatch buffers into an outbox and flushes after transaction commit.

## ForgeError Variants

Use variant → HTTP + internal code. Frontend branches on the internal code; never on `message` (locale/wording drift).

| Variant | HTTP | Code | Use for |
|---|---|---|---|
| `NotFound` | 404 | `NOT_FOUND` | Missing resource. |
| `Unauthorized` | 401 | `UNAUTHORIZED` | Missing/invalid auth. |
| `Forbidden` | 403 | `FORBIDDEN` | Lacks permission. Reserved for permissions — do **not** use for billing/plan state (triggers frontend logout). |
| `Validation` | 400 | `VALIDATION_ERROR` | Malformed input. |
| `InvalidArgument` | 400 | `INVALID_ARGUMENT` | Business-rule rejection (use for "upgrade required" etc.). |
| `Timeout` | 504 | `TIMEOUT` | Exceeded handler timeout. |
| `RateLimitExceeded` | 429 | `RATE_LIMITED` | Throttled. `details.retry_after_secs` populated. |
| `Internal` | 500 | `INTERNAL_ERROR` | Unhandled server error. |

## CLI Command Reference

| Command | Purpose |
|---|---|
| `forge new <name>` | Scaffold a project from a template. Pins `[package] version = "1.0.0"` so it doesn't inherit the framework's workspace version. |
| `forge generate` | Sync backend to frontend bindings. Detects target via `svelte.config.js` or `Dioxus.toml`. |
| `forge check` | fmt + clippy + sqlx cache + schema + **system-table-write rule** + bindings. |
| `forge migrate <up\|down\|status\|prepare>` | Manage DB migrations. Advisory-locked for cluster safety. |
| `forge test` | cargo test → docker compose → Playwright. |

## Project File Standards
- Editable code: `src/functions/`, `src/schema/`, `src/utils/`.
- Generated code: `frontend/src/lib/forge/` (Svelte) or `frontend/src/forge/` (Dioxus). **Never edit.** See [Pitfalls](./pitfalls.md#1-generated-code).
- Migrations: SQL files in `migrations/` with `-- @up` / `-- @down` markers. No `IF NOT EXISTS` — migrations must be deterministic.
