# SvelteKit Reference

This guide provides specific instructions for projects using SvelteKit. You can identify these projects by the presence of a `frontend/package.json` file containing Svelte dependencies and a `frontend/src/lib/forge/` directory.

## Generated Naming Patterns

Forge generates both static and reactive APIs to handle different frontend use cases. For a backend query named `list_todos`:

| Export | File | Returns | Usage |
|---|---|---|---|
| `listTodos()` | `api.ts` | `Promise<T[]>` | Performing a one-shot, manual RPC call. |
| `listTodosStore$()` | `api.ts` | `SubscriptionStore<T[]>` | Standard Svelte store for classic applications. |
| `listTodos$()` | `reactive.svelte.ts` | `ReactiveQuery<T[]>` | Svelte 5 runes for high-performance reactive logic. |

### Mutations, Jobs, and Workflows
- **Mutations**: `createTodo()` returns a `Promise<T>` for manual execution.
- **Jobs**: `trackExportUsers(args)` returns a `JobStore<T>` to monitor background processing.
- **Workflows**: `trackOnboardUser(args)` returns a `WorkflowStore<T>` for tracking multi-step execution.
- **Uploads**: The generated `call()` function automatically detects `File` or `Blob` types and switches to `multipart/form-data`.

## Reactive Store and Rune Contract

Every reactive store or rune-based query follows a consistent interface. See [Frontend Playbook](../frontend.md#subscription-state-shape) for the standard state shape.

## Using Svelte 5 Runes

When using `listTodos$()`, you receive a `$state`-backed object. These are ideal for building highly reactive interfaces with minimal boilerplate.

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

## Application Configuration

- **Setup**: In your `+layout.svelte`, wrap your application in a `ForgeProvider`. Define a `getToken` function to manage authentication and use `onMutationError` for global error handling.
- **Disable SSR**: Always set `export const ssr = false;` in your root `+layout.ts`. Forge's SSE and reactive models are designed for client-side execution.

## Authentication and Session Management

Forge generates an `auth.svelte.ts` helper providing a global `auth` object for session management.

- **Session Handling**: Use `auth.setAuth(token, refresh, user)` to persist tokens and trigger SSE reconnection. Use `auth.clearAuth()` to sign out.
- **Auto-Refresh**: Call `auth.startRefreshLoop(url)` to automatically rotate tokens before they expire.
- **Automatic Reconnection**: `auth.setAuth()` and `auth.clearAuth()` automatically call `getForgeClient()?.reconnect()` to re-establish the SSE stream under the new identity. No manual remounting is needed in Svelte.


## Mutation Best Practices

Use `fireMutation` for simple operations. This utility automatically routes any errors to your global `onMutationError` handler, ensuring users are notified of failures without manual catch blocks.

```typescript
import { fireMutation } from '@forge-rs/svelte';
import { createTodo } from '$lib/forge';

fireMutation(createTodo, { title: 'New task' });
```

## Optimistic Mutations

Improve user experience by using `createOptimisticMutation` to apply local patches over a live subscription store. This provides instant visual feedback while the server processes the change.

```typescript
const todos = listTodosStore$();
const reorder = createOptimisticMutation(reorderTask, todos, (data, args) => 
  data.map(t => t.id === args.id ? { ...t, ...args } : t)
);
// Use reorder.data for UI display; fire applies instantly and auto-reverts on error or 3s TTL.
reorder.fire({ id, status: 'done' });
```

## Common Failure Cases

These are the patterns that most frequently cause infinite loops, memory leaks, or stale session bugs.

### Subscription inside `$derived` leaks unboundedly

Creating a store inside `$derived` opens a new SSE subscription on every recomputation and never closes the old one. After a few navigation cycles the server is flooded with dead connections.

```svelte
<!-- WRONG — new subscription every time id changes -->
<script>
  const item = $derived(getItemStore$({ id: currentId }));
</script>

<!-- RIGHT — use $effect to manage lifecycle manually -->
<script>
  let item = $state(null);
  let unsub;
  $effect(() => {
    unsub?.();
    const store = getItemStore$({ id: currentId });
    unsub = store.subscribe(v => { item = v; });
    return () => unsub?.();
  });
</script>
```

### Not calling `auth.setAuth()` / `auth.clearAuth()` after login/logout breaks SSE

`ForgeProvider` gets the token via your `getToken` callback, but the SSE connection only reconnects when explicitly told to. `auth.setAuth()` and `auth.clearAuth()` handle this automatically — they update the stored token AND call `getForgeClient()?.reconnect()` to re-establish the stream.

If you store tokens manually (e.g. direct `localStorage.setItem`) and skip these methods, the SSE stream keeps running under the old identity and returns stale or wrong data.

```svelte
<!-- WRONG — SSE never reconnects under new user -->
<script>
  async function login(email, password) {
    const result = await signIn({ email, password });
    localStorage.setItem('token', result.access_token); // bypass auth store
  }
</script>

<!-- RIGHT — reconnect happens inside setAuth -->
<script>
  import { auth } from '$lib/forge/auth.svelte';
  async function login(email, password) {
    const result = await signIn({ email, password });
    auth.setAuth(result.access_token, result.refresh_token, result.user);
  }
</script>
```

### Mutation errors silently swallowed without `onMutationError`

Calling `createTodo(args)` directly swallows errors unless you wrap it in a try/catch. `fireMutation` routes errors to the global handler automatically.

```typescript
// WRONG — errors lost silently
createTodo({ title });

// RIGHT — routes to onMutationError
fireMutation(createTodo, { title });

// also RIGHT if you need local handling
try {
    await createTodo({ title });
} catch (e) {
    showToast(e.message);
}
```

### `ssr = false` missing causes hydration failures

Forge's SSE and reactive stores require a browser environment. Without this, SvelteKit will attempt server-side rendering and crash on `EventSource` or `localStorage`.

```typescript
// frontend/src/routes/+layout.ts — REQUIRED
export const ssr = false;
```

## Critical Constraints

- **Do not edit generated files**: Never modify files in `$lib/forge/*`.
- **Subscription leaks in runes**: Never create a store inside a `$derived` rune — use `$effect` with `unsubscribe()` cleanup when parameters change.
- **Avoid refetch loops**: Rely on reactive stores instead of manual polling to reduce server load.
- **Handle all mutation errors**: Use the global `onMutationError` to ensure users receive feedback when backend operations fail.
