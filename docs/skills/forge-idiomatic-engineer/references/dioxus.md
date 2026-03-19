# Frontend Dioxus Playbook

Load this reference only when the repo is using the Dioxus target (`frontend/Cargo.toml`, `Dioxus.toml`, generated bindings in `frontend/src/forge`).

Forge's Dioxus target keeps the same backend contract as SvelteKit, but projects it into Rust bindings and hooks instead of TypeScript stores.

## 1) Delivery and Tooling Order

Keep the same backend-first workflow:

1. backend correctness and tests
2. `forge generate`
3. thin Dioxus integration
4. runtime verification
5. `forge check` last

Do not hand-write fake bindings under `frontend/src/forge`. Regenerate them from Rust source.

## 2) Discovery

Read only what you need:

- `frontend/Cargo.toml`
- `frontend/Dioxus.toml` if present
- `frontend/src/main.rs`
- the generated `frontend/src/forge/mod.rs` only to confirm names and exports
- the specific component/module you will change

Typical generated output:

- `frontend/src/forge/types.rs`
- `frontend/src/forge/api.rs`
- `frontend/src/forge/mod.rs`

## 3) Quality Gates

Prefer the project's real Rust-side checks:

```bash
cargo fmt --manifest-path frontend/Cargo.toml
cargo check --manifest-path frontend/Cargo.toml
```

If `dx` is part of the workflow, use it for final build verification:

```bash
cd frontend
dx build --web --release
```

## 4) App Structure

Keep `main.rs` thin and push UI into small components/modules.

```text
frontend/
├── Cargo.toml
├── Dioxus.toml
└── src/
    ├── main.rs              # App bootstrap + ForgeProvider
    ├── components/          # Handwritten UI
    ├── routes/              # Optional route modules if the app organizes that way
    └── forge/               # Generated bindings (DO NOT EDIT)
```

## 5) Provider and Hooks

Use `ForgeProvider` near the app root and keep generated hook usage close to the component that needs it.

```rust
use dioxus::prelude::*;
use forge_dioxus::ForgeProvider;

fn api_url() -> &'static str {
    option_env!("FORGE_API_URL").unwrap_or("http://localhost:8080")
}

#[component]
fn App() -> Element {
    rsx! {
        ForgeProvider {
            url: api_url().to_string(),
            TodoScreen {}
        }
    }
}
```

Generated query bindings typically expose:

- `list_todos(&ForgeClient, ...)` for one-shot calls
- `use_list_todos(...)` for the default live subscription state
- `use_list_todos_signal(...)` when you need the live subscription signal
- `use_list_todos_query(...)` for an explicit one-shot hook
- `use_create_todo()` style hooks for client-bound mutations

Example:

```rust
use dioxus::prelude::*;

use crate::forge::{CreateTodoInput, use_create_todo, use_list_todos};

#[component]
fn TodoScreen() -> Element {
    let create_todo = use_create_todo();
    let todos = use_list_todos();

    rsx! {
        button {
            onclick: move |_| {
                let create_todo = create_todo.clone();
                spawn(async move {
                    let _ = create_todo(CreateTodoInput::new("Learn Forge")).await;
                });
            },
            "Add todo"
        }

        ul {
            for todo in todos.data.clone().unwrap_or_default() {
                li { "{todo.title}" }
            }
        }
    }
}
```

## 6) State and Component Rules

- Prefer local `use_signal` state for component-local concerns.
- Prefer generated Forge subscription hooks over manual polling or refetch loops.
- Keep components focused; extract subcomponents before `main.rs` becomes a wall of `rsx!`.
- Use plain Rust helpers for formatting and transformation instead of burying logic in JSX-sized inline expressions.

## 7) Common Mistakes

- Editing `frontend/src/forge/*` by hand instead of running `forge generate`
- Treating Dioxus projects like SvelteKit projects with `package.json`-based assumptions
- Adding manual refetches after mutations when subscriptions already cover invalidation
- Doing frontend work before confirming the backend contract and handler registration
