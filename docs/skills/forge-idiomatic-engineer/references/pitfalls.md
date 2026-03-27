# Pitfalls

Common mistakes and their fixes, organized by topic. Load this when starting implementation or when builds fail.

## 1. Generated Code

**Editing generated files.**
Files under `frontend/src/lib/forge/` (SvelteKit) or `frontend/src/forge/` (Dioxus) are overwritten by `forge generate`. Fix the Rust source, not the output.

**Forgetting `forge generate` after backend changes.**
New queries, mutations, models, or enum changes require regeneration. The frontend compiles against stale types and fails at runtime with deserialization errors.

**Putting `#[derive(...)]` before `#[forge::model]`.**
The model macro must be the first attribute. Derives placed before it produce confusing compiler errors.
```rust
// wrong
#[derive(Debug, Clone)]
#[forge::model]
pub struct Item { ... }

// correct
#[forge::model]
#[derive(Debug, Clone)]
pub struct Item { ... }
```

## 2. Macros & Registration

**Naming functions with the macro type suffix.**
`heartbeat_daemon` generates `HeartbeatDaemonDaemon`. Name it `heartbeat` — the macro adds the suffix.

**Private handler functions.**
`#[forge::query] async fn get_items(...)` generates a private struct that `pub use` won't re-export. Always use `pub async fn`.

**Bare `log` flag.**
`#[forge::query(log)]` does not work. Use `#[forge::query(log = "info")]` with a quoted string.

**Dummy `Option<()>` parameters.**
If the handler has no business input, omit the parameter entirely. Dummy inputs trigger unnecessary validation.

**Using `public` by default.**
Auth is required by default for a reason. Only add `public` when the business requirement explicitly calls for unauthenticated access.

**Forgetting to register handlers.**
Macros generate structs but don't make them reachable. Each handler needs a register call in `main.rs` (`.register_query::<GetItemsQuery>()`, etc.) unless `auto_register()` is wired up.

## 3. Database & Transactions

**Calling `dispatch_job`/`start_workflow` without `transactional`.**
Without it, job inserts happen before the mutation commits. If the mutation rolls back, orphan jobs execute against non-existent data.

**Using `ctx.db()` in mutations instead of `ctx.conn()`.**
Mutations need `ctx.conn()` for transactional access. Bind to a mutable variable:
```rust
let mut conn = ctx.conn().await?;
sqlx::query_as::<_, User>("SELECT id, name FROM users WHERE id = $1")
    .bind(id)
    .fetch_one(&mut conn)
    .await
```

**Mocking the database in tests.**
Use `TestQueryContext`/`TestMutationContext` with real DB connections. Mocking hides migration bugs and constraint violations. Use `IsolatedTestDb` for clean per-test databases.

**Hand-writing PG triggers for reactivity.**
Call `SELECT forge_enable_reactivity('table_name');` in migrations. Hand-written triggers break change detection because the payload format won't match the Reactor.

**Using `SELECT *` in subscribed queries.**
Column-aware invalidation only works with explicit column lists. `SELECT *` falls back to table-level invalidation, triggering unnecessary re-fetches.

## 4. Workflows

**Using `tokio::sleep` for long waits.**
Workflows survive restarts; `tokio::sleep` doesn't. Use `ctx.sleep()` which persists the wake time in the event store.

**Using `FnOnce` closures in sequential steps with retry.**
Sequential steps need `Fn` closures because they may execute multiple times on retry. Parallel steps use `FnOnce` (no retry).

**Step results are cached by name on resume.**
If you rename a step between code versions, the old cache entry won't match and the step re-executes. Keep step names stable across versions.

## 5. Frontend

**Calling `refetch()` on reactive queries.**
SSE pushes updates automatically. `refetch()` doubles traffic and causes flicker. Exception: after workflow step completion in tests where SSE timing is unreliable.

