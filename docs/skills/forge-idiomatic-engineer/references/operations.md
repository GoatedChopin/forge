# Operations

Production deployment, scaling, observability, and hardening.

## Deploy

Single binary: `cargo build --release`. Frontend embedded via `embedded-frontend` feature flag (Cargo feature that bundles the frontend build directory into the binary).

### Docker

```dockerfile
FROM rust:1-alpine AS builder
RUN apk add --no-cache musl-dev openssl-dev openssl-libs-static pkgconf
WORKDIR /app
COPY . .
RUN cargo build --release

FROM alpine:3.21
RUN apk add --no-cache ca-certificates libgcc
COPY --from=builder /app/target/release/my-app /app/my-app
COPY --from=builder /app/migrations /app/migrations
ENV RUST_LOG=info
CMD ["/app/my-app"]
```

Migrations are loaded from filesystem at runtime (not embedded). Copy the `migrations/` directory into the production image.

### Health Endpoints

| Path | Purpose | Response |
|---|---|---|
| `/_api/health` | Liveness | 200 always |
| `/_api/ready` | Readiness | 200 or 503 (checks DB + reactor) |

### Graceful Shutdown

On SIGTERM: stop accepting → drain in-flight (30s timeout) → release leadership → deregister → close connections.

```yaml
# Kubernetes
terminationGracePeriodSeconds: 45  # > drain timeout (30s)
readinessProbe:
  httpGet:
    path: /_api/ready
    port: 8080
```

## Scaling

### Multiple Nodes

All coordination through PostgreSQL. No separate service mesh.

- Leader election: advisory locks (`pg_try_advisory_lock`), not Raft/Paxos
- Job claiming: `FOR UPDATE SKIP LOCKED` (no thundering herd)
- Heartbeat: 5s interval, dead after `max(15s, 3 * adaptive_interval)`
- Node roles: `gateway`, `function`, `worker`, `scheduler`

### Worker Pools

Route jobs to specialized workers:
```rust
#[forge::job(worker_capability = "gpu")]
```

```toml
[node]
roles = ["worker"]
worker_capabilities = ["gpu", "general"]
```

Jobs without `worker_capability` run on any worker.

### Read Replicas

```toml
[database]
replica_urls = ["postgres://replica1...", "postgres://replica2..."]
read_from_replica = true
```

Round-robin distribution via atomic counter. Health monitor pings replicas every 15s. Unhealthy replicas skipped, falls back to primary.

Use `#[forge::query(consistent)]` for read-after-write paths:
- Immediate post-mutation confirmation screens
- Permission checks that depend on just-written state
- Idempotency checks that must observe latest writes

Replica-safe (no `consistent` needed): dashboards, analytics, non-critical listing screens, background reporting.

### Pool Isolation

```toml
[database.pools.default]
size = 30

[database.pools.jobs]
size = 10

[database.pools.analytics]
size = 5
statement_timeout_secs = 600
```

Prevents a runaway analytics query from exhausting connections for user requests.

## Observability

```toml
[observability]
enabled = true
otlp_endpoint = "http://localhost:4318"
sampling_ratio = 1.0
log_level = "info"
```

### Trace Correlation

Every request gets `request_id` (UUID) and `trace_id` (distributed trace). Available via `ctx.request.request_id` and `ctx.request.trace_id`.

Pass to external calls:
```rust
ctx.http()
    .post(url)
    .header("x-request-id", ctx.request.request_id.to_string())
    .header("x-trace-id", &ctx.request.trace_id)
```

### Quiet Routes

Exclude noisy health checks from logs/traces:
```toml
[gateway]
quiet_routes = ["/_api/health", "/_api/ready"]
```

### Structured Logging

```rust
tracing::info!(job_id = %ctx.job_id, attempt = ctx.attempt, "Processing");
```

Use context fields, not string interpolation. Set `RUST_LOG=info` (or `debug` for troubleshooting).

## Real-Time Tuning

### Debouncing

Default: 50ms quiet period after last change, 200ms max window. Configurable in code via `InvalidationConfig`.

### Adaptive Tracking

Row-level tracking for 1-100 subscribed rows per table. Table-level above 100. Hysteresis band: switches at >100, back at <50. Max 10,000 tracked rows per table.

### Column-Aware Invalidation

Queries with explicit column lists (`SELECT title, completed FROM todos`) skip re-execution when only unrelated columns change (`UPDATE todos SET updated_at = now()`). `SELECT *` defeats this optimization.

### SSE

Keepalive: 30s ping. Max sessions: 10,000 (configurable via `gateway.sse_max_sessions`). Reconnection handled client-side with exponential backoff.

## Circuit Breaker

Defaults: 5 failures → open, 30s base backoff, 1.5x multiplier, 10min max, 2 successes → close from half-open. Tracks per host (`scheme://host:port`).

Use `ctx.http_with_circuit_breaker()` on MutationContext. Raw `ctx.http()` bypasses the circuit breaker.

## Caching

Content-addressable: `hash(function_name + args + auth_scope)`. TTL-based with lazy eviction. Max 10,000 entries. Different users get separate cache entries via auth_scope_hash. Invalidate programmatically via `cache.invalidate(fn, args)` or `cache.invalidate_function(fn)`.

## Operational Checklist

- [ ] `DATABASE_URL` set (not embedded PG in production)
- [ ] `JWT_SECRET` rotated and not checked into source control
- [ ] Health endpoints accessible from load balancer
- [ ] `terminationGracePeriodSeconds` > 30s
- [ ] Pool isolation configured for mixed workloads
- [ ] `quiet_routes` excludes health endpoints from logs
- [ ] `RUST_LOG` set to `info` (not `debug` in production)
- [ ] Migrations tested with rollback (`forge migrate down`)
- [ ] Rate limits configured on public endpoints
- [ ] Circuit breaker enabled for external API calls
