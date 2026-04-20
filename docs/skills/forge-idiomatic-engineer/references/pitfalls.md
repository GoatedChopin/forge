# Pitfalls

Fast index of mistakes. Assume the happy path will fail — entities vanish, tokens expire, networks drop.

## 1. Generated Code

- **Never edit** `frontend/src/lib/forge/` (Svelte) or `frontend/src/forge/` (Dioxus). `forge generate` overwrites them. Fix the Rust source instead.
- Run `forge generate` after every backend change. Skipping causes runtime deserialization errors.
- `#[forge::model]` must come **before** `#[derive(...)]`:

```rust
// WRONG — derive expands before model sees the struct
#[derive(Debug, Clone)]
#[forge::model]
pub struct Item { ... }

// RIGHT
#[forge::model]
#[derive(Debug, Clone)]
pub struct Item { ... }
```

## 2. Environment & Configuration

- `ctx.env_require()` / `ctx.env_or()` — **not** `std::env::var()`. See [API Reference](./api.md#environment-variables). Mocks only hook the context methods.
- `ctx.http()` for outbound RPC; `ctx.raw_http()` only when you need streaming or custom redirects.
- Don't copy-paste config helpers (`app_url()`, etc.) across handlers. Extract to `src/utils/`.
- **TLS cert/key must be set together**: `[gateway.tls]` rejects configs that set only `cert_path` or only `key_path`. Set both for file-based mode, or neither for self-signed. Startup fails fast with a clear config error.
- **TLS cert/key files must be readable and valid PEM**: Startup fails fast if paths are missing, unreadable, or malformed. The error message includes the offending path. Fix at deploy time, not at first request.
- **Health checks must switch to HTTPS when TLS is enabled**: Load balancer / Kubernetes probes targeting `/_api/ready` will fail with TLS handshake errors if they're still configured for HTTP. Update target group protocol (ALB) or probe `scheme: HTTPS` (k8s) when flipping `[gateway.tls] enabled = true`.
- **Do not use self-signed mode for public-facing deployments**: Self-signed is for zero-trust-behind-LB use. Browsers and API clients will not trust the certificate. For public TLS, terminate at a load balancer / CDN, or use file-based mode with a real CA-issued certificate.
- **Certificate rotation requires restart**: File-based TLS does not hot-reload. Rolling deployment is the intended rotation mechanism.

## 3. Macros & Registration

- Handler fn must be `pub async fn` — private fns fail codegen.
- Don't include the type in the fn name (`heartbeat`, not `heartbeat_daemon` → avoids `HeartbeatDaemonDaemon`).
- Omit the args parameter entirely when a handler takes no input — no `Option<()>` or dummy structs.
- Handlers require auth by default; opt out with `public`.
- Register every handler in `src/main.rs` via `.register_*::<NameType>()` or `.auto_register()`. Macros alone don't wire them in.
- Attribute values like `log = "info"` must be quoted strings.

## 4. Database & Transactions

- Always use `sqlx::query!()` / `query_as!()` — never `sqlx::query()` / `query_as::<_,T>()`. Run `forge migrate prepare` after schema changes to refresh `.sqlx/`.
- Cast enums explicitly in SELECT: `status as "status: ScanStatus"`. Use `"column?"` only to override nullability inference.
- Dispatch jobs / workflows only inside `transactional` mutations. See [Patterns](./patterns.md#background-jobs).
- **Pick the right handle** — details in [API Reference](./api.md#database-access--three-shapes):

```rust
// Query: ForgeDb, standard sqlx convention
sqlx::query_as!(User, "...", id).fetch_one(ctx.db()).await?

// Mutation: ForgeConn for transactional writes
let mut conn = ctx.conn().await?;
sqlx::query_as!(User, "...", id).fetch_one(&mut conn).await?
```

- **`DbConn` has an inverted convention** — it is not a sqlx `Executor`. Call `.fetch_*` on the `DbConn`, passing the query: `db.fetch_optional(sqlx::query_as!(...))`. Also: `DbConn` only wraps `query_as!`. For `query!` / `query_scalar!` / `execute`, use `ctx.db()` or `&mut *conn`.
- Use a real DB in tests (`IsolatedTestDb`) — mocks hide migration bugs and constraint violations.
- Enable reactivity via `SELECT forge_enable_reactivity('table_name');` in migrations. Never hand-write triggers.
- Avoid `SELECT *` in subscribed queries — it forces table-level invalidation. Explicit column lists unlock row-level tracking.
- For enriched / joined reads, define a dedicated struct (e.g. `SiteSummary`). Returning the base model silently drops joined columns.

## 5. Workflows

- `ctx.sleep()`, not `tokio::sleep` — only `ctx.sleep()` persists across restarts.
- Step names are cache keys. Renaming breaks resume. Bump the workflow version instead.
- Signature mismatch at startup blocks runs and flips `/_api/ready` to 503. Check for in-flight runs before removing an old version.
- Always set a timeout on `wait_for_event` so stalls become observable.

## 6. Frontend

- Never call `refetch()` on an SSE-backed store. The stream pushes updates.
- Guard against `loading` / `error` / null `data`. See [Frontend](./frontend.md#subscription-state-shape).
- Let the generated auth helper manage tokens. Manual `localStorage` writes bypass SSE reconnect — see [Frontend](./frontend.md#authentication-and-session-management).
- Don't fetch inside `$effect` / `use_effect` — race conditions and leaks. Use subscription hooks.
- Route mutation errors to a global handler (`onMutationError` in Svelte, `on_mutation_error` in Dioxus). Users must see failures.

## 7. Authentication

- `jwt_secret` in `forge.toml` is required for `issue_token_pair()`. TTLs default to 1h / 30d if omitted.
- Drop `ctx.conn()` before calling `issue_token_pair()` — it needs its own connection and will deadlock on pool exhaustion.
- Refresh calls must be unauthenticated — the built-in `refresh_token` provider does this, don't hand-roll.
- Reserve `Forbidden` for real permission violations. Using it for billing/plan state triggers the global `onAuthError` handler and logs the user out. Use `InvalidArgument` for business-rule rejections.

## 8. Custom Routes and Uploads

- **Custom routes live under `/_api`**: `ForgeBuilder::custom_routes(|pool| ...)` merges into the gateway router. A declared `/export/csv` resolves to `/_api/export/csv`. Document the full path to clients — writing the raw declaration is a common off-by-prefix bug.
- **Never `.unwrap()` `AuthContext`**: The auth middleware still forwards unauthenticated requests to your handler. `auth.user_id().unwrap()` panics and hits the workspace `clippy::unwrap_used` deny. Use `match auth.user_id()` with an early 401 return.
- **Don't re-implement auth in custom handlers**: Middleware already parses the JWT and injects `Extension<AuthContext>`. Do not reach for headers or parse tokens yourself.
- **Per-file upload cap is independent of total body**: `gateway.max_body_size` caps the full multipart body, but individual files are capped by `gateway.max_file_size` (defaults to `"10mb"`). A mutation that legitimately accepts a big file must declare `max_size = "…"`; that value becomes both the total and per-file limit for that endpoint.

## 9. Resilience & Hygiene

- Always check entity existence with `fetch_optional().await?.ok_or_else(|| ForgeError::NotFound(format!(...)))`. See [Resilience](./resilience.md#2-database-and-data-integrity).
- Include IDs and context in error messages.
- Delete commented-out code immediately — Git is the history.
- Run `forge check` after every change to catch orphans.

## 10. Code Reuse

- Don't hand-roll `SELECT * FROM users WHERE id = $1` in every handler. Extract `current_user(db, user_id)`. See [Recipes](./recipes.md#1-current-user-helper).
- Don't hand-roll auth storage in Svelte — the generated `auth.svelte.ts` (`setAuth`, `clearAuth`, `startRefreshLoop`) handles SSE reconnection. See [Svelte](./frontend/svelte.md#authentication-and-session-management).
- Don't `INSERT INTO forge_jobs` / `forge_workflow_runs` / `forge_signals_events` manually. Direct writes skip the outbox, break idempotency, and break SKIP LOCKED ordering. Use `ctx.dispatch_job()`, `ctx.start_workflow()`, `ctx.record_signal()`. `forge check` fails the build on raw writes.

## 11. Integration Anti-patterns

- **Email HTML inlined in handlers**: escaping, i18n, and preview-time testing become painful. Use [`askama`](https://docs.rs/askama) templates under `templates/`. See [Recipes](./recipes.md#4-transactional-email-askama--smtp).
- **`serde_json::Value` webhooks**: untyped payloads defer validation to runtime. Declare a typed struct — the macro deserialises for you.
- **String-matching error messages**: match `ForgeError` variants in Rust, internal codes (`"NOT_FOUND"`, `"UNAUTHORIZED"`) on the frontend. See [API Reference](./api.md#forgeerror-variants).
- **Hand-rolled HMAC verification**: use the `WebhookSignature` constructors. See [API Reference](./api.md#signature-constructors).
- **Provider SDKs for payments / AI / S3**: stay neutral — standard protocols (HMAC, S3 API, HTTP JSON) work everywhere. See [Recipes](./recipes.md).

## 12. Svelte Reactive

- Don't wrap `listTodos$()` runes helpers in a `toReactive` adapter. They already manage lifecycle via `$effect` roots — wrapping reintroduces the leaks the rune form eliminates. See [Svelte](./frontend/svelte.md#using-svelte-5-runes).
- Never create a store inside a `$derived`. Opens a new SSE subscription every recomputation.
- Set `export const ssr = false;` in `+layout.ts`. SSE / `EventSource` / `localStorage` aren't available server-side.