**Reusing ForgeClient across auth state changes.**
The SSE session binds to a principal on first connection. Toggling the token causes `SESSION_PRINCIPAL_MISMATCH`. Destroy and recreate the client when auth changes. Dioxus: keyed remount via `use_auth_key()`. SvelteKit: SSE auto-reconnects on `setAuth`/`clearAuth`.

**Using effects/watchers for data fetching.**
Reactive stores (`$`-suffixed in Svelte, `use_*_live` in Dioxus) handle subscriptions. Effects create race conditions, duplicate subscriptions, and memory leaks.

**Forgetting `ForgeProvider`/`ForgeAuthProvider` at the root.**
Without it, `getForgeClient()` / `use_forge_client()` returns nothing and components silently fail.

**Dioxus: not cloning Mutation handles before async closures.**
Mutation handles must be cloned into the closure's scope before `spawn` or async blocks.

**Dioxus: wrong TLS feature for non-WASM targets.**
Use `reqwest` with `rustls-tls` for native targets. The default OpenSSL may not be available in all build environments.

**Dioxus: calling `logout` inside `with_auth_error_handler`.**
Logout triggers a keyed remount that destroys the refresh timer. Signal a refresh attempt instead; let the refresh logic decide whether to log out.

## 6. Auth

**Using authenticated client for refresh calls.**
The `ForgeClient` sends the expired token in the `Authorization` header. The runtime validates the token before checking if the endpoint is public. Use an anonymous `ForgeClient` for refresh:
```rust
let anon_client = ForgeClient::new(ForgeClientConfig::new(api_url.to_string()));
let input = RefreshInput::new(refresh_token);
refresh(&anon_client, input).await
```

**Trusting client-supplied ownership fields.**
Don't compare `input.user_id != ctx.require_user_id()`. The router's scope enforcement already validates identity fields at runtime. Manual checks are redundant and error-prone.

**Relying on `#[serde(skip)]` to hide fields.**
`forge generate` reads the Rust AST, not serde attributes. Skipped fields still appear in generated types. Create a separate public struct (e.g., `PublicUser`) without sensitive fields and use it in return types.

## 7. Build & Runtime

**Killing processes on occupied ports.**
If `lsof` shows the port is taken, tell the user and stop. Don't kill processes or change ports — another Forge instance may be running intentionally.

**Skipping `forge check` before finishing.**
`forge check` validates structure, formatting, and linting. Always run it. If formatting fails, run `cargo fmt` and `bun run format` first.

**Skipping `forge generate` between backend and frontend work.**
Backend contract changes require regeneration before frontend code will compile correctly.

### Common Build Errors

**"cannot find type `GetItemQuery` in module `functions`"**
Handler function is not `pub`, or the module path in `main.rs` is wrong. Make the function `pub` and verify the registration path.

**SQLx compile errors (`SQLX_OFFLINE`)**
Run `cargo sqlx prepare` after the app has run against a real DB to populate the `.sqlx/` cache. Or use the runtime function form (`query_as::<_, T>(...)`) instead of the bang macro.

**"type mismatch" in generated frontend code**
Re-run `forge generate`. The generated types are stale relative to the backend.

**SSE not updating after mutations**
Check that the table has `SELECT forge_enable_reactivity('table_name');` in its migration. Without it, the Reactor has no trigger to detect changes.

**Jobs not executing**
Check in order: (1) `[worker]` section in `forge.toml`, (2) handler registered in `main.rs`, (3) `worker_capability` matches node config, (4) queue status via `SELECT status, count(*) FROM forge_jobs GROUP BY status;`.

**`ForgeConn` doesn't implement `Executor`**
Bind `ctx.conn().await?` to a `let mut conn` variable and pass `&mut conn` to query methods.

**Pool exhaustion (`pool timed out`)**
Check `pg_stat_activity` for stuck connections. Isolate analytics/jobs into separate pools via `forge.toml`. Ensure mutations drop connections before long non-DB work.
