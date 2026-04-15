# Dioxus Reference

This reference is specifically for projects using Dioxus as their frontend framework. You can identify these projects by the presence of a `frontend/Cargo.toml` file containing a `dioxus` dependency and a `frontend/src/forge/` directory.

## Generated Naming Patterns

Forge generates type-safe hooks for your backend handlers. For a query named `get_user(ctx, id: Uuid)`, the following functions are generated:

| Export | Returns | Recommended Usage |
|---|---|---|
| `get_user(client, args)` | `Result<T, E>` | Performing a one-shot manual async call. |
| `use_get_user(args)` | `QueryState<T>` | Fetching data once using a Dioxus hook. |
| `use_get_user_live(args)` | `SubscriptionState<T>` | Creating a live SSE subscription for real-time updates. |

### Mutations, Jobs, and Workflows
- **Mutations**: `use_create_user()` returns a `Mutation<P, T>` handle. You can execute it using `.call(params).await` for manual control or `.fire(params)` for a fire-and-forget style.
- **Jobs**: `use_send_email(args)` returns a `JobExecutionState<T>` to track the status of a background task.
- **Workflows**: `use_onboard_user(args)` returns a `WorkflowExecutionState<T>` to manage a durable multi-step process.

## Subscription State Structure

The `SubscriptionState<T>` struct provides all the information needed to manage the UI lifecycle. See [Frontend Playbook](../frontend.md#subscription-state-shape) for details on the common state shape.

## Application Setup

- **Authentication**: Use `ForgeAuthProvider` to manage token storage (via `localStorage` or filesystem), handle token refreshes, and automatically recover from 401 Unauthorized errors.
- **Providers**: Use the root `ForgeProvider` for unauthenticated applications or `ForgeAuthProvider` for those requiring user sessions. Access the auth state using `use_forge_auth()`.

## Mutation Implementation Patterns

- **Avoid manual cloning**: Modern Forge hooks provide the `.fire(args)` method, which handles internal spawning and clones automatically. This routes any errors to the global `on_mutation_error` handler.
- **Explicit error handling**: Use `.fire_with(args, |err| ...)` when you need to handle errors locally within a specific component.
- **Avoid signal reads in async closures**: Do not read signals inside a `spawn` block, as this can cause a panic if the component has been unmounted. Clone any required values before entering the async block.

## Optimistic Updates

Use `use_optimistic` to apply local patches to a live subscription store before the server confirms the change. This improves perceived performance.

```rust
let tasks_sub = use_list_tasks_live_signal();
let reorder = use_optimistic(use_reorder_task(), tasks_sub, |tasks, args| {
    // Apply patch logic and return a modified Vec
});
// Always read from reorder.data(), as it contains the merged server and local state.
reorder.fire(args);
```

The optimistic state automatically reverts if the server returns an error or if the default 3-second TTL expires without confirmation.

## Authentication State and Keyed Remounting

**MANDATE:** Use `use_auth_key()` to force a complete remount whenever the authentication state changes to prevent session mismatches. See [Frontend Playbook](../frontend.md#authentication-and-session-management) for the underlying principle.

```rust
fn AppShell() -> Element {
    let auth_key = use_auth_key();
    rsx! { 
        main { 
            key: "{auth_key}", // Triggers a remount on auth change
            Router::<Route> {} 
        } 
    }
}
```

## Platform-Specific Requirements

- **WASM (Web)**: Uses the browser's native `EventSource`. The authentication token is passed via a URL query parameter (`?token=...`).
- **Native Applications**: Uses `reqwest-eventsource`, passing the token via the standard `Authorization` header. You must ensure `reqwest` is configured with `rustls-tls` for non-WASM targets.

## Common Failure Cases

These are the patterns most likely to cause a build error, panic, or silent bug. Fix the root cause — don't work around them.

### Signal read inside `spawn` panics at runtime

Reading a signal inside a `spawn` block will panic if the component unmounts before the future resolves. Clone the value before entering the async block.

```rust
// WRONG — panics if component unmounts
spawn(async move {
    let id = some_signal.read().id; // panic here
    create_item.call(Args { id }).await;
});

// RIGHT — clone before the block
let id = some_signal.read().id;
spawn(async move {
    create_item.call(Args { id }).await;
});
```

Using `.fire(args)` avoids this entirely — prefer it over manual `spawn` for mutations.

### Missing `key: "{auth_key}"` causes session bleed

Without keyed remounting, a new user's session reuses stores from the previous user, serving stale or wrong data. This is not a visual glitch — it's a data leak.

```rust
// WRONG — no remount on auth change
fn App() -> Element {
    rsx! { Router::<Route> {} }
}

// RIGHT
fn App() -> Element {
    let auth_key = use_auth_key();
    rsx! { main { key: "{auth_key}", Router::<Route> {} } }
}
```

### `use_*_live` inside a loop creates N subscriptions that don't clean up

Each call to `use_get_item_live(id)` opens an SSE subscription. Calling it inside `for id in ids` creates unbounded subscriptions. Fetch a collection-level query instead.

```rust
// WRONG
for id in &ids {
    let item = use_get_item_live(GetItemArgs { id: *id }); // N subscriptions
}

// RIGHT
let items = use_list_items_live(ListItemsArgs { ids: ids.clone() });
```

### Optimistic state not read from `.data()`

`reorder.fire(args)` applies the local patch, but you must read from `reorder.data()` not the original subscription to see the merged state.

```rust
// WRONG — shows unpatched data
let tasks = use_list_tasks_live(args);
let reorder = use_optimistic(use_reorder_task(), tasks, |t, a| { ... });
rsx! { for task in tasks.read().data { ... } } // reads original, not patched

// RIGHT
rsx! { for task in reorder.data() { ... } }
```

## Critical Constraints

- **Do not edit generated code**: See [Pitfalls](../pitfalls.md#1-generated-code).
- **Avoid manual refetch loops**: See [Pitfalls](../pitfalls.md#6-frontend).
- **Do not use effects for data fetching**: This is a common source of race conditions and memory leaks.
- **Authenticated client for refresh**: Never use an authenticated `ForgeClient` to perform a token refresh; the refresh handler requires an anonymous client to avoid rejection due to an expired token header.
