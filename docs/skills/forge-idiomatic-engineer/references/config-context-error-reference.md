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

## 2) Context Quick Reference

### QueryContext
- `ctx.db()` (routes to replica or primary based on `consistent` flag and replica health)
- `ctx.db_conn()`
- `ctx.require_user_id()`
- `ctx.require_subject()`
- `ctx.auth` and `ctx.request`
- env helpers (`env`, `env_or`, `env_require`, `env_parse`)

### MutationContext
- `ctx.db()` (transaction-aware `DbConn`)
- `ctx.pool()`
- `ctx.http()`
- `ctx.dispatch_job(...)`
- `ctx.start_workflow(...)`
- `ctx.cancel_job(...)`
- auth and env helpers

### JobContext
- `ctx.db()`, `ctx.db_conn()`, `ctx.http()`
- `ctx.progress(...)`
- `ctx.heartbeat().await`
- `ctx.check_cancelled().await`
- `ctx.save(...)`, `ctx.saved().await`, `ctx.set_saved(...)`

### CronContext
- `ctx.db()`, `ctx.db_conn()`, `ctx.http()`
- `ctx.delay()`, `ctx.is_late()`
- `ctx.log.info/warn/error/debug(...)`

### WorkflowContext
- `ctx.db()`, `ctx.db_conn()`, `ctx.http()`
- `ctx.step(...).run().await`
- `ctx.parallel().step(...).run().await`
- `ctx.sleep(...)`, `ctx.wait_for_event(...)`
- `ctx.workflow_time()`, step state getters

### DaemonContext
- `ctx.db()`, `ctx.db_conn()`, `ctx.http()`
- `ctx.dispatch_job(...)`, `ctx.start_workflow(...)`
- `ctx.shutdown_signal()`

### WebhookContext
- `ctx.db()`, `ctx.db_conn()`, `ctx.http()`
- `ctx.header(...)`, `ctx.headers()`
- `ctx.dispatch_job(...)`, `ctx.cancel_job(...)`
- `ctx.idempotency_key`

### McpToolContext
- `ctx.db()`, `ctx.db_conn()`
- `ctx.require_user_id()`, `ctx.require_subject()`
- `ctx.dispatch_job(...)`, `ctx.start_workflow(...)`

### RequestMetadata
From request-capable contexts:
- `request_id`
- `trace_id`
- `client_ip`
- `user_agent`
- `timestamp`

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
