# Common Failure Scenarios

Load when builds fail, runtime errors appear, or the agent is going in circles on compilation issues. These are real failures encountered during Forge development with known fixes.

## Backend Compilation

### `SQLX_OFFLINE=true` but no cached data

```
error: `SQLX_OFFLINE=true` but there is no cached data for this query
```

**Cause**: `sqlx::query_as!` (bang macro) requires either a live database connection or a pre-built `.sqlx/` cache directory. New queries won't have cache entries.

**Fix**: Use the runtime function form instead of the macro:
```rust
// won't compile without DB or cache
sqlx::query_as!(User, "SELECT * FROM users WHERE id = $1", id)

// compiles without DB, validates at runtime
sqlx::query_as::<_, User>("SELECT id, name, email FROM users WHERE id = $1")
    .bind(id)
    .fetch_one(ctx.db())
    .await
```

After the app runs against a real DB, run `cargo sqlx prepare` to populate the cache for CI.

### Generated structs not found in `functions::`

```
error[E0425]: cannot find type `GetCounterQuery` in module `functions`
```

**Cause**: Forge macros generate structs with the same visibility as the annotated function. Private functions produce private structs that `pub use module::*` won't re-export.

**Fix**: Make handler functions `pub`:
```rust
// generated struct is private, invisible outside module
#[forge::query(public)]
async fn get_counter(ctx: &QueryContext) -> Result<Counter> { ... }

// generated struct is pub, re-exported through mod.rs
#[forge::query(public)]
pub async fn get_counter(ctx: &QueryContext) -> Result<Counter> { ... }
```

### `ForgeConn` doesn't implement `Executor`

```
error[E0277]: the trait `Executor<'_>` is not implemented for `ForgeConn<'_>`
    but it is implemented for `&mut ForgeConn<'_>`
```

**Cause**: `ctx.conn().await?` returns an owned `ForgeConn`. sqlx query methods need `&mut ForgeConn`.

**Fix**: Bind to a mutable variable first:
```rust
// fails: temporary ForgeConn dropped
sqlx::query_as::<_, User>("...")
    .fetch_one(ctx.conn().await?)

// works: bound to mutable local
let mut conn = ctx.conn().await?;
sqlx::query_as::<_, User>("...")
    .fetch_one(&mut conn)
```

Note: `QueryContext.db()` returns `ForgeDb` which works directly without `&mut`. Only `MutationContext.conn()` needs this pattern.

## Docker / forge dev

### `project root does not exist`

```
backend-1  | error: project root does not exist
backend-1 exited with code 1
```

**Cause**: Scaffolded `docker-compose.yml` from `forge new` contains hardcoded paths that assume the project lives inside the Forge workspace:
```yaml
working_dir: /workspace/examples/with-dioxus/minimal  # wrong for standalone
volumes:
  - ../../..:/workspace  # wrong for standalone
```

**Fix**: Update `docker-compose.yml` to mount the project root directly:
```yaml
working_dir: /app
volumes:
  - .:/app
  - cargo_cache:/usr/local/cargo/registry
  - ./target:/app/target
```

Always check `docker-compose.yml` after `forge new`. The `working_dir` and `volumes` paths are the usual culprits.

### Target directory in Docker volume vs bind mount

Docker volumes (`target_cache:/app/target`) are faster but hide build artifacts from the host. Bind mounts (`./target:/app/target`) are slower on macOS but let the host IDE see compiled artifacts for code intelligence.

For development, prefer bind mount (`./target:/app/target`). For CI, prefer named volume.

## Frontend General

### `ForgeError` cannot be formatted with `{}`

```
error[E0277]: `ForgeError` cannot be formatted with the default formatter
```

**Cause**: `ForgeError` implements `Debug` but not `Display`. Dioxus RSX `"{error}"` uses `Display`.

**Fix**: Use debug format:
```rust
// fails
p { "{error}" }

// works
p { "{error:?}" }

// better: extract the message field
p { "{error.message}" }
```

## Auth + SSE

### `SESSION_PRINCIPAL_MISMATCH`

```
ForgeError { code: "SESSION_PRINCIPAL_MISMATCH", message: "Request principal does not match session principal" }
```

**Cause**: The SSE session was established with one identity (e.g. anonymous) and a subsequent subscribe request uses a different identity (e.g. authenticated user). This happens when a `ForgeClient` is created before login and reused after login. The server binds each SSE session to a principal on first connection and rejects requests from a different principal.

The check is in `sse.rs`:
```rust
fn same_principal(a: &AuthContext, b: &AuthContext) -> bool {
    match (a.is_authenticated(), b.is_authenticated()) {
        (false, false) => true,
        (true, true) => a.principal_id().is_some() && a.principal_id() == b.principal_id(),
        _ => false,  // one authed, one not = mismatch
    }
}
```

**Fix**: The `ForgeClient` (and its SSE session) must be destroyed and recreated when the auth identity changes. Use `ForgeAuthProvider` with `use_auth_key()` to force a full remount on login/logout:

```rust
fn App() -> Element {
    rsx! {
        ForgeAuthProvider {
            url: API_URL,
            app_name: "my-app",
            AppShell {}
        }
    }
}

