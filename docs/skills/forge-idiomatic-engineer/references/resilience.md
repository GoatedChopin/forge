# Resilience Patterns

Forge applications must be designed to survive data deletion, expired tokens, race conditions, and network interruptions. Always assume that the system will fail and implement graceful degradation to inform the user.

## 1. Authentication and Session Resilience

Ensuring session continuity requires both backend validation and proactive frontend recovery.

| Scenario | Backend Strategy | Frontend Strategy |
|---|---|---|
| **User Deleted Mid-Session** | Use `fetch_optional` to return `Unauthorized` if the principal no longer exists. | Call `auth.clearAuth()` and redirect the user to a "Account Deleted" page. |
| **Refresh Token Revoked** | Return `Unauthorized` on the refresh call. | Catch the failure, clear local storage, and redirect to login with a reason parameter. |
| **Roles Changed Mid-Session** | Re-verify critical permissions in the database; do not rely solely on the JWT roles. | Handle 403 Forbidden errors by refreshing the local authentication state. |
| **JWT Expires Mid-Mutation** | The framework validates at the start; ensure the entire operation is transactional. | Catch 401 errors, attempt one automatic `auth.tryRefresh()`, and retry the operation once. |
| **Multi-Tab Logout** | Not applicable. | Listen for `storage` events or use a `BroadcastChannel` to sync logout state across tabs. |
| **Long Tab Hibernation** | Not applicable. | Use the `visibilitychange` event to trigger `auth.tryRefresh()` and logout if the session is unrecoverable. |

## 2. Database and Data Integrity

Maintain data integrity by handling concurrent modifications and ensuring consistent reads.

| Scenario | Backend Strategy | Frontend Strategy |
|---|---|---|
| **Entity Deleted During Action** | Return a 404 with specific context. | Remove the item from local state and show a descriptive toast message. |
| **Concurrent Update Conflict** | Use a `version` column and return a 409 Conflict error if the version has changed. | Show a diff to the user or force a data refresh to resolve the conflict. |
| **Foreign Key Target Deleted** | Explicitly handle `is_foreign_key_violation` errors. | Inform the user that the parent entity no longer exists. |
| **Connection Pool Exhaustion** | Set a low `timeout_secs` to fail fast and use isolated pools for different services. | Show a "System busy, please try again" message. |
| **Read Replica Lag** | Use `#[query(consistent)]` to force a read from the primary database after a write. | Return the updated data directly from the mutation response. |

## 3. SSE and Realtime Resilience

Forge's Server-Sent Events (SSE) stream is designed to be self-healing.

- **Automatic Reconnection**: Subscriptions automatically re-register on reconnect. Do not implement manual client-side caching for reactive data.
- **Access Control**: The Reactor re-evaluates permissions with the current authentication state; unauthorized data is never pushed to the client.
- **State Over Deltas**: Treat every SSE push as the complete current state rather than a delta to avoid synchronization issues.
- **Hibernation Handling**: Reconnect the SSE client on `visibilitychange` to ensure the data is fresh when the user returns.

## 4. Job and Workflow Resilience

Jobs and workflows must be idempotent to survive crashes and retries.

- **Existence Checks**: Always check that target entities still exist at the start of every job or workflow step. Exit gracefully if they are missing.
- **Idempotency**: Annotate jobs with `idempotent(key = "...")` to prevent duplicate processing if a worker crashes before recording completion.
- **State Verification**: Re-verify all business preconditions at every step of a long-running workflow.
- **Workflow Stalls**: Always use `wait_for_event` with a defined timeout to prevent workflows from hanging indefinitely.
- **Transactional Dispatch**: See [Patterns Reference](./patterns.md#background-job-implementation).

## 5. Client-Side Resilience

- **Navigation Protection**: Use `beforeNavigate` to warn users if they are navigating away while a mutation is in progress.
- **Double-Click Prevention**: Disable submission buttons while `mutation.loading` is true.
- **Storage Failover**: Wrap all `localStorage` access in try/catch blocks and fall back to `sessionStorage` or in-memory storage if disk access is restricted.
- **Payload Validation**: Validate file sizes and request formats client-side before sending to provide instant feedback and reduce server load.

## Resilience Checklist

Use this checklist during every feature implementation to ensure your code handles common failure modes:

- [ ] Does the handler survive if authentication is revoked during execution?
- [ ] Is there a check for target entity existence before performing an action?
- [ ] Are concurrent modifications handled using version checks or etags?
- [ ] Does the UI handle network drops gracefully during an operation?
- [ ] Is double-clicking prevented on all action buttons?
- [ ] Does the frontend handle long-term hibernation and token expiry?
- [ ] Are background jobs idempotent to handle retries after a crash?
- [ ] Does the user receive clear, actionable feedback for every failure mode?
- [ ] Are data subscriptions correctly re-established after a reconnection?
