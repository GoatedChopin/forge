# Examples

Runnable FORGE apps grouped by frontend framework. These directories are also the source of truth for `forge new --template`.

## Svelte

| Template id | Highlights | Description |
|-------------|------------|-------------|
| [with-svelte/minimal](with-svelte/minimal/) | Starter, Playwright | Smallest Svelte starter that passes `forge check` out of the box |
| [with-svelte/demo](with-svelte/demo/) | Queries, mutations, jobs, crons, workflows | Feature tour app used for end-to-end regression coverage |
| [with-svelte/realtime-todo-list](with-svelte/realtime-todo-list/) | Queries, mutations, reactivity | Real-time todo list with generated Svelte bindings |

## Dioxus

| Template id | Highlights | Description |
|-------------|------------|-------------|
| [with-dioxus/minimal](with-dioxus/minimal/) | Starter, Playwright | Smallest Dioxus starter with generated Rust bindings |
| [with-dioxus/demo](with-dioxus/demo/) | Queries, mutations, runtime bindings | Demo app for Rust-on-both-sides regression coverage |
| [with-dioxus/realtime-todo-list](with-dioxus/realtime-todo-list/) | Queries, mutations, reactivity | Real-time todo list with generated Dioxus bindings |

## Running an example

```bash
cd examples/with-svelte/realtime-todo-list
docker compose up
```

Backend at `http://localhost:9081`, frontend at `http://localhost:9080`.

Postgres binds to `127.0.0.1:5432` so you can connect from host tools (psql, rust-analyzer, sqlx) without exposing it to the network.

## Development

These examples are workspace members. Their `docker-compose.yml` files use the workspace root as build context so changes to `crates/` are picked up automatically.

All templates keep a checked-in `.sqlx/` directory. Templates with compile-time `sqlx::query*!` macros also keep the offline cache there. To refresh it after changing queries or migrations:

```bash
cd examples/with-svelte/realtime-todo-list
docker compose exec backend forge migrate prepare
```

To verify queries on host, make sure the db container is running and your editor picks up `DATABASE_URL` from the example's `.env`.
