# Quality Gates

Review criteria, anti-patterns, security, and checklists.

## Review Order

Check in this order (P0 first):

1. **Auth**: public endpoints intentional? Ownership validated server-side?
2. **Atomicity**: multi-write mutations use `transactional`? Jobs dispatched inside transactions?
3. **Data correctness**: not-found vs forbidden semantics correct? Validation at boundaries?
4. **Read consistency**: `consistent` attribute where read-after-write matters?
5. **Retry/idempotency**: external calls retried? Webhooks idempotent?
6. **Observability**: structured logging with context fields? Trace IDs propagated?
7. **Test coverage**: handler tests + edge cases + Playwright for UI?
8. **`forge check` passes**
9. **Types in right place**: schema in `src/schema/`, handlers in `src/functions/`
10. **Readability**: clear names, no obvious comments, minimal nesting

## Anti-Patterns

### Backend

| # | Pattern | Fix |
|---|---|---|
| AP-1 | Manual identity comparison (`input.user_id != ctx.require_user_id()`) | Remove; router enforces scope |
| AP-2 | `dispatch_job` in non-transactional mutation | Add `transactional` attribute |
| AP-3 | Generic error swallowing (`map_err(\|_\| ForgeError::Internal(...))`) | Preserve error context |
| AP-4 | `unwrap()` / `expect()` in handlers | Use `?` with proper error types |
| AP-5 | Dynamic SQL without `tables = [...]` | Add explicit table deps for reactivity |
| AP-6 | `SELECT *` in column-sensitive subscriptions | List specific columns |
| AP-7 | Missing `LIMIT` on unbounded queries | Always paginate |
| AP-8 | `tokio::sleep` > 100s in workflows | Use `ctx.sleep()` |
| AP-9 | Editing generated files | Run `forge generate` instead |
| AP-10 | Skipping `forge generate` after contract change | Always regenerate |
| AP-11 | Handler not registered in `main.rs` | Add register call |
| AP-12 | `CREATE TABLE IF NOT EXISTS` in migrations | Use proper migration versioning |
| AP-13 | Manual refetch loops in frontend | Use Forge reactivity |
| AP-14 | Dummy input on no-arg handlers | Omit the parameter |
| AP-15 | Defaulting to `public` on endpoints | Auth by default |
| AP-16 | `#[derive(...)]` before `#[forge::model]` | Model macro must come first |
| AP-17 | `log` as bare flag (`log` instead of `log = "info"`) | Use quoted string |
| AP-18 | Function named with macro suffix (`heartbeat_daemon`) | Just `heartbeat` |
| AP-19 | Claiming completion without running tests | Run tests first |
| AP-20 | `ctx.http()` when circuit breaker needed | Use `ctx.http_with_circuit_breaker()` |
| AP-21 | Same SQL/validation duplicated across handlers | Extract to `pub(crate) async fn helper(db: DbConn<'_>, ...)` |
| AP-22 | Deferring required tooling steps to the user | If the task depends on `forge generate` or migrations, execute them |
| AP-23 | Behavior changed without new tests | `forge check` and lint are not tests. Add handler/unit tests. |
| AP-24 | Declaring success before end-to-end boot verification | Boot the real app flow and verify the primary path |
| AP-25 | Missing trace/correlation in external calls | Forward `x-request-id` and `x-trace-id` headers |

### Frontend

| # | Pattern | Fix |
|---|---|---|
| FP-1 | Editing `$lib/forge/*` or `src/forge/*` | Never; run `forge generate` |
| FP-2 | Manual SSE/WebSocket management | Use generated stores/hooks |
| FP-3 | Client-side auth enforcement only | Always validate backend |
| FP-4 | Missing loading/error states | Handle all three states |
| FP-5 | `$effect` / `use_effect` for data fetching | Use reactive stores/hooks |
| FP-6 | Forgetting `ForgeProvider` in root | Required for client setup |
| FP-7 | Reusing authenticated ForgeClient for refresh calls | Use anonymous client (no token provider) for refresh |
| FP-8 | Calling `logout` in Dioxus `with_auth_error_handler` | Signal refresh attempt instead; logout kills the refresh timer |
| FP-9 | Setting refresh timer > 80% of token lifetime | Use ~2/3 of token lifetime (10min for 15min tokens) |
| FP-10 | Token toggling on existing ForgeClient (Dioxus) | Keyed remount via `use_auth_key()` to recreate client |
| FP-11 | Not handling rate limit `retry_after_secs` in error details | Show countdown or implement exponential backoff |

## Security Checklist

- [ ] All data-mutating endpoints require auth (not `public`)
- [ ] User-owned resources use scope keys validated by the router
- [ ] Secrets in env vars, not in code or `forge.toml` (use `${VAR}`)
- [ ] Webhook endpoints use signature verification (`hmac_sha256`)
- [ ] MCP tools have `require_role` and appropriate hints (`read_only`, `destructive`)
- [ ] Rate limits on public endpoints and expensive operations
- [ ] File upload size limits are appropriate for the use case
- [ ] CORS origins configured (not wildcard in production)
- [ ] `jwt_secret` is strong and rotated
- [ ] No `allow_unsigned` on production webhooks

## Data Integrity

- Multi-write mutations: `transactional` attribute required
- Not-found: `ForgeError::NotFound` (404). Forbidden: `ForgeError::Forbidden` (403). Do not leak existence info.
- Idempotency on all external-facing mutation endpoints
- Workflow step names must be stable across versions (used as cache keys)
- Cron exactly-once via `UNIQUE(cron_name, scheduled_time)` (no user action needed)

## Performance

- Bounded results: `LIMIT` on all list queries
- Explicit column lists in subscribed queries (enables column-aware invalidation)
- Pool isolation for mixed workloads (analytics, jobs, default)
- Appropriate indexes for WHERE/JOIN columns
- `cache = "30s"` for queries that tolerate stale reads
- Jobs for anything > 100ms that doesn't need synchronous response

## Default Safe Attributes

External mutations:
```rust
#[forge::mutation(transactional, rate_limit(requests = 10, per = "1m", key = "user"))]
```

MCP tools:
```rust
#[forge::mcp_tool(require_role("admin"), read_only, rate_limit(requests = 20, per = "1m"))]
```

Webhooks:
```rust
#[forge::webhook(path = "/hooks/x", signature = WebhookSignature::hmac_sha256("X-Sig", "SECRET_ENV"), idempotency = "header:X-Id")]
```

Durable jobs:
```rust
#[forge::job(retry(max_attempts = 5, backoff = "exponential"), timeout = "30m")]
```

## Clarity Refinement

After implementation is correct, scan for:
- Unnecessary nesting (flatten with early returns)
- Poor names (`data`, `handle`, `flag` → describe the thing)
- Obvious comments (`// increment counter` above `counter += 1`)
- Dead code or commented-out blocks (delete them)

This pass changes zero behavior. If in doubt, skip it.
