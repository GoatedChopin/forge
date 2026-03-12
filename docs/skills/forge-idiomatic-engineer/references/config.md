# Config, CLI, and Project Reference

Forge configuration, context API, error types, CLI workflow, and project structure.

## Contents

1. [forge.toml configuration](#1-forgetoml-configuration)
2. [Context quick reference](#2-context-quick-reference)
3. [Error handling reference](#3-error-handling-reference)
4. [CLI workflow](#4-cli-workflow)
5. [Project structure](#5-project-structure)
6. [Common mistakes](#6-common-mistakes)

---

## 1) forge.toml configuration

### [project]
- `name`: project identifier
- `version`: app version label

### [database]
- `url`, `pool_size`, `pool_timeout_secs`, `statement_timeout_secs`
- `replica_urls`, `read_from_replica` (health-aware round-robin, 15s ping, auto-fallback to primary)

### [database.pools.*]
Per-workload pool isolation (bulkhead). Unconfigured pools fall back to primary.
- `size`, `timeout_secs`, `statement_timeout_secs`

Pool routing:
- `default`: queries, mutations, reactor, rate limiter, cluster coordination
- `jobs`: job workers, cron runners, daemon processes, workflow executors
- `analytics`: available via `db.analytics_pool()` for user code
- `observability`: internal metrics collection (pool utilization, slow query tracking)

### [gateway]
- `port`, `grpc_port`, `max_connections`, `request_timeout_secs`
- `cors_enabled`, `cors_origins`, `quiet_routes`

### [function]
- `max_concurrent`, `timeout_secs`, `memory_limit`

### [security]
- `secret_key`

### [auth]
- `jwt_algorithm`, `jwt_secret`, `jwks_url`, `jwks_cache_ttl_secs`
- `jwt_issuer`, `jwt_audience`, `token_expiry`, `session_ttl_secs`

### [mcp]
- `enabled`, `path`, `session_ttl_secs`, `allowed_origins`, `require_protocol_version_header`

### [worker]
- `max_concurrent_jobs`, `job_timeout_secs`, `poll_interval_ms`

### [cluster]
- `name`, `discovery`, `heartbeat_interval_secs`, `dead_threshold_secs`

### [node]
- `roles`: `gateway`, `function`, `worker`, `scheduler`
- `worker_capabilities`

### [observability]
- `enabled`, `otlp_endpoint`, `service_name`
- `enable_traces`, `enable_metrics`, `enable_logs`
- `sampling_ratio`, `log_level`

### Config value types and env var interpolation

Environment variable interpolation (`${VAR}` or `${VAR-default}`) works for all value types. Substitution happens before TOML parsing, so the result must be valid TOML syntax.

```toml
# booleans: no quotes (substitution produces bare true/false, valid TOML)
enabled = ${FORGE_OTEL_ENABLED-false}

# strings: use quotes (substitution produces the value inside the quotes)
otlp_endpoint = "${OTEL_EXPORTER_OTLP_ENDPOINT-http://localhost:4318}"

# wrong: quoting a boolean produces a string, not a boolean
enabled = "${FORGE_OTEL_ENABLED-false}"
```

## 2) Context quick reference

### AuthContext (available as `ctx.auth` on request-capable contexts)

Identity:
- `ctx.auth.require_user_id()` -> `Result<Uuid>`
- `ctx.auth.user_id()` -> `Option<Uuid>`
- `ctx.auth.require_subject()` -> `Result<String>`
- `ctx.auth.subject()` -> `Option<String>`
- `ctx.auth.principal_id()` -> `Option<String>`
- `ctx.auth.require_tenant_id()` -> `Result<Uuid>`

State: `ctx.auth.is_authenticated()` -> `bool`

Roles: `ctx.auth.roles()` -> `&[String]`, `ctx.auth.has_role(role)` -> `bool`

Claims: `ctx.auth.claim(key)` -> `Option<&Value>`, `ctx.auth.claims()` -> `&HashMap<String, Value>`

Shorthand: `ctx.require_user_id()` and `ctx.require_subject()` delegate to `ctx.auth`.

### Claims Builder (for token issuance)

```rust
use forge::forge_core::Claims;

let claims = Claims::builder()
    .user_id(uuid)
    .subject("custom-sub")
    .role("admin")
    .roles(vec!["admin", "editor"])
    .claim("org_id", json!("org-123"))
    .tenant_id(tenant_uuid)
    .duration_secs(3600)
    .build()?;
```

Reading claims: `claims.get_claim(key)`, `claims.has_role(role)`, `claims.tenant_id()`, `claims.is_expired()`

### QueryContext
- `ctx.db()` (routes to replica or primary based on `consistent` flag)
- `ctx.db_conn()` (returns `DbConn<'_>` for shared helpers)
- `ctx.auth`, `ctx.request`
- env helpers (`env`, `env_or`, `env_require`, `env_parse`)

### MutationContext
- `ctx.db()` (transaction-aware `DbConn`)
- `ctx.pool()` (direct pool access, bypasses transaction)
- `ctx.is_transactional()` -> `bool`
- `ctx.http()` -> `&reqwest::Client`
- `ctx.http_with_circuit_breaker()` -> `&CircuitBreakerClient` (per-host failure tracking, auto-trips after 5 failures)
- `ctx.issue_token(&claims)` (HMAC mode only)
- `ctx.dispatch_job(name, args)`, `ctx.dispatch_job_with_context(name, args, context)`
- `ctx.start_workflow(name, args)`
- `ctx.cancel_job(job_id, reason)`
- `ctx.auth`, `ctx.request`, env helpers

### JobContext
- `ctx.db()`, `ctx.db_conn()`, `ctx.http()`
- `ctx.progress(pct, message)` (0-100)
- `ctx.heartbeat().await` (keep long-running job alive)
- `ctx.check_cancelled().await` (errors if cancellation requested)
- `ctx.is_cancel_requested()` -> `bool`
- `ctx.save(key, value)`, `ctx.saved().await`, `ctx.set_saved(data)` (persistent state across retries)
- `ctx.is_retry()` -> `bool`, `ctx.is_last_attempt()` -> `bool`
- `ctx.attempt` -> `u32`, `ctx.job_id`

### CronContext
- `ctx.db()`, `ctx.db_conn()`, `ctx.http()`
- `ctx.delay()` -> `chrono::Duration`, `ctx.is_late()` -> `bool`
- `ctx.run_id`, `ctx.trace_id()`
- `ctx.log.info/warn/error/debug(message, json_data)`
- `ctx.span()` (parent span for child trace propagation)

### WorkflowContext
- `ctx.db()`, `ctx.db_conn()`, `ctx.http()`
- `ctx.step(name, closure).compensate(closure).run().await`
- `ctx.parallel().step(...).step(...).run().await` (concurrent steps)
- `ctx.sleep(duration)`, `ctx.sleep_until(datetime)`
- `ctx.wait_for_event::<T>(event_name, timeout)`
- `ctx.workflow_time()`, `ctx.elapsed()`
- `ctx.is_resumed()`, `ctx.is_step_completed(name)`, `ctx.is_step_started(name)`
- `ctx.get_step_result::<T>(name)`, `ctx.get_step_state(name)`
- `ctx.tenant_id()` -> `Option<Uuid>`

### DaemonContext
- `ctx.db()`, `ctx.db_conn()`, `ctx.http()`
- `ctx.dispatch_job(...)`, `ctx.start_workflow(...)`
- `ctx.shutdown_signal()` (await to block until shutdown)
- `ctx.is_shutdown_requested()` -> `bool`
- `ctx.heartbeat()`, `ctx.trace_id()`, `ctx.span()`

### WebhookContext
- `ctx.db()`, `ctx.db_conn()`, `ctx.http()`
- `ctx.header(name)` -> `Option<String>` (case-insensitive)
- `ctx.headers()` -> all request headers
- `ctx.dispatch_job(...)`, `ctx.cancel_job(...)`
- `ctx.idempotency_key`

### McpToolContext
- `ctx.db()`, `ctx.db_conn()`
- `ctx.require_user_id()`, `ctx.require_subject()`
- `ctx.dispatch_job(...)`, `ctx.start_workflow(...)`
- `ctx.request` (request_id, trace_id, client_ip, user_agent, timestamp)

### RequestMetadata
Available via `ctx.request` on request-capable contexts:
- `request_id` -> `Uuid`, `trace_id` -> `String`
- `client_ip` -> `Option<String>`, `user_agent` -> `Option<String>`
- `timestamp` -> `chrono::DateTime<Utc>`

### Environment Access
All contexts implement `EnvAccess`:
- `ctx.env(key)` -> `Option<String>`
- `ctx.env_or(key, default)` -> `String`
- `ctx.env_require(key)` -> `Result<String>`
- `ctx.env_parse::<T>(key)` -> `Result<T>` (parses to any `FromStr` type)

### Macro Attribute Reference

**Query**: `public`, `require_role("admin")`, `tables = ["orders", "items"]`, `cache = "30s"`, `consistent`, `timeout = 10`, `rate_limit(requests = 100, per = "1m", key = "user")`, `log`

**Mutation**: `public`, `require_role("admin")`, `transactional`, `timeout = 30`, `rate_limit(requests = 10, per = "1m", key = "ip")`, `log`

**Job**: `name = "custom"`, `priority = "high"` (background|low|normal|high|critical), `timeout = "30m"`, `ttl = "24h"`, `retry(max_attempts = 5, backoff = "exponential", max_backoff = "10m")`, `idempotent(key = "order_id")`, `worker_capability = "gpu"`, `public`, `require_role("admin")`

**Cron**: `"0 9 * * *"` (required), `timezone = "America/New_York"`, `catch_up`, `catch_up_limit = 3`, `timeout = "5m"`, `group = "reports"`

**Daemon**: `leader_elected = true`, `restart_on_panic = true`, `restart_delay = "5s"`, `startup_delay = "2s"`, `max_restarts = 10`

**Webhook**: `path = "/webhooks/stripe"`, `signature = WebhookSignature::hmac_sha256(...)`, `idempotency = "body:$.id"`, `timeout = "30s"`, `allow_unsigned`

**MCP Tool**: `name`, `title`, `description`, `require_role(...)`, `read_only`, `destructive`, `idempotent`, `open_world`, `timeout = 30`, `rate_limit(...)`

**Rate limit keys**: `"user"`, `"ip"`, `"tenant"`, `"global"`

## 3) Error handling reference

### Preferred ForgeError by condition

| Condition | Variant | HTTP |
|---|---|---|
| Invalid input contract | `Validation` | 400 |
| Malformed arguments | `InvalidArgument` | 400 |
| Missing auth / invalid token | `Unauthorized` | 401 |
| No permission / scope mismatch | `Forbidden` | 403 |
| Missing entity | `NotFound` | 404 |
| Rate-limited request | `RateLimitExceeded { ... }` | 429 |
| Deadline exceeded | `Timeout` | 504 |
| DB infrastructure fault | `Database` / `Sql` | 500 |
| Truly unknown server failure | `Internal` | 500 |

Do not collapse domain-specific conditions into `Internal`. Preserve root cause where useful. Distinguish `NotFound` from `Forbidden` to avoid information leaks.

## 4) CLI workflow

### Golden rule

Generate first, edit second. Backend source (`src/schema`, `src/functions`) is the source of truth. `forge generate` syncs backend contracts into frontend/runtime artifacts.

### Discovery before commands

Resolve the actual CLI entrypoints before you run anything:

- app root: directory containing `forge.toml`
- Forge CLI command: `forge`, project wrapper, documented local binary, or checked-in build output
- frontend package manager: inspect `frontend/package.json`, lockfiles, or project scripts
- frontend target: inspect `frontend/package.json` + `svelte.config.js` for SvelteKit, or `frontend/Cargo.toml` + `Dioxus.toml` for Dioxus
- database workflow: inspect project docs and available tooling

Do not assume the app binary itself supports Forge CLI subcommands.
If `forge` is not on `PATH`, search the repo and adjacent toolchain locations before treating it as unavailable.

### Go-to commands once the toolchain is confirmed

```bash
forge new my-app --template with-svelte/minimal
forge generate
forge check
```

### Required behavior

- Use `forge new` for new projects. Do not hand-scaffold the initial layout.
- Run Forge commands from the app root, not a guessed subdirectory.
- Author backend handlers/contracts directly in `src/functions/*` and `src/schema/*`.
- After changing schema/functions/macros, run `forge generate`.
- If `forge generate` is required but the CLI command is unresolved, solve that tooling problem first instead of faking generated output or deferring the step to the user.
- Migrations run automatically on backend startup. For manual migration management during dev, use `docker compose exec db psql -U postgres -d <dbname>` or `docker compose exec backend forge migrate status`.
- After adding handlers, verify `src/main.rs` registration before frontend work or delivery.
- Run `forge check` from app root before completion. Fix all findings, rerun until clean.
- For frontend, run the target's real quality gates: SvelteKit usually means `lint`, `svelte-check`, types, and formatting; Dioxus usually means `cargo fmt`, `cargo check`, and any project-specific frontend tests.
- For tasks that should work out of the box, boot the project through its real dev entrypoint (`forge dev` or equivalent) before delivery.

### Backend generation policy

- `forge add` is intentionally not part of this workflow.
- Backend primitives are authored directly in Rust source.
- `forge generate` keeps client/runtime artifacts aligned with backend code.
- If generation lands in an unexpected nested path such as `frontend/frontend/...`, stop and fix the cwd or config issue instead of patching the duplicate output.

### Never edit generated files

- `frontend/src/lib/forge/*`
- `frontend/src/forge/*`

When change is needed, modify Rust source or Forge config, then regenerate.
Do not hand-create files in these locations as a substitute for generation.

### Safe edit zones

- `src/functions/*`, `src/schema/*`, `src/utils/*`
- `frontend/src/routes/*`
- `frontend/src/lib/*` excluding generated Forge paths

### Migration cleanup

When creating real migration files, check `migrations/` for scaffolded files from `forge new`:
- `with-*/minimal` templates create `0001_initial.sql.example` (commented placeholder). Delete it before creating your real migration.
- `with-*/demo` templates create `0001_initial.sql` (real migration with tables). Delete it and drop those tables if already run.

Before writing a migration, inspect an existing project/template migration and preserve its expected format. Some projects require markers such as `-- @up` and `-- @down`.

Do not use `CREATE TABLE IF NOT EXISTS` in migrations. It silently skips creation if a conflicting table from the scaffold migration already exists.
Do not insert rows directly into `forge_migrations`. If migration state is wrong, fix the migration workflow instead of mutating Forge's bookkeeping table by hand.
Do not raw-apply Forge-formatted migration files with `psql < ...` unless you know the file is compatible with that runner; markers and Forge helper functions can make this path misleading.

### Database access during development

PostgreSQL is only accessible inside the Docker network. The backend connects via `db:5432`. To interact with the database from the host:

```bash
# psql session
docker compose exec db psql -U postgres -d <dbname>

# migration status
docker compose exec backend forge migrate status

# rollback
docker compose exec backend forge migrate down
```

### Common `forge check` issues

**`#[forge::model]` info warning**: Informational, not an error. Standard derives work fine without the macro.

**Clippy flakiness on first run**: Stale incremental cache. Forge sets `CARGO_INCREMENTAL=0` internally. Run `cargo clean` and retry if persistent.

## 5) Project structure

### Preferred layout

```text
src/
  main.rs
  functions/
    mod.rs
    ...
  schema/
    mod.rs
    ...
  utils/
    mod.rs
    ...
```

### Folder responsibilities

**`src/functions/`**: Forge handlers only (queries, mutations, jobs, crons, workflows, webhooks, MCP tools). Keep function-specific validation, orchestration, and helper logic nearby. Extract to `utils` only when genuinely shared.

**`src/schema/`**: All input/output structs, domain models, enums, and data contracts. `#[forge::model]` and `#[forge::forge_enum]` definitions. This is the single source of truth for type shapes; handlers import from here.

**`src/utils/`**: Pure helper functions, input normalization, validation helpers, formatting/mapping utilities. No framework-heavy coupling unless justified.

### Example module wiring

```rust
// src/main.rs
mod functions;
mod schema;
mod utils;

let builder = forge::ForgeRuntime::builder()
    .register_query::<functions::ListOrdersQuery>()
    .register_mutation::<functions::CreateOrderMutation>();

// src/functions/mod.rs
pub mod orders;
pub mod users;

// src/schema/mod.rs
pub mod order;
pub mod user;
```

Macros do not auto-register handlers. Adding a module and re-exporting it from `mod.rs` is not enough; `src/main.rs` must register each query, mutation, job, and workflow on the runtime builder.

### Test placement

Tests live with the code they test using inline `#[cfg(test)] mod tests` blocks. No separate `tests/` directory for unit tests.

### Rule of thumb

- Talks to context and performs side effects: `functions/`
- Models business entities/contracts: `schema/`
- Reusable pure logic: `utils/`
- Tests go in the same file as the code they cover
- Do not prematurely abstract one-off function logic into `utils/`

## 6) Common mistakes

- **Patching generated TS client manually**: update Rust function/schema signatures and rerun `forge generate`
- **Skipping generation after backend changes**: run `forge generate` immediately
- **Custom fetch wrappers bypassing generated client**: use generated API/stores/runes unless special constraint exists
- **Handwritten types drifting from Rust source**: keep Rust as type source and regenerate frontend types

### Operational conditions checklist

Before completion, validate:
- config settings match deployment topology
- read replica mode matches consistency needs
- pool isolation sized to workload ratios
- quiet routes list is intentional
- auth mode and key material are production-safe
- observability toggles and sampling are deliberate
