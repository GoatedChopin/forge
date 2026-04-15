# API Reference

This reference provides a comprehensive guide to Forge macros, context types, configuration options, and error variants.

## Macro Attributes

Forge handlers are defined using Rust macros that generate necessary structs and registration logic.

### `#[forge::query]`
Defines a read-only operation. The macro generates a `{PascalCase}Query` struct and implements the `ForgeQuery` trait. All private queries must explicitly filter results by the current user or owner unless the `unscoped` attribute is used.

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
Defines a data-modifying operation. The macro generates a `{PascalCase}Mutation` struct and implements the `ForgeMutation` trait.

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
Defines an asynchronous background task. These tasks are durable and automatically retried upon failure.

| Attribute | Description and Rationale |
|---|---|
| `name = "x"` | Overrides the default job name. |
| `timeout = "1h"` | Sets the maximum execution duration. **Default: `"1h"`**. |
| `priority = "normal"` | Priority level. Values: `background`(0), `low`(25), `normal`(50), `high`(75), `critical`(100). **Default: `"normal"`**. |
| `retry(max_attempts = 3, backoff = "exponential")` | Retry config. `backoff` accepts `"exponential"`, `"linear"`, or `"fixed"`. **Defaults: `max_attempts = 3`, `backoff = "exponential"`**. |
| `worker_capability` | Specifies a capability string required by the worker node to execute this job. |
| `idempotent` | Prevents duplicate job executions. Use `key = "input.id"` to specify the uniqueness key. |
| `ttl = "24h"` | Defines how long the job record persists in the database after completion. |
| `compensate = "fn"` | Specifies a cleanup function to run if the job ultimately fails after all retries. |

### `#[forge::cron("0 9 * * *")]`
Defines a task that runs on a recurring schedule. Execution is guaranteed to happen exactly once across the cluster.

| Attribute | Description and Rationale |
|---|---|
| `timezone = "UTC"` | Sets the schedule's timezone. |
| `group = "default"` | Groups crons for concurrency management. |
| `timeout = "1h"` | Sets the maximum allowed execution time. |
| `catch_up` | Executes missed intervals if the system was offline. **Default limit: 10 catch-up executions**. |

### `#[forge::workflow]`
Defines a durable, multi-step business process. Workflows are versioned to ensure that in-flight runs can complete even if the code changes.

| Attribute | Description and Rationale |
|---|---|
| `name = "x"` | Provides a logical ID shared across different versions of the workflow. |
| `version = "..."` | A unique version string. Changes to steps require a version bump. |
| `active` | Marks this version as the one responsible for handling new workflow runs. |
| `deprecated` | Marks the version as inactive; it will only finish existing runs. |
| `timeout = "24h"` | Sets the maximum time a workflow run is allowed to execute. |

### `#[forge::webhook]`
Defines an HTTP endpoint for receiving events from external services. The handler is registered at `POST /webhooks/{path}`.

| Attribute | Description and Rationale |
|---|---|
| `path = "/webhooks/stripe"` | The URL path this webhook listens on. Must start with `/`. |
| `signature = WebhookSignature::...` | Configures signature verification. Omitting this attribute causes the handler to reject all requests unless `allow_unsigned` is set. |
| `allow_unsigned` | Accept requests with no signature. Only use this during local development or for sources that cannot sign requests. |
| `idempotency = "header:X-Id"` | Extracts a deduplication key from the given header. Use `"body:$.id"` to extract from the request body via JSONPath. |
| `timeout = "30s"` | Sets the handler timeout. Also applies to `ctx.http()` calls within the handler. |

#### Signature Constructors

Use `WebhookSignature` (from `forge::prelude::*`) to configure signature verification. Each constructor sets the algorithm, the header to read the signature from, and the environment variable holding the secret.

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

Use context methods instead of `std::env::var()` — they are mockable in tests and fail fast at startup with a clear error.

| Method | Behavior |
|---|---|
| `ctx.env_require("KEY")` | Returns the value or a `ForgeError::Config` if missing. Use for required secrets. |
| `ctx.env_or("KEY", "default")` | Returns the value or a fallback string. Use for optional config with a sensible default. |

## HTTP Client

`ctx.http()` returns a circuit-breaker-backed `reqwest` client. The default timeout matches the handler's configured `timeout`. Always use this instead of constructing your own client so circuit breaking and tracing work correctly.

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

These are the options most likely to cause silent runtime failures when missing. See the full reference at `docs/docs/ship/configuration.mdx`.

