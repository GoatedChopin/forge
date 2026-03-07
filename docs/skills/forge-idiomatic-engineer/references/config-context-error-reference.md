# Config, Context, and Error Reference

This is the quick operational reference for Forge apps.

## 1) forge.toml Configuration Reference

### [project]
- `name`: project identifier
- `version`: app version label

### [database]
- `mode`
- `url`
- `pool_size`
- `pool_timeout_secs`
- `statement_timeout_secs`
- `replica_urls`
- `read_from_replica` (enables health-aware round-robin across replicas; 15s ping interval, auto-fallback to primary)

### [database.pools.*]
Per-workload pool isolation (bulkhead). Unconfigured pools fall back to primary.
- `size`
- `timeout_secs`
- `statement_timeout_secs`

Pool routing:
- `default`: queries, mutations, reactor, rate limiter, cluster coordination
- `jobs`: job workers, cron runners, daemon processes, workflow executors
- `analytics`: available via `db.analytics_pool()` for user code
- `observability`: internal metrics collection (pool utilization, slow query tracking)

### [gateway]
- `port`
- `grpc_port`
- `max_connections`
- `request_timeout_secs`
- `cors_enabled`
- `cors_origins`
- `quiet_routes`

### [function]
- `max_concurrent`
- `timeout_secs`
- `memory_limit`

### [security]
- `secret_key`

### [auth]
- `jwt_algorithm`
- `jwt_secret`
- `jwks_url`
- `jwks_cache_ttl_secs`
- `jwt_issuer`
- `jwt_audience`
- `token_expiry`
- `session_ttl_secs`

### [mcp]
- `enabled`
- `path`
- `session_ttl_secs`
- `allowed_origins`
- `require_protocol_version_header`

### [worker]
- `max_concurrent_jobs`
- `job_timeout_secs`
- `poll_interval_ms`

### [cluster]
- `name`
- `discovery`
- `heartbeat_interval_secs`
- `dead_threshold_secs`

### [node]
- `roles`
- `worker_capabilities`

Roles:
- `gateway`
- `function`
- `worker`
- `scheduler`

### [observability]
- `enabled`
- `otlp_endpoint`
- `service_name`
- `enable_traces`
- `enable_metrics`
- `enable_logs`
- `sampling_ratio`
- `log_level`

### Config value types and env var interpolation

Environment variable interpolation (`${VAR}` or `${VAR-default}`) only works for **string** values in `forge.toml`. Boolean and integer fields must use literal TOML values. Using `${VAR}` syntax for a boolean field causes a TOML parse error.

```toml
# correct
enabled = false
otlp_endpoint = "${OTEL_EXPORTER_OTLP_ENDPOINT-http://localhost:4318}"

# wrong - boolean fields cannot use env var interpolation
enabled = ${FORGE_OTEL_ENABLED-false}
```

## 2) Context Quick Reference

### AuthContext (available as `ctx.auth` on request-capable contexts)

Identity:
- `ctx.auth.require_user_id()` -> `Result<Uuid>` (errors if unauthenticated)
- `ctx.auth.user_id()` -> `Option<Uuid>` (returns None if unauthenticated)
- `ctx.auth.require_subject()` -> `Result<String>` (raw subject string)
- `ctx.auth.subject()` -> `Option<String>`
- `ctx.auth.principal_id()` -> `Option<String>` (prefers sub claim, falls back to UUID)
- `ctx.auth.require_tenant_id()` -> `Result<Uuid>`

State:
- `ctx.auth.is_authenticated()` -> `bool`

Roles:
- `ctx.auth.roles()` -> `&[String]`
- `ctx.auth.has_role(role)` -> `bool`

Claims:
- `ctx.auth.claim(key)` -> `Option<&serde_json::Value>`
- `ctx.auth.claims()` -> `&HashMap<String, serde_json::Value>`

Shorthand helpers `ctx.require_user_id()` and `ctx.require_subject()` delegate to `ctx.auth`.

### Claims Builder (for token issuance)

```rust
use forge::forge_core::Claims;

let claims = Claims::builder()
    .user_id(uuid)              // sets sub claim as UUID
    .subject("custom-sub")     // sets sub as arbitrary string
    .role("admin")             // add single role
    .roles(vec!["admin", "editor"]) // set multiple roles
    .claim("org_id", json!("org-123")) // custom claim
    .tenant_id(tenant_uuid)   // sets tenant_id claim
    .duration_secs(3600)       // token expiry
    .build()?;
```

