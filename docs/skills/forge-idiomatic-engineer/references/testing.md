# Testing Reference

Effective testing focuses on identifying failure modes like expired tokens, deleted entities, and race conditions. Always prioritize testing failure cases over happy paths; if you only have time to implement one test, make it the one that verifies the system's error-handling logic.

## Backend Integration Testing

Use the specific `TestContext` builders to simulate various application states and identities without running the entire application.

| Scenario | Testing Strategy |
|---|---|
| **Authentication Boundary** | Use `TestQueryContext::minimal()` to verify that unauthenticated requests are rejected with 401 Unauthorized. |
| **Role Enforcement** | Build a context with a "User" role to verify that 403 Forbidden is returned for administrative endpoints. |
| **Missing Resource Handling** | Call your handler with a random UUID to verify that 404 Not Found is returned with proper context. |
| **Concurrency and Data Integrity** | Create an entity, delete it directly in the database, and then attempt to update it via a mutation to verify that 404 is returned correctly. |
| **Ownership Enforcements** | Create an entity as User A and attempt to view it as User B to ensure that the data is not leaked and returns 404. |
| **Job Idempotency** | Dispatch the same job twice with identical input and verify that the resulting state in the database reflects only a single processing operation. |

### Backend Implementation Pattern

```rust
// Use the builder to configure the test context exactly as needed.
let ctx = TestQueryContext::builder()
    .as_user(id)
    .with_role("admin")
    .build();

let result = my_handler(&ctx).await;
assert_err_variant!(result, ForgeError::NotFound(_));

// For database-dependent tests, use IsolatedTestDb to ensure each test case starts fresh.
let db = IsolatedTestDb::setup("test_name", ...).await.unwrap();
let ctx = TestMutationContext::builder()
    .with_pool(db.pool().clone())
    .build();
```

## Frontend E2E Testing (Playwright)

Forge frontends should be tested using real browser interactions. Do not call RPC handlers directly in your frontend tests.

| Scenario | Testing Strategy |
|---|---|
| **Session Expiry Recovery** | Clear `localStorage` and reload the page to verify that the user is correctly redirected to the login page. |
| **Real-Time Data Sync** | View an item list, perform a deletion in a separate tab, and verify that the list in the current tab updates automatically via SSE. |
| **Submission Throttling** | Rapidly click a "Save" button multiple times to verify that only one creation event is recorded on the backend. |
| **Loading and Disabled States** | Verify that action buttons are disabled while `mutation.loading` is true to prevent duplicate submissions. |
| **Offline Performance** | Simulate a network disconnection using Playwright's `setOffline(true)` to verify that an offline indicator is displayed. |
| **SSE Reconnection** | Forcefully abort the `_api/events` connection and verify that the client automatically re-establishes the stream and updates its state. |

### Frontend Implementation Pattern

```typescript
// Utilize the authedPage fixture to ensure the test starts with a valid session.
test('real-time data synchronization', async ({ authedPage: page }) => {
  await page.goto('/items');
  await waitForSSE(page); // Wait for the SSE stream to be established.

  // Simulate external data change and verify UI updates.
  await expect(page.getByTestId('item-count')).toHaveText('1', { timeout: 15000 });
});
```

## Testing Checklist

Before submitting your changes, ensure that you have verified the following:

- [ ] **Authentication**: Are 401 Unauthorized errors returned for missing credentials?
- [ ] **Authorization**: Are 403 Forbidden errors returned when a user lacks the necessary roles?
- [ ] **Data Integrity**: Does ownership enforcement prevent unauthorized users from seeing private data?
- [ ] **Error Clarity**: **MANDATE:** Return 404 Not Found for missing entities. See [Resilience Patterns](./resilience.md#2-database-and-data-integrity).
- [ ] **Background Tasks**: Are jobs idempotent and do they gracefully handle missing target entities?
- [ ] **User Experience**: Does the UI provide loading indicators and prevent duplicate submissions?
- [ ] **Reactivity**: Do live subscriptions stay synchronized after simulated network reconnections?
- [ ] **Failure Feedback**: Are user-facing error messages clear and actionable?
