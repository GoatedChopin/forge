# Frontend Playbook

This reference outlines the core principles and architectural patterns for building frontends with Forge. For framework-specific implementation details, refer to the SvelteKit (`svelte.md`) and Dioxus (`dioxus.md`) guides.

## Core Development Rules

- **The backend is the source of truth**: Always define your data contracts on the backend first. This ensures that your frontend types are derived from a single, consistent source.
- **Strict development workflow**: **MANDATE:** Always run `forge generate` after backend changes. Never edit generated files. See [Pitfalls](./pitfalls.md#1-generated-code).

## Reactivity Model

Forge uses a server-driven reactivity model to keep the frontend in sync with the database.

- **Query-based subscriptions**: The backend query is the fundamental unit of reactivity. When you subscribe to a query, the server monitors relevant database changes.
- **Server-driven updates**: When the database changes, the server re-executes the query, hashes the result, and pushes updates to the client via Server-Sent Events (SSE) only if the data has actually changed.
- **No manual refetching**: You do not need to implement manual refetching, WebSockets, or complex cache invalidation logic. The framework handles data synchronization automatically.

## Subscription State Shape

All subscription stores and hooks return a consistent state object to help you manage the UI lifecycle.

```typescript
{
  loading: boolean,      // True until the first data packet is received.
  data: T | null,        // The current result of the query.
  error: Error | null,   // Contains the last error encountered during the subscription.
  stale: boolean         // True if the client is currently disconnected and attempting to reconnect.
}
```

### Specialized Statuses
- **Jobs and Workflows**: These states include additional fields such as `jobId`, `status`, `progress`, and `output`.
- **Terminal Errors**: Statuses like `blocked_version` or `signature_mismatch` indicate an operational error (e.g., a version mismatch between frontend and backend). These should be displayed as critical system errors.

## Authentication and Session Management

- **Configuration**: Set your `access_token_ttl` and `refresh_token_ttl` in `forge.toml` to control session duration.
- **Token Issuance**: Use `ctx.issue_token_pair()` on the backend to generate JWTs. See [Patterns Reference](./patterns.md#2-authentication-and-authorization).
- **Session Continuity**: The client must reconnect its SSE stream whenever the authentication principal changes (e.g., after a login or logout). Mismatching a session with a new user will cause errors.

- **Persistence**: Store tokens and user information in `localStorage` and implement a periodic refresh loop to maintain the session.

## Error Handling Logic

- **Structured Errors**: Forge returns errors in a `{ code, message, details? }` format.
- **Control Flow**: Use the error `code` (e.g., `NOT_FOUND`, `RATE_LIMITED`) for programmatic logic and the `message` for user-facing display.
- **Automatic Cooldowns**: Use `details.retry_after_secs` to implement UI-level cooldown timers for rate-limited operations.
- **Managed Retries**: The client library automatically handles SSE reconnection with exponential backoff. Do not implement custom retry loops for subscriptions.

## File Uploads

- **Multipart Support**: Mutations using `Upload` types automatically switch to `multipart/form-data`.
- **Supported Types**: You can use `Upload`, `Vec<Upload>`, or `Option<Upload>` in your mutation parameters.
- **Default Constraints**: The maximum body size is 20MB by default, but individual JSON fields are limited to 1MB. Field names must be under 255 characters.
- **Large Files**: For files significantly larger than 20MB, use the backend to generate presigned S3/GCS URLs and upload directly from the browser.

## Signals (Analytics and Diagnostics)

Signals are automatically initialized by the `ForgeProvider` to provide observability without manual instrumentation.

- **Auto-capture**: Forge automatically tracks SPA page views, frontend errors, Web Vitals (LCP, CLS, INP, FCP, TTFB, navigation timing, long tasks), online/offline transitions, and correlation IDs for RPC calls.
- **Manual Tracking**: Use `track()`, `identify()`, `captureError()`, and `vital()` to record custom events, user traits, or performance metrics.
- **Privacy Compliance**: Forge uses daily-rotating hashed visitor IDs and does not set tracking cookies. Set `anonymize_ip = true` to drop raw IPs. The SDK honors browser `DNT: 1` and `Sec-GPC: 1` out of the box (`respectDnt: true`) and the server short-circuits signal ingestion when those headers arrive. Crash reports (`/signal/report`) still land so production errors from DNT users don't disappear.
- **Event Batching**: Signals are batched (up to 20 events), persisted to `localStorage` so they survive reloads, and flushed on an interval, on `visibilitychange` / `pagehide`, or when the network returns online.
- **Client Config**: `SignalsConfig` on the provider: `enabled`, `autoPageViews`, `autoCaptureErrors`, `autoWebVitals`, `autoNetworkEvents`, `respectDnt`, `persistQueue`, `flushInterval`, `maxBatchSize`.

## Forbidden Practices

- **Manual refetch loops**: These cause unnecessary server load and UI flickering; use live subscriptions instead.
- **Client-side only auth**: Never rely on frontend permission checks; always validate authorization on the backend.
- **Skipping generation**: Failing to run `forge generate` after backend changes will lead to runtime type mismatches.
