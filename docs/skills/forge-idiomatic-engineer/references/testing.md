# Testing Reference

## Minimum Bar

For every behavior change, add:
- Happy-path tests covering main success scenarios
- Failure-path tests (validation, authz, not-found, conflict)
- Boundary value tests (empty inputs, max lengths, zero amounts)
- Edge case tests for non-obvious logic branches

For new CRUD: one happy-path + one failure-path per handler, boundary tests for validated fields, one Playwright path if UI changed.

Bug fix => add a regression test that fails before the fix and passes after.

When a handler has non-trivial logic (calculations, transformations, decisions), extract it into a pure function in `utils/` and test exhaustively there. The handler becomes a thin adapter. Pure functions are cheapest to test and often contain the most subtle logic.

## Test Location

Tests live inline with the code they prove:
```rust
// src/functions/orders.rs
#[forge::mutation(transactional)]
pub async fn create_order(...) -> Result<Order> { ... }

#[cfg(test)]
mod tests {
    use super::*;
    use forge::testing::*;

    #[tokio::test]
    async fn test_create_order() { ... }
}
```

## Test Contexts

Every handler type has a matching test context with builder pattern.

### TestQueryContext

```rust
TestQueryContext::minimal()                    // unauthenticated, no DB
TestQueryContext::authenticated(user_id)       // UUID auth, no DB
TestQueryContext::with_pool(pool, Some(uid))   // with DB
TestQueryContext::builder()
    .as_user(Uuid::new_v4())
    .as_subject("firebase-uid")               // for non-UUID auth
    .with_role("admin")
    .with_roles(vec!["admin".into(), "editor".into()])
    .with_claim("org_id", json!("org-123"))
    .with_tenant(tenant_id)
    .with_pool(pool)
    .with_env("API_KEY", "test-key")
    .with_envs(map)
    .build()
```

Access: `ctx.db()` → `Option<&PgPool>`, `ctx.auth`, `ctx.request`, `ctx.env_mock()`.

### TestMutationContext

Same auth/env/pool builders, plus:
```rust
.mock_http("api.stripe.com/*", |req| MockResponse::json(json!({...})))
.mock_http_json("api.stripe.com/*", json!({...}))
.with_job_dispatch(Arc::new(MockJobDispatch::new()))
.with_workflow_dispatch(Arc::new(MockWorkflowDispatch::new()))
```

Access: `ctx.http()` → `&MockHttp`, `ctx.job_dispatch()`, `ctx.workflow_dispatch()`, `ctx.dispatch_job(...)`, `ctx.start_workflow(...)`, `ctx.pending_jobs()`, `ctx.assert_job_buffered("type")`.

### TestJobContext

```rust
TestJobContext::builder("send_email")
    .with_job_id(uuid)
    .as_retry(3)                              // attempt = 3
    .with_max_attempts(5)
    .as_last_attempt()                        // attempt = max_attempts
    .with_cancellation_requested()
    .as_user(uuid)
    .with_pool(pool)
    .mock_http_json("...", json!(...))
    .with_env("KEY", "val")
    .build()
```

Access: `ctx.progress(50, "halfway")`, `ctx.progress_updates()` → `Vec<{percent, message}>`, `ctx.saved()`, `ctx.save(k, v)`, `ctx.is_cancel_requested()`, `ctx.check_cancelled()`, `ctx.request_cancellation()`, `ctx.heartbeat()` (no-op).

Fields: `ctx.job_id`, `ctx.job_type`, `ctx.attempt`, `ctx.max_attempts`, `ctx.auth`.

### TestCronContext

```rust
TestCronContext::builder("daily_cleanup")
    .with_run_id(uuid)
    .scheduled_at(time)
    .executed_at(time)
    .with_timezone("America/New_York")
    .as_catch_up()
    .build()
```

Access: `ctx.delay()` → `chrono::Duration`, `ctx.is_late()` → delay > 1 min, `ctx.log.info("msg")`, `ctx.log.entries()`.

Fields: `ctx.run_id`, `ctx.cron_name`, `ctx.scheduled_time`, `ctx.execution_time`, `ctx.timezone`, `ctx.is_catch_up`, `ctx.log`.

### TestWorkflowContext

```rust
TestWorkflowContext::builder("onboarding")
    .with_run_id(uuid)
    .with_version(2)
    .with_workflow_time(fixed_time)            // pins deterministic time
    .as_resumed()
    .with_completed_step("step_a", json!({...}))
    .with_tenant(uuid)
    .build()
```

Access: `ctx.is_resumed()`, `ctx.workflow_time()`, `ctx.is_step_completed("name")`, `ctx.get_step_result::<T>("name")`, `ctx.record_step_start/complete(...)`, `ctx.completed_step_names()`, `ctx.sleep(dur)` (no-op, recorded), `ctx.sleep_called()`, `ctx.elapsed()`.

### TestDaemonContext

```rust
TestDaemonContext::builder("heartbeat")
    .with_instance_id(uuid)
    .build()
```

Access: `ctx.is_shutdown_requested()`, `ctx.request_shutdown()`, `ctx.shutdown_signal()` (awaitable), `ctx.shutdown_tx` (public, clone for async triggers).

### TestWebhookContext

