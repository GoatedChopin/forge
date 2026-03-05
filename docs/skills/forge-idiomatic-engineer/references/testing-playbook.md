# Testing Playbook

Use this when changing Forge handlers.

## Baseline expectation

For every behavior change, add:
- one happy-path test
- one failure-path test (validation/authz/not found)

For dispatching flows, assert side effects.
For SQL-heavy logic, prefer real DB tests with `IsolatedTestDb`.

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

## 9) Assertion helpers to prefer

- `assert_ok!`
- `assert_err_variant!`
- `assert_job_dispatched!`
- `assert_workflow_started!`
- `assert_http_called!`
- HTTP body/route assertions via mock APIs

## 10) Regression rule

Bug fix => add a regression test that fails before the fix and passes after.
