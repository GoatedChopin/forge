# Security and Scale

Apply these checks before finalizing.

## 1) Security Checklist

- Authentication:
  - default auth retained unless endpoint intentionally public
  - `require_role("...")` for sensitive operations
- Authorization:
  - user/tenant scope checks in code and SQL
  - never trust client-provided IDs without principal check
- Secrets:
  - read with `ctx.env_require(...)`
  - never log secret values
- Webhooks:
  - signature verification enabled
  - idempotency configured
- MCP tools:
  - private by default
  - timeout and rate limits on external-facing tools

## 2) Data Integrity Checklist

- multi-write mutation with side effects is `transactional`
- explicit not-found vs forbidden semantics
- idempotency keys where duplicate execution can cause damage
- workflow step names stable across versions

## 3) Performance and Scaling Checklist

- Queries:
  - bounded results or pagination
  - avoid N+1 DB patterns
  - right indexes for common filters
  - explicit column lists on hot-path queries (enables column-aware invalidation, skips unnecessary re-execution)
  - `#[forge::query(consistent)]` only where read-after-write is actually needed
- Connection pools:
  - pool isolation configured when mixed workloads risk starvation (`database.pools.default/jobs/analytics/observability`)
  - pool sizes tuned to actual workload ratios, not blanket defaults
- Jobs:
  - retries tuned to dependency reliability
  - long jobs report progress and check cancellation
  - worker capability routing for specialized workloads
- Crons:
  - timezone explicit for business schedules
  - catch-up behavior intentional
- Workflows:
  - only for truly durable multi-step orchestration
  - timeout aligned to process horizon

## 4) Read Replica Checklist

- `read_from_replica` enabled only when consistency expectations are explicit
- `#[forge::query(consistent)]` on strict read-after-write paths (post-checkout confirmation, permission checks after writes)
- dashboard/reporting paths left on replica routing (no `consistent`)
- health monitoring is automatic (15s ping), no manual failover needed
- fallback-to-primary behavior understood operationally

## 5) Observability Checklist

- OTLP config intentional (`enabled`, endpoint, sampling)
- structured logs include key IDs (`request_id`, `trace_id`, entity/run/job IDs)
- async retries/failures include attempt metadata
- quiet routes configured intentionally (health probes usually excluded)

## 6) Default safe attribute sets

### External mutation
```rust
#[forge::mutation(
    timeout = 15,
    rate_limit(requests = 30, per = "1m", key = "user")
)]
```

### External MCP tool
```rust
#[forge::mcp_tool(
    timeout = 15,
    rate_limit(requests = 60, per = "1m", key = "user")
)]
```

### External webhook
```rust
#[forge::webhook(
    path = "/webhooks/provider",
    signature = WebhookSignature::hmac_sha256("X-Signature", "PROVIDER_WEBHOOK_SECRET"),
    idempotency = "header:X-Request-Id",
    timeout = "30s"
)]
```

### Durable job
```rust
#[forge::job(
    timeout = "30m",
    retry(max_attempts = 5, backoff = "exponential", max_backoff = "10m"),
    priority = "normal"
)]
```