Reading claims back:
- `claims.get_claim(key)` -> `Option<&serde_json::Value>`
- `claims.has_role(role)` -> `bool`
- `claims.tenant_id()` -> `Option<Uuid>`
- `claims.is_expired()` -> `bool`

### QueryContext
- `ctx.db()` (routes to replica or primary based on `consistent` flag and replica health)
- `ctx.db_conn()` (returns `DbConn<'_>` for shared helper functions)
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
- `ctx.progress(pct, message)` (report progress 0-100)
- `ctx.heartbeat().await` (keep long-running job alive)
- `ctx.check_cancelled().await` (errors if cancellation requested)
- `ctx.is_cancel_requested()` -> `bool` (check without error)
- `ctx.save(key, value)`, `ctx.saved().await`, `ctx.set_saved(data)` (persistent key-value state across retries)
- `ctx.is_retry()` -> `bool` (true when attempt > 1)
- `ctx.is_last_attempt()` -> `bool`
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
- `ctx.parallel().step(...).step(...).run().await` (concurrent step execution)
- `ctx.sleep(duration)`, `ctx.sleep_until(datetime)`
- `ctx.wait_for_event::<T>(event_name, timeout)` (pause until external event)
- `ctx.workflow_time()`, `ctx.elapsed()`
- `ctx.is_resumed()` -> `bool`
- `ctx.is_step_completed(name)`, `ctx.is_step_started(name)`
- `ctx.get_step_result::<T>(name)`, `ctx.get_step_state(name)`
- `ctx.tenant_id()` -> `Option<Uuid>`

### DaemonContext
- `ctx.db()`, `ctx.db_conn()`, `ctx.http()`
- `ctx.dispatch_job(...)`, `ctx.start_workflow(...)`
- `ctx.shutdown_signal()` (await this to block until shutdown)
- `ctx.is_shutdown_requested()` -> `bool` (non-blocking check)
- `ctx.heartbeat()` (keep daemon registration alive)
- `ctx.trace_id()`, `ctx.span()`

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
- `request_id` -> `Uuid`
- `trace_id` -> `String`
- `client_ip` -> `Option<String>`
- `user_agent` -> `Option<String>`
- `timestamp` -> `chrono::DateTime<Utc>`

### Environment Access
All contexts implement `EnvAccess`:
- `ctx.env(key)` -> `Option<String>`
- `ctx.env_or(key, default)` -> `String`
- `ctx.env_require(key)` -> `Result<String>`
- `ctx.env_parse::<T>(key)` -> `Result<T>` (parses to any `FromStr` type)

## 3) Error Handling Reference

### Preferred ForgeError by condition

- Invalid input contract: `ForgeError::Validation`
- Malformed arguments: `ForgeError::InvalidArgument`
- Missing auth / invalid token: `ForgeError::Unauthorized`
- No permission / scope mismatch: `ForgeError::Forbidden`
- Missing entity: `ForgeError::NotFound`
- Deadline exceeded: `ForgeError::Timeout`
- Rate-limited request: `ForgeError::RateLimitExceeded { ... }`
- DB infrastructure fault: `ForgeError::Database` / `ForgeError::Sql`
- Truly unknown server failure: `ForgeError::Internal`

### Mapping guidance

- Do not collapse domain-specific conditions into `Internal`.
- Preserve root cause where useful (`Database(e.to_string())`, `e.into()`).
- Distinguish `NotFound` from `Forbidden` to avoid information leaks and logic bugs.

### HTTP semantics

- `Validation` / `InvalidArgument` => 400
- `Unauthorized` => 401
- `Forbidden` => 403
- `NotFound` => 404
- `RateLimitExceeded` => 429
- `Timeout` => 504
- Others => 500

## 4) Operational Conditions Checklist

Before completion, validate:
- config settings match deployment topology
- read replica mode matches consistency needs (`consistent` attribute on read-after-write queries)
- pool isolation sized to workload ratios when mixed workloads exist
- quiet routes list is intentional
- auth mode and key material are production-safe
- observability toggles and sampling are deliberate