fn AppShell() -> Element {
    // Key changes on login/logout, forcing full remount and fresh SSE session
    let auth_key = use_auth_key();
    rsx! { main { key: "{auth_key}", Router::<Route> {} } }
}
```

`ForgeAuthProvider` handles token wiring, 401 recovery, and periodic refresh automatically. `use_auth_key()` returns a key that increments on every `login`/`login_with_viewer`/`logout` call, forcing a full component remount that recreates the SSE connection with the new identity.

Do NOT create a single `ForgeClient` at the app root and toggle its token. The SSE session is bound to the principal at connection time and cannot be re-bound.

### SSE token expiry causes unexpected logout

```
WARN forge_runtime::gateway::sse: SSE token validation failed: Token expired
WARN forge_runtime::gateway::server: Request rejected duration_ms=0 http.status_code=401
```

**Cause**: The access token expires (e.g. after 15 minutes). SSE reconnects with the expired token and gets a 401. If `with_auth_error_handler` calls `clear_auth` directly, the keyed remount fires (key changes from user ID to `"anon"`), the component tree unmounts (destroying the refresh timer), and the user is logged out before the proactive refresh timer ever fires.

**Fix**: Two things must be true:

1. **Auth error handler must trigger a refresh, not a logout.** Use a `needs_refresh` signal that a `use_effect` watches, then call `try_refresh`. Only log out if refresh fails. See `references/frontend/dioxus.md` "Auth Error Handler Must Refresh, Not Logout" for the full pattern.

2. **Proactive refresh timer must fire well before token expiry.** Rule of thumb: refresh at ~2/3 of the token lifetime. For a 15-minute token, use a 10-minute timer. A 12-minute timer with a 15-minute token leaves only 3 minutes of buffer, which is too tight if the timer drifts or the refresh request is slow.

### Refresh mutation fails with `REQUEST_FAILED` / empty response body

```
ForgeClientError { code: "REQUEST_FAILED", message: "EOF while parsing a value at line 1 column 0", details: None }
```

**Cause**: The `refresh` mutation is `(public)`, but the `ForgeClient` still sends the expired access token in the `Authorization` header. The Forge runtime validates the token before checking if the endpoint is public. An expired token produces a 401 with an empty or non-JSON body, which the client fails to deserialize.

**Fix**: Use an anonymous `ForgeClient` (no token provider) for the refresh call:

```rust
pub async fn try_refresh(api_url: &str, auth_signal: &mut Signal<AuthState>) {
    let refresh_token = match &*auth_signal.read() {
        AuthState::Authenticated(stored) => stored.refresh_token.clone(),
        _ => return,
    };

    let anon_client = ForgeClient::new(ForgeClientConfig::new(api_url.to_string()));
    let input = RefreshInput::new(refresh_token);
    match refresh(&anon_client, input).await {
        Ok(resp) => { /* update stored auth */ }
        Err(_) => { clear_auth(auth_signal); }
    }
}
```

Do NOT reuse the authenticated `ForgeClient` for refresh calls. The expired token in the header will cause the request to fail even though the endpoint doesn't require auth.

### `#[serde(skip)]` fields still appear in generated frontend types

```
ForgeClientError { code: "DESERIALIZATION_ERROR", message: "missing field `password_hash`" }
```

**Cause**: `forge generate` scans Rust struct fields directly. Serde attributes like `#[serde(skip)]` or `#[serde(skip_serializing)]` are invisible to the code generator. The field appears in the generated TypeScript/Dioxus type, but the backend omits it from JSON, causing deserialization failure.

**Fix**: Create a separate public-facing type without the sensitive field:

```rust
// Internal, used for DB queries
#[derive(Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    #[serde(skip, default)]
    pub password_hash: String,
}

// Public, used in API responses — this is what forge generate sees
#[derive(Serialize, Deserialize)]
pub struct PublicUser {
    pub id: Uuid,
    pub email: String,
}

impl From<User> for PublicUser { ... }
```

Use `PublicUser` in all handler return types (`AuthResponse`, `me` query, etc). The code generator picks up the return types and only generates frontend types for structs that appear in the API surface.

## Formatting / forge check

`forge check` runs `cargo fmt`, `clippy`, and `prettier`. If it fails on formatting alone, run the fixers before investigating further:
```bash
cargo fmt
cd frontend && bun run format  # or npm run format
```

Then re-run `forge check`. Don't waste time reading formatting diffs.
