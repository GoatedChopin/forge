# Dioxus Reference

Load when the repo has `frontend/Cargo.toml` with `dioxus` dep, `Dioxus.toml`, and generated bindings in `frontend/src/forge/`.

## Generated Naming Patterns

For a backend function `get_user(ctx, id: Uuid)`:

| Export | Returns | Usage |
|---|---|---|
| `get_user(client, args)` | `Result<User, ForgeClientError>` | Async fn for manual calls |
| `use_get_user(args)` | `QueryState<User>` | One-shot query hook |
| `use_get_user_live(args)` | `SubscriptionState<User>` | Live subscription hook |

Mutations: `use_create_user()` → `Mutation<CreateUserParams, User>`. Call via `mutation.call(params).await`.

Jobs: `use_send_email(args)` → `JobExecutionState<()>`.

Workflows: `use_onboard_user(args)` → `WorkflowExecutionState<User>`.

## State Types

```rust
pub struct QueryState<T> {
    pub loading: bool,
    pub data: Option<T>,
    pub error: Option<ForgeError>,
}

pub struct SubscriptionState<T> {
    pub loading: bool,
    pub data: Option<T>,
    pub error: Option<ForgeError>,
    pub stale: bool,
    pub connection_state: ConnectionState,
}

// ConnectionState: Disconnected | Connecting | Connected
```

## Params Generation

Multiple args → `{PascalCase}Params` struct with builder:
```rust
let params = GetUserParams::new("user-id");
let params = UpdateUserParams::new("id").email("new@example.com");
```

Single custom Args/Input struct → passes through directly, no Params generated.

## App Setup

```rust
use crate::forge::*;

fn main() {
    dioxus::launch(app);
}

fn app() -> Element {
    rsx! {
        ForgeProvider {
            url: "http://localhost:8080",
            // get_token: optional auth callback
            Router::<Route> {}
        }
    }
}
```

## Component Pattern

```rust
#[component]
fn TodoList() -> Element {
    let todos = use_list_todos_live();

    rsx! {
        if todos.loading {
            p { "Loading..." }
        } else if let Some(error) = &todos.error {
            p { "Error: {error.message}" }
        } else if let Some(data) = &todos.data {
            for todo in data {
                p { "{todo.title}" }
            }
        }
    }
}
```

## Mutation Pattern

```rust
#[component]
fn CreateTodo() -> Element {
    let mut title = use_signal(|| String::new());
    let create = use_create_todo();

    rsx! {
        input {
            value: "{title}",
            oninput: move |e| title.set(e.value())
        }
        button {
            onclick: move |_| {
                let t = title.read().clone();
                spawn(async move {
                    create.call(CreateTodoParams::new(t)).await;
                });
            },
            "Add"
        }
    }
}
```

## Type Mappings

| Rust backend | Dioxus frontend |
|---|---|
| `String`, `Uuid` | `String` |
| `DateTime<Utc>` | `String` (ISO 8601) |
| `i32`, `i64`, `f32`, `f64` | same |
| `bool` | `bool` |
| `Option<T>` | `Option<T>` |
| `Vec<T>` | `Vec<T>` |
| `Upload` | `ForgeUpload` |
| `serde_json::Value` | `JsonValue` |
| `Bytes` | `Vec<u8>` |

## Re-exports

Generated `mod.rs` re-exports everything from `forge_dioxus`:
```rust
pub use forge_dioxus::{
    ConnectionState, ForgeClient, ForgeClientConfig, ForgeClientError,
    ForgeError, ForgeProvider, ForgeUpload, JobExecutionState, Mutation,
    QueryState, SubscriptionState, WorkflowExecutionState,
    use_connection_state, use_forge_client, use_forge_query,
    use_forge_subscription, use_forge_mutation, use_forge_job, use_forge_workflow,
};
```

Import via `use crate::forge::*;` in components.

## Subscription Internals

- SSE-based, same as SvelteKit
- Reconnection: exponential backoff 1s * 2^min(attempts, 4), max 10 attempts
- Data updates debounced at 120ms to coalesce rapid pushes
- `reconnect_nonce` signal triggers re-render on reconnect

## Common Mistakes

- Editing `frontend/src/forge/*` (overwritten by `forge generate`)
- Treating Dioxus like SvelteKit (different reactive model: signals, not stores)
- Adding manual refetch logic instead of using `_live` hooks
- Forgetting `ForgeProvider` at the app root
- Using `use_effect` for data fetching instead of generated hooks
- Doing frontend work before backend contract is confirmed