```toml
[auth]
# REQUIRED if issue_token_pair() is used. Missing these causes a panic at startup.
access_token_ttl = "15m"
refresh_token_ttl = "7d"
jwt_secret = "${JWT_SECRET}"

[database]
url = "${DATABASE_URL}"
max_connections = 20          # default pool size

[worker]
concurrency = 10              # parallel job slots per node

[observability]
# Optional. Enables OTLP trace/metric export.
otlp_endpoint = "http://localhost:4318"

[signals]
enabled = true                # default; set false to disable analytics
anonymize_ip = false
```

### Pool Routing

Forge uses isolated connection pools to prevent jobs and analytics from starving web requests.

| Pool | Used by |
|---|---|
| `default` | Queries, mutations, crons, webhooks |
| `jobs` | Job worker polling and execution |
| `observability` | OTLP metric writes |
| `analytics` | Signals / `forge_signals_events` writes |

Each pool can be sized independently under `[database.pools.jobs]`, etc.

## Duration Formats
Time durations can be expressed as `500ms`, `30s`, `5m`, `2h`, `7d`, or a bare number representing seconds. Note that `query`, `mutation`, and `mcp_tool` timeout attributes specifically require a bare `u64` integer representing seconds.

## Context Capability Matrix

Each handler type receives a specific context object providing access to framework services.

| Feature | Query | Mut | Job | Cron | WF | Dmn | Web | MCP |
|---|---|---|---|---|---|---|---|---|
| `db()` (Read access) | yes | — | yes | yes | yes | yes | yes | yes |
| `conn()` (Write access) | — | yes | yes | yes | yes | yes | yes | yes |
| `http()` (Client) | — | yes | yes | yes | yes | yes | yes | — |
| `auth` (Session info) | yes | yes | yes | yes | yes | — | — | yes |
| `dispatch_job` | — | yes | — | — | — | yes | yes | yes |
| `issue_token_pair` | — | yes | — | — | — | — | — | — |
| `step()` / `sleep()` | — | — | — | — | yes | — | — | — |
| `heartbeat()` / `save()` | — | — | yes | — | — | yes | — | — |
| `EnvAccess` | yes | yes | yes | yes | yes | yes | yes | yes |

### Context Usage Notes
- **Database Access**: In mutations, use `let mut conn = ctx.conn().await?` to obtain a transactional connection. Pass `&mut conn` to SQL macros to ensure your queries are part of the transaction.
- **Environment Variables**: Use the `EnvAccess` methods (e.g., `ctx.env_require()`) for all configuration to ensure your code is mockable in tests.
- **HTTP Client**: Use `ctx.http()` for circuit-breaker-backed requests. The default timeout for these requests matches the handler's defined timeout.

## ForgeError Variants

Forge uses structured error variants to ensure consistent error handling across the stack and proper HTTP status code mapping.

| Variant | HTTP Code | Internal Code | Rationale |
|---|---|---|---|
| `NotFound` | 404 | `NOT_FOUND` | Resource does not exist. |
| `Unauthorized` | 401 | `UNAUTHORIZED` | Authentication is missing or invalid. |
| `Forbidden` | 403 | `FORBIDDEN` | User lacks permission for the operation. |
| `Validation` | 400 | `VALIDATION_ERROR` | Request data is malformed or invalid. |
| `Timeout` | 504 | `TIMEOUT` | Operation exceeded its allotted time. |
| `RateLimitExceeded` | 429 | `RATE_LIMITED` | Too many requests from the same identity. |
| `Internal` | 500 | `INTERNAL_ERROR` | Unhandled server-side error. |

## CLI Command Reference

| Command | Purpose |
|---|---|
| `forge new <name>` | Scaffolds a new project from a template. |
| `forge generate` | Synchronizes backend changes with frontend bindings and types. |
| `forge check` | Runs linting, formatting, and validates SQL and bindings. |
| `forge migrate <up|down>`| Manages database migrations. |
| `forge test` | Executes full-stack E2E tests using Playwright. |

## Project File Standards
- **Source Code**: Editable logic resides in `src/functions/`, `src/schema/`, and `src/utils/`.
- **Generated Code**: **MANDATE:** Never edit generated files. See [Pitfalls](./pitfalls.md#1-generated-code).
- **Migrations**: Create new SQL files in `migrations/`. Always include `-- @up` and `-- @down` markers. Do not use `IF NOT EXISTS` clauses; migrations should be deterministic.
