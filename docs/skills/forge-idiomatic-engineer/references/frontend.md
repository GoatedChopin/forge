# Frontend Playbook

Shared principles for building Forge frontends. Framework-specific details live in `frontend/svelte.md` and `frontend/dioxus.md`.

## Core Rule

Backend is the source of truth. Frontend is a projection of the backend contract.

1. Finish backend handlers + tests
2. Run `forge generate`
3. Wire frontend against generated bindings
4. Never hand-edit generated files

## Generated Files

SvelteKit: `frontend/src/lib/forge/` (types.ts, api.ts, stores.ts, reactive.svelte.ts, auth.svelte.ts, index.ts)

Dioxus: `frontend/src/forge/` (types.rs, api.rs, mod.rs)

These are overwritten on every `forge generate`. Treat as read-only.

## Reactivity Model

Forge uses query-based subscriptions, not table-based. The backend query is the unit of reactivity. When underlying tables change (via LISTEN/NOTIFY), the server re-executes the query, hashes the result, and only pushes over SSE if data actually differs.

What this means for frontends:
- No manual refetch loops. Subscribe and the data stays current.
- No WebSocket management. SSE is handled by the generated client.
- No cache invalidation logic. The server decides when to push.

## State Shape

All subscription stores expose:

```
loading: bool       // true until first data arrives
data: T | null      // the result
error: Error | null // last error
stale: bool         // reserved for reconnection status
```

Jobs add: `jobId`, `status`, `progress`, `message`, `output`, `error`.
Workflows add: `workflowId`, `status`, `step`, `steps[]`, `output`, `error`.

## Auth Pattern

1. Backend: `[auth]` config in `forge.toml` + public `register`/`login`/`refresh` mutations using `ctx.issue_token_pair()`
2. Frontend: auth layer persists tokens + user/viewer to localStorage, provides token to the client, runs periodic refresh
3. On auth change: client reconnects SSE automatically (subscriptions re-register with new identity)

Both SvelteKit (`auth.setAuth(token, refreshToken, user)`) and Dioxus (`auth.login_with_viewer(token, refreshToken, &viewer)`) store the authenticated user alongside tokens. This avoids apps needing their own user persistence layer.

Protected endpoints require `Authorization: Bearer <token>`. Public endpoints (`#[forge::query(public)]`) skip auth.

## Error Handling

Backend errors serialize to `{ code, message, details? }`. Frontend gets typed `ForgeError` / `ForgeClientError`.

Pattern: check `error.code` for control flow (`NOT_FOUND`, `VALIDATION_ERROR`, `UNAUTHORIZED`, `RATE_LIMITED`), show `error.message` for user display.

Rate limit errors include `details.retry_after_secs`.

## File Uploads

Mutations with `Upload`-typed parameters automatically use multipart/form-data. The generated client detects `File`/`Blob` args and routes to `/_api/rpc/{fn}/upload`.

SvelteKit: pass `File` from `<input>` directly. Dioxus: use `ForgeUpload` type.

Limits: 10 MB per file, 20 fields max, 1 MB max JSON field.

## Performance

- Use `cache = "30s"` on queries that tolerate stale reads
- Paginate with `LIMIT`/`OFFSET` or cursor-based pagination
- Avoid `SELECT *` in queries that don't need all columns (defeats column-aware invalidation)
- Jobs for anything > 100ms that doesn't need a synchronous response

## Accessibility

- Semantic HTML: `<main>`, `<nav>`, `<header>`, headings in order
- Form labels on every input
- Loading/error states visible and announced (`aria-live="polite"`)
- Keyboard navigation works
- `prefers-reduced-motion` respected
- WCAG AA contrast minimum

## Never Do

- Edit files in `frontend/src/lib/forge/` or `frontend/src/forge/`
- Manual refetch loops (use Forge reactivity)
- Client-side auth enforcement without backend validation
- `SELECT *` subscriptions that could be column-specific
- Skip `forge generate` after backend contract changes
