# Forge-Specific Defaults

Apply these automatically unless the user explicitly overrides.

## Auth and Scope

- Default to secure mode. Configure `[auth]` in `forge.toml` with HS256 self-signed JWT unless the user explicitly asks for no auth or a public-only app.
- Create public `register` + `login` mutation pair. Do not use seed migrations for sample accounts.
- After backend is running, verify the auth flow end-to-end via shell: call `register` then `login` via `curl` against the running API. Use the developer's OS username (`whoami`) as the sample username with a simple default password. Print the credentials to the console.
- Auth frontend is required when auth is configured: build a login page and a register page. Keep styling minimal and functional with high-contrast colors for readability. Do not over-invest in styling on first iteration, get the basics right.
- **Playwright integration tests for auth are non-negotiable.** Write and run tests covering: register a new account, log in, verify authenticated state, and log out. Auth is not complete until these pass.
- All queries and mutations default to authenticated (`ctx.require_user_id()?`) unless `public` is intentionally justified and the user explicitly requests it.
- Prefer `ctx.require_user_id()?` / `ctx.require_subject()?`.
- For user-scoped args, compare to authenticated principal and fail with `Forbidden` on mismatch.
- On the frontend, wire up the generated auth store with localStorage persistence and SSE reconnection on auth state change.

### Token issuance

When HMAC auth is configured (`jwt_algorithm = "HS256"` in `forge.toml`), mutations can issue tokens via `ctx.issue_token()`. Use `Claims::builder()` to construct claims:

```rust
use forge::forge_core::Claims;

#[forge::mutation(public)]
pub async fn register(ctx: &MutationContext, input: AuthInput) -> Result<AuthResponse> {
    // ... create user in DB ...

    let claims = Claims::builder()
        .user_id(user.id)
        .duration_secs(7 * 24 * 3600)
        .build()
        .map_err(|e| ForgeError::Internal(e))?;

    let token = ctx.issue_token(&claims)?;

    Ok(AuthResponse { token, user_id: user.id, username: user.username })
}
```

No extra dependencies needed. Forge uses the same secret from `[auth].jwt_secret` for both validation and issuance. Add `bcrypt` to `Cargo.toml` for password hashing.

For RSA/JWKS auth (external providers), token issuance is not available since Forge doesn't hold the private key. `ctx.issue_token()` returns an error in that case.

### Identity scope enforcement (hard requirement)

Authenticated (non-public) functions with an input parameter **must** include at least one identity or tenant scope argument in their input struct. Without it, Forge returns a runtime error: `"Function '...' must include identity or tenant scope arguments"`.

Identity scope arguments are only required when the function signature includes an input parameter. Functions with no input args are exempt because there are no args to carry scope fields. These functions still require authentication and can access the verified user via `ctx.require_user_id()`.

Recognized identity keys: `user_id`, `userId`, `owner_id`, `ownerId`, `owner_subject`, `ownerSubject`, `subject`, `sub`, `principal_id`, `principalId`.
Recognized tenant keys: `tenant_id`, `tenantId`.

Forge validates that the provided value matches the authenticated JWT subject. This is enforced automatically at the router level for all non-public functions. **Do not add manual identity comparison checks in handlers** — the router already rejects mismatches before the handler runs. Just call `ctx.require_user_id()?` to get the verified ID for use in queries.

```rust
// Every authenticated input struct needs an identity key
#[derive(Debug, Serialize, Deserialize)]
pub struct ListTodosInput {
    pub user_id: Uuid,  // required by Forge scope enforcement
}

// Handler: no manual comparison needed, router already validated input.user_id == JWT sub
#[forge::query]
pub async fn list_todos(ctx: &QueryContext, _input: ListTodosInput) -> Result<Vec<Todo>> {
    let uid = ctx.require_user_id()?;
    // use uid directly in queries — it's guaranteed to match input.user_id
    sqlx::query_as("SELECT * FROM todos WHERE user_id = $1")
        .bind(uid)
        .fetch_all(ctx.db())
        .await
        .map_err(Into::into)
}
```

**Anti-pattern — do NOT do this:**
```rust
// WRONG: redundant check, the router already enforces this
let uid = ctx.require_user_id()?;
if input.user_id != uid {
    return Err(ForgeError::Forbidden("Access denied".into()));
}
```

### Input type derives

All input/arg structs for queries and mutations must derive both `Serialize` and `Deserialize`. Forge requires `Serialize` for type generation and response handling, even though inputs are deserialized from requests.

## No-Input Handlers

- When a query or mutation takes no input args, omit the args parameter entirely.
- The macro auto-generates a unit type when there is no second argument.
- Never use `Option<()>`, `()`, or dummy input structs as a workaround.
- For authenticated functions that only need user identity (no other input), use the no-input form. Identity scope enforcement is skipped automatically when there is no input parameter. The JWT still carries identity, accessible via `ctx.require_user_id()`.

