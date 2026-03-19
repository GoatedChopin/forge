# SvelteKit Reference

Load when the repo has `frontend/package.json` with Svelte deps and generated bindings in `frontend/src/lib/forge/`.

## Generated Naming Patterns

For a backend function `list_todos`:

| Export | File | Returns | Usage |
|---|---|---|---|
| `listTodos()` | api.ts | `Promise<Vec<Todo>>` | One-shot RPC |
| `listTodosStore$()` | api.ts | `SubscriptionStore<Todo[]>` | Svelte store subscription |
| `listTodos$()` | reactive.svelte.ts | `ReactiveQuery<Todo[]>` | Svelte 5 runes reactive |

Mutations: `createTodo()` → `Promise<Todo>` (no Store$ variant).

Jobs: `trackExportUsers(args)` → `JobStore<Output>` (`track` + PascalCase).

Workflows: `trackOnboardUser(args)` → `WorkflowStore<Output>`.

Upload mutations (args contain `Upload` type): `call()` auto-detects `File`/`Blob` and routes to multipart endpoint. No special import needed.

## Store Contract

```typescript
interface SubscriptionResult<T> {
  loading: boolean;
  data: T | null;
  error: ForgeError | null;  // { code, message, details? }
  stale: boolean;
}

// Store methods
store.refetch()      // force re-fetch
store.unsubscribe()  // stop updates
store.reset()        // clear data, set loading
```

Auto-cleanup: subscriptions stop when all Svelte subscribers detach.

## Svelte 5 Runes

Use `$state`, `$derived`, `$effect` (sparingly). The `$`-suffixed reactive wrappers (`listTodos$()`) return `ReactiveQuery<T>` which is `$state`-backed.

```svelte
<script>
  import { listTodos$ } from '$lib/forge';
  const todos = listTodos$();
</script>

{#if todos.loading}
  <p>Loading...</p>
{:else if todos.error}
  <p>{todos.error.message}</p>
{:else}
  {#each todos.data ?? [] as todo}
    <p>{todo.title}</p>
  {/each}
{/if}
```

## App Setup

Root layout (`+layout.svelte`):
```svelte
<script>
  import { ForgeProvider } from '$lib/forge';
  import { getToken } from '$lib/auth';
</script>

<ForgeProvider url="http://localhost:8080" getToken={getToken}>
  {@render children()}
</ForgeProvider>
```

Disable SSR: `+layout.ts` with `export const ssr = false;`

## Auth Store

Generated `auth.svelte.ts` provides:
```typescript
class AuthStore {
  get token(): string | null
  get user(): User | null
  get isAuthenticated(): boolean
  setAuth(token: string, user: User): void   // persists to localStorage, reconnects SSE
  clearAuth(): void                           // clears localStorage, reconnects SSE
}
export const auth: AuthStore;
export function getToken(): string | null;
```

Use `auth.setAuth(token, user)` after login. Use `auth.clearAuth()` on logout. SSE reconnects automatically on auth change.

## Navigation

Use `resolve()` from `$app/paths` for all hrefs and goto calls:
```svelte
<script>
  import { resolve } from '$app/paths';
</script>
<a href={resolve('/settings')}>Settings</a>
```

## Route Structure

```
frontend/src/
  routes/
    +layout.svelte          # ForgeProvider, nav
    +layout.ts              # export const ssr = false
    +page.svelte            # landing
    (auth)/
      login/+page.svelte
      register/+page.svelte
    (app)/
      +layout.svelte        # auth guard
      dashboard/+page.svelte
  lib/
    components/             # reusable UI
    forge/                  # GENERATED, never edit
    auth.ts                 # auth store helpers
```

## Type Mappings

| Rust | TypeScript (args) | TypeScript (returns) |
|---|---|---|
| `String`, `Uuid` | `string` | `string` |
| `DateTime<Utc>` | `string` | `string` (ISO 8601) |
| `i32`, `i64`, `f32`, `f64` | `number` | `number` |
| `bool` | `boolean` | `boolean` |
| `Option<T>` | `T \| null` | `T \| null` |
| `Vec<T>` | `T[]` | `T[]` |
| `Upload` | `File \| Blob` | `File \| Blob` |
| `serde_json::Value` | `unknown` | `unknown` |
| `Bytes` | `Uint8Array` | `Blob` |

## SSE Internals

- Connects to `/_api/events?token=<jwt>`
- Receives `session_id` + `session_secret` on connect
- Subscriptions register via POST to `/_api/subscribe`
- Reconnection: exponential backoff with jitter, base 1s, cap 30s, max 10 attempts
- Events: `connected`, `update` (with target routing), `error`

## Datetime Utility

```typescript
import { dt } from '@forge-rs/svelte';
dt.now()                    // UTC ISO string
dt.parse(input)             // Date
dt.format(input, opts?)     // Intl.DateTimeFormat
dt.relative(input, base?)   // Intl.RelativeTimeFormat
```

## Common Mistakes

- Editing `$lib/forge/*` (overwritten on `forge generate`)
- Manual refetch loops instead of using `Store$` subscriptions
- Forgetting `ForgeProvider` in root layout
- Using `$effect` for data fetching instead of reactive stores
- Not handling `loading` and `error` states in UI