```rust
TestWebhookContext::builder("stripe_webhook")
    .with_header("Stripe-Signature", "t=123,v1=abc")
    .with_headers(map)
    .with_idempotency_key("evt_123")
    .with_job_dispatch(Arc::new(MockJobDispatch::new()))
    .build()
```

Access: `ctx.header("name")` (case-insensitive), `ctx.headers()`, `ctx.job_dispatch()`, `ctx.dispatch_job(...)`. No workflow dispatch on webhooks.

## HTTP Mocking

Pattern matching supports `*` wildcard (start, middle, end of URL).

```rust
// JSON response
.mock_http_json("api.stripe.com/*", json!({"id": "ch_123"}))

// Custom handler
.mock_http("api.stripe.com/*", |req| {
    if req.body["amount"].as_i64().unwrap() > 10000 {
        MockResponse::error(400, "Amount exceeds limit")
    } else {
        MockResponse::json(json!({"id": "ch_123"}))
    }
})
```

Verification:
```rust
ctx.http().assert_called("api.stripe.com/*");
ctx.http().assert_called_times("api.stripe.com/*", 1);
ctx.http().assert_not_called("api.paypal.com/*");
ctx.http().assert_called_with_body("api.stripe.com/*", |body| body["amount"] == 1000);
let reqs = ctx.http().requests_to("api.stripe.com/*");
```

`MockResponse` constructors: `json(val)`, `ok()`, `error(status, msg)`, `not_found(msg)`, `unauthorized(msg)`, `internal_error(msg)`.

## Assertion Macros

```rust
assert_ok!(result);
assert_err!(result);
assert_err_variant!(result, ForgeError::NotFound(_));

assert_job_dispatched!(ctx, "send_email");
assert_job_dispatched!(ctx, "send_email", |args| args["to"] == "x");
assert_job_not_dispatched!(ctx, "send_sms");

assert_workflow_started!(ctx, "onboarding");
assert_workflow_started!(ctx, "onboarding", |input| input["plan"] == "premium");
assert_workflow_not_started!(ctx, "enterprise");

assert_http_called!(ctx, "api.stripe.com/*");
assert_http_not_called!(ctx, "api.paypal.com/*");
```

Helper functions: `error_contains(&err, "substring")`, `validation_error_for_field(&err, "field")`, `assert_json_matches(&actual, &pattern)` (partial/subset match).

## Database Testing

```rust
// Testcontainers (auto Docker PG, needs `testcontainers` feature)
let db = TestDatabase::from_env().await?;

// Explicit URL
let db = TestDatabase::from_url("postgres://localhost/test").await?;

// Isolated per-test database
let db = IsolatedTestDb::setup(
    "test_name",
    &forge::get_internal_sql(),
    Path::new("migrations"),
).await?;

// Use in context
let ctx = TestMutationContext::builder()
    .as_user(uuid)
    .with_pool(db.pool().clone())
    .build();

// Cleanup (optional, orphans GC'd on next run)
db.cleanup().await?;
```

`IsolatedTestDb` methods: `pool()`, `execute(sql)`, `run_sql(multi_statement_sql)`, `migrate(Path)`, `cleanup()`.

## Playwright

Always import from generated fixtures:
```typescript
import { test } from "../tests/fixtures";

test("creates a todo", async ({ page, rpc, gotoReady, uniqueId }) => {
    await gotoReady(page, "/");
    // ...
});
```

Fixtures: `rpc` (direct backend calls), `gotoReady` (waits for SSE readiness), `uniqueId` (test isolation), `ACTION_TIMEOUT`, `trackConsoleErrors(page)`.

Register a unique user per test using `uniqueId()` for full isolation, even when running in parallel.

`gotoReady()` waits for the first `/_api/subscribe` response, which is the signal that reactivity is wired up. Don't use it if SSE was already established during a login step. In that case, wait for a UI element that depends on server data instead.

`forge dev` runs the frontend in Docker, but Playwright needs a locally accessible dev server. Stop the Docker frontend container, then run `bun run dev` locally in `frontend/`. The backend can stay in Docker.

Add `test-results/` and `playwright-report/` to `.prettierignore` to prevent `forge check` from formatting generated HTML/JSON in those directories.

Run: `forge test` (backend must be running). Debug: `forge test --ui`. Headed: `forge test --headed`.

## Coverage Philosophy

- **Boundary values**: test edges (zero, one, max, max+1, empty, whitespace-only)
- **State transitions**: test each valid transition and at least one invalid one
- **Error variety**: test that the right error variant comes back, not just "it errors"
- **Combinatorics**: if two inputs interact (role + scope, tier + amount), test interesting combinations
- **Pure functions get the most tests**: cheap to write and often contain subtle logic
- **Handler tests cover integration**: auth, scope, dispatch, DB interaction. More expensive so focus on critical paths.

A well-tested module should make a reviewer think "I can't change this behavior without a test failing."

## Execution Order

1. Backend unit tests (`cargo test`)
2. `forge generate` if contract changed
3. Frontend lint/build checks
4. Playwright (`forge test --skip-backend`)
5. `forge check`

Do not claim completion if tests were not run, Playwright failed, runtime boot is blocked, or `forge check` still fails.
