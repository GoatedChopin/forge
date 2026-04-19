# Frontend Playbook

Cross-framework frontend rules. For SvelteKit or Dioxus specifics, see `frontend/svelte.md` / `frontend/dioxus.md`.

## Core Rules
- Backend is the source of truth — always define data contracts in Rust first.
- Run `forge generate` after every backend change. Never edit generated files.

## Reactivity Model
- The backend query is the unit of reactivity. Subscribing registers the query with the server.
- On DB change → server re-executes, hashes the result, pushes via SSE only when the hash differs.
- No manual refetching, WebSockets, or cache invalidation. The Reactor handles it.

## Subscription State Shape

All subscription stores/hooks expose the same shape:

```typescript
{
  loading: boolean,          // true until first packet
  data: T | null,            // current query result
  error: ForgeError | null,  // { code, message, details? }
  stale: boolean             // true while reconnecting
}
```

`ForgeError` is `{ code: string; message: string; details?: Record<string, unknown> }`. Branch on `error.code` (e.g. `"NOT_FOUND"`, `"UNAUTHORIZED"`, `"RATE_LIMITED"`), never on `error.message`. See [API Reference](./api.md#forgeerror-variants).

### Specialised statuses
- **Jobs / workflows**: add `jobId`, `status`, `progress`, `output`.
- **Terminal errors**: `blocked_version`, `signature_mismatch` indicate operational mismatch — surface as critical.

## Authentication and Session Management
- Set `access_token_ttl` and `refresh_token_ttl` in `forge.toml`.
- Issue tokens via `ctx.issue_token_pair()` on the backend. See [Patterns](./patterns.md#2-authentication-and-authorization).
- The SSE connection must reconnect whenever the principal changes. The generated auth helpers (`auth.setAuth` in Svelte, `ForgeAuthProvider` in Dioxus) do this — don't hand-roll token storage.
- Persist tokens in `localStorage` (wrap in try/catch — private-mode browsers block it) and run a refresh loop.
- **Refresh calls must be anonymous**: the built-in `refresh_token` provider handles this. Passing an expired token on a refresh call gets rejected.

## Error Handling
- Programmatic branching → `error.code`. Display text → `error.message`.
- Rate limits → read `details.retry_after_secs` for a cooldown timer.
- SSE reconnection is automatic with exponential backoff. Never write your own retry loop for subscriptions.
- Route mutation errors to a global `onMutationError` handler (Svelte) or `on_mutation_error` (Dioxus). Don't rely on per-call try/catch unless you need local UX.

## File Uploads
- Mutations with `Upload`, `Vec<Upload>`, or `Option<Upload>` params auto-switch to `multipart/form-data`. The generated client sends `File` / `Blob` correctly.
- Defaults: 20 MB body, 1 MB per JSON field, 255-char field names.
- For files much larger than 20 MB, generate presigned S3/GCS URLs server-side and upload directly from the browser. See [Recipes](./recipes.md#5-file-upload-to-s3-compatible-storage) for the server side.

## Signals (analytics + diagnostics)
`ForgeProvider` initialises the tracker automatically.

- Auto-captures SPA page views, frontend errors, and the `x-correlation-id` header linking RPC calls.
- Manual: `track(event, props)`, `identify(userId, traits)`, `captureError(err, context)`.
- Privacy: daily-rotating hashed visitor ID, no cookies. Set `anonymize_ip = true` in `forge.toml` to hash IPs.
- Batched up to 20 events, flushed on idle or `visibilitychange`.

## Forbidden

- Manual `refetch()` loops on live subscriptions — traffic, flicker, stale data.
- Client-side-only authorisation — always re-check on the backend.
- Skipping `forge generate` after backend changes — guarantees runtime type mismatches.
- Hand-rolling auth storage around the generated helper — breaks SSE reconnection on login/logout.
