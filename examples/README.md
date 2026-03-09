# Examples

Working apps built with FORGE. Each one runs with docker compose.

| Example | Highlights | Description |
|---------|------------|-------------|
| [todo](todo/) | Queries, Mutations | Minimal CRUD to get started |
| [kanban-board](kanban-board/) | Auth, Jobs, Crons, Workflows | Full-featured board with JWT auth, background exports, and scheduled cleanup |
| [support-desk](support-desk/) | MCP, Real-time | Ticket system exposing domain logic as MCP tools for LLM agents |

## Running an example

```bash
cd examples/todo
docker compose up
```

Backend at `http://localhost:8080`, frontend at `http://localhost:5173`.

Postgres binds to `127.0.0.1:5432` so you can connect from host tools (psql, rust-analyzer, sqlx) without exposing it to the network.

## Development

These examples are workspace members. The docker-compose files set build context to the workspace root so changes to `crates/` are picked up by cargo-watch automatically.

All examples use `sqlx::query_as!` macros for compile-time SQL checking. To regenerate the `.sqlx/` offline cache after changing queries or migrations:

```bash
cd examples/todo
docker compose exec backend forge migrate prepare
```

To verify queries on host, make sure the db container is running and your editor picks up `DATABASE_URL` from the example's `.env`.
