# Testing Playbook

Use this when changing Forge handlers.

## Baseline expectation

Unit tests are the primary defense against regressions. The goal is wide case coverage that makes accidental breakage structurally difficult.

`forge check` runs as the absolute final step (after all tests, coverage, and refinement). See the required execution sequence at the bottom of this file.

For every behavior change, add:
- happy-path tests covering the main success scenarios
- failure-path tests (validation/authz/not found/conflict)
- boundary value tests (empty inputs, max lengths, zero amounts, negative values)
- edge case tests for any non-obvious logic branches

For dispatching flows, assert side effects.
For SQL-heavy logic, prefer real DB tests with `IsolatedTestDb`.

Think of tests as a specification: someone reading only the tests should understand what the function accepts, rejects, and guarantees.

## Test location

Tests live alongside the code they test using `#[cfg(test)] mod tests` at the bottom of the same file. This applies to handlers in `src/functions/`, helpers in `src/utils/`, and types in `src/schema/`. No separate `tests/` directory for unit tests.

## 0) Pure logic unit tests (preferred starting point)

Extract business logic into pure functions that don't need context or DB. These are fastest to write, run, and maintain.

```rust
// src/utils/pricing.rs
pub fn compute_discount(subtotal_cents: i64, tier: CustomerTier) -> i64 {
    match tier {
        CustomerTier::Standard => 0,
        CustomerTier::Premium => subtotal_cents / 10,
        CustomerTier::Enterprise => subtotal_cents / 5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_gets_no_discount() {
        assert_eq!(compute_discount(10000, CustomerTier::Standard), 0);
    }

    #[test]
    fn premium_gets_ten_percent() {
        assert_eq!(compute_discount(10000, CustomerTier::Premium), 1000);
    }

    #[test]
    fn zero_subtotal_returns_zero() {
        assert_eq!(compute_discount(0, CustomerTier::Premium), 0);
    }
}
```

```rust
// src/utils/validation.rs
pub fn normalized_title(raw: &str) -> Result<String> {
    let v = raw.trim();
    if v.is_empty() {
        return Err(ForgeError::Validation("Title cannot be empty".into()));
    }
    if v.len() > 120 {
        return Err(ForgeError::Validation("Title must be <= 120 chars".into()));
    }
    Ok(v.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_whitespace() {
        assert_eq!(normalized_title("  hello  ").unwrap(), "hello");
    }

    #[test]
    fn rejects_empty_string() {
        assert!(normalized_title("").is_err());
    }

    #[test]
    fn rejects_whitespace_only() {
        assert!(normalized_title("   ").is_err());
    }

    #[test]
    fn rejects_over_120_chars() {
        let long = "a".repeat(121);
        assert!(normalized_title(&long).is_err());
    }

    #[test]
    fn accepts_exactly_120_chars() {
        let exact = "a".repeat(120);
        assert!(normalized_title(&exact).is_ok());
    }
}
```

When a handler has non-trivial logic (calculations, transformations, decision trees), pull that logic into a pure function in `utils/` and test it exhaustively there. The handler becomes a thin adapter that calls the pure function.

## 1) Query tests

```rust
#[tokio::test]
async fn list_orders_requires_matching_scope() {
    let user_id = uuid::Uuid::new_v4();
    let ctx = forge::testing::TestQueryContext::builder()
        .as_user(user_id)
        .build();

    let result = list_orders(&ctx, ListOrdersInput { user_id: uuid::Uuid::new_v4() }).await;
    assert!(matches!(result, Err(ForgeError::Forbidden(_))));
}
```

## 2) Mutation tests

```rust
#[tokio::test]
async fn create_order_dispatches_confirmation_job() {
    let user_id = uuid::Uuid::new_v4();
    let ctx = forge::testing::TestMutationContext::builder()
        .as_user(user_id)
        .build();

    let result = create_order(&ctx, CreateOrderInput { user_id, total_cents: 5000 }).await;

    forge::assert_ok!(result);
    forge::assert_job_dispatched!(ctx, "send_order_email");
}
```

## 3) HTTP mock tests

```rust
#[tokio::test]
async fn charge_card_calls_provider_once() {
    let ctx = forge::testing::TestMutationContext::builder()
        .mock_http_json("api.stripe.com/*", serde_json::json!({ "id": "ch_123" }))
        .build();

    let _ = charge_card(&ctx, ChargeInput::default()).await;

    ctx.http().assert_called_times("api.stripe.com/*", 1);
}
```

## 4) DB integration test with isolation

```rust
async fn setup_db(name: &str) -> forge::testing::IsolatedTestDb {
    forge::testing::IsolatedTestDb::setup(
        name,
        &forge::get_internal_sql(),
        std::path::Path::new("migrations"),
    )
    .await
    .expect("db setup")
}
```

Use this for:
- SQL joins/CTEs that enforce ownership
- not-found semantics
- ordering/position logic
- migration-dependent behavior

## 5) Read consistency tests

If introducing replica-aware endpoints, test behavior assumptions:
- `consistent` endpoint returns latest state after mutation (verify `FunctionInfo.consistent == true`)
- eventual endpoint documents lag tolerance and still behaves safely
- column-aware invalidation: verify hot-path queries use explicit column lists, not `SELECT *`

## 6) Webhook tests

Minimum checks:
- valid signed request accepted
- duplicate idempotency key deduped
- malformed payload rejected correctly
- expected job dispatch occurs

## 7) Workflow/Job tests

- verify critical step progression
- verify failure path behavior
- verify dispatch/progress/cancellation logic where applicable

## 8) Frontend tests (when UI is changed)

- loading/error/stale state rendering
- keyboard navigation and focus behavior
- accessible names/labels for controls
- no manual refetch anti-pattern where reactivity exists
- add at least one basic Playwright integration test path and run it
- **all Playwright tests must pass before the task is considered complete** — if a test fails, fix the underlying issue and rerun until green. Do not report completion with failing tests.

## 9) Assertion helpers to prefer

- `assert_ok!`
- `assert_err_variant!`
- `assert_job_dispatched!`
- `assert_workflow_started!`
- `assert_http_called!`
- HTTP body/route assertions via mock APIs

## 10) Regression rule

Bug fix => add a regression test that fails before the fix and passes after.

## 11) Coverage philosophy

The delivery requirement is strict for changed modules:
- measure coverage with an actual tool (`cargo llvm-cov` preferred)
- require 100% line coverage for changed modules, or explicitly report blocker if measurement/tooling is unavailable

Coverage quality still matters beyond raw percentage. To get there:

- **Boundary values**: test the edges (zero, one, max, max+1, empty, whitespace-only).
- **State transitions**: if an entity moves through states, test each valid transition and at least one invalid one.
- **Error variety**: don't just test "it errors." Test that the *right* error variant comes back with the *right* message context.
- **Combinatorics**: if two inputs interact (role + scope, tier + amount), test the interesting combinations, not just the diagonal.
- **Pure functions get the most tests**: they're cheap to test and often contain the most subtle logic. Extract and cover them aggressively.
- **Handler tests cover integration**: auth, scope, dispatch, DB interaction. These are more expensive so focus on the critical paths.

A well-tested module should make a reviewer think "I can't change this behavior without a test failing."

## 12) Required execution sequence

1. Run backend/frontend tests for changed areas.
2. Generate and review coverage results; enforce 100% line coverage on changed modules.
3. If UI exists or changed, write/update Playwright integration tests and run them. **Fix any failures before proceeding** — loop until all tests pass.
4. As the absolute final step, run `forge check` from the app root. Fix all findings and rerun until fully clean. Nothing ships with unresolved findings or failing tests.