```rust
// Before (unnecessary boilerplate):
#[derive(Debug, Serialize, Deserialize)]
pub struct ListTodosInput {
    pub user_id: Uuid,  // only exists for scope enforcement
}

#[forge::query(tables = ["todos"])]
pub async fn list_todos(ctx: &QueryContext, _input: ListTodosInput) -> Result<Vec<Todo>> {
    let uid = ctx.require_user_id()?;
    // ...
}

// After (clean):
#[forge::query(tables = ["todos"])]
pub async fn list_todos(ctx: &QueryContext) -> Result<Vec<Todo>> {
    let uid = ctx.require_user_id()?;
    // ...
}
```

Only add `user_id` to input structs when the function actually needs user-provided arguments beyond identity.

## Macro Attribute Reference

### Query attributes

```rust
#[forge::query(
    public,                          // no auth required
    require_role("admin"),           // role-based access
    tables = ["orders", "items"],    // table deps for reactive invalidation
    cache = "30s",                   // response cache TTL
    consistent,                      // force primary read (bypass replicas)
    timeout = 10,                    // query timeout in seconds
    rate_limit(requests = 100, per = "1m", key = "user"), // per-user rate limit
    log,                             // enable request logging
)]
```

### Mutation attributes

```rust
#[forge::mutation(
    public,
    require_role("admin"),
    transactional,                   // wrap in DB transaction (default: false)
    timeout = 30,
    rate_limit(requests = 10, per = "1m", key = "ip"),
    log,
)]
```

### Job attributes

```rust
#[forge::job(
    name = "custom_job_name",        // override default name
    priority = "high",               // background | low | normal | high | critical
    timeout = "30m",                 // max execution time
    ttl = "24h",                     // discard if not started within TTL
    retry(max_attempts = 5, backoff = "exponential", max_backoff = "10m"),
    idempotent(key = "order_id"),    // deduplicate by input field
    worker_capability = "gpu",       // route to specific workers
    public,
    require_role("admin"),
)]
```

### Cron attributes

```rust
#[forge::cron(
    "0 9 * * *",                     // cron schedule (required)
    timezone = "America/New_York",
    catch_up,                        // run missed executions
    catch_up_limit = 3,              // max catch-up runs
    timeout = "5m",
    group = "reports",               // logical grouping
)]
```

### Daemon attributes

```rust
#[forge::daemon(
    leader_elected = true,           // single instance across cluster
    restart_on_panic = true,
    restart_delay = "5s",
    startup_delay = "2s",
    max_restarts = 10,
)]
```

### Webhook attributes

```rust
#[forge::webhook(
    path = "/webhooks/stripe",
    signature = WebhookSignature::hmac_sha256("Stripe-Signature", "STRIPE_WEBHOOK_SECRET"),
    // also: hmac_sha1, hmac_sha512
    idempotency = "body:$.id",
    timeout = "30s",
    allow_unsigned,                  // skip signature validation (dev only)
)]
```

### MCP Tool attributes

```rust
#[forge::mcp_tool(
    name = "tickets.list",
    title = "List Tickets",
    description = "Read-only ticket listing",
    require_role("support"),
    read_only,                       // behavioral hint: no side effects
    destructive,                     // behavioral hint: irreversible
    idempotent,                      // behavioral hint: safe to retry
    open_world,                      // behavioral hint: accesses external resources
    timeout = 30,
    rate_limit(requests = 60, per = "1m", key = "user"),
)]
```

### Rate limit key options

- `"user"`: per authenticated user
- `"ip"`: per client IP
- `"tenant"`: per tenant_id claim
- `"global"`: shared across all callers

## Mutation Atomicity

- If mutation dispatches jobs/workflows or has multi-write consistency needs, require `transactional`.

## Read Replica Usage

- Use replica reads only for tolerance to replication lag.
- For read-your-write critical paths, use `#[forge::query(consistent)]` to force primary reads.
- Document consistency assumptions.
- Configure pool isolation (`database.pools.*`) to prevent workload starvation across query/job/analytics boundaries.
- Rely on health-aware replica selection (automatic, 15s ping interval) rather than manual failover logic.

## Observability and Trace Context

- Include stable identifiers (`request_id`, `trace_id`, entity/job/workflow IDs) in logs.
- Propagate trace/request context to external HTTP calls where practical.
- Keep health probes in quiet routes unless explicitly monitored.

## Error Discipline

Map errors precisely:
- `Validation` for boundary input issues
- `Unauthorized`/`Forbidden` for auth/authz
- `NotFound` for missing resources
- keep `Internal` as last resort

## Frontend Defaults (unless backend-only explicitly requested)

- Svelte 5 runes-first (`$state`, `$derived`) with generated reactive Forge stores.
- Treat `$effect` as last resort; prefer explicit events and derived state first.
- Avoid manual refetch anti-patterns; rely on Forge reactivity.
- Accessibility is mandatory: semantics, keyboard, focus states, contrast, reduced-motion support.
- SEO is mandatory: semantic structure, metadata, share previews, crawl-friendly content structure.
- Copy quality is mandatory: clear human copy tailored to domain context; no generic AI-sounding filler.
- Use frontend CLI tooling when available: prefer `bun` if present, run `eslint` and `svelte-check`.
- Keep first-iteration styling minimal and high-contrast. Get the basics right before polishing.
