# support-desk

A compact support inbox example built with [FORGE](https://tryforge.dev).

This app demonstrates a practical MCP pattern:
- Human agents act through a small web UI.
- An LLM agent can perform the same ticket actions through MCP on `/_api/mcp`.
- Both paths hit the same domain actions (create ticket, update status/priority, add note).

## What is FORGE?

FORGE handles the hard parts of full-stack engineering so you can focus on business logic:

- **Auth & Sessions** - JWT validation, role-based access, multi-tenancy
- **Smart Caching** - Query-level caching with rate limiting out of the box
- **Transactional Safety** - Atomic writes with automatic rollback on failure
- **End-to-End Type Safety** - Backend types flow directly to your frontend
- **Background Jobs** - Retries, progress tracking, worker capabilities
- **Cron Scheduling** - Timezone-aware, catch-up runs, leader-only execution
- **Durable Workflows** - Multi-step processes that survive restarts
- **Real-time Updates** - SSE subscriptions with automatic invalidation
No Redis. No Kafka. No message queues. Just PostgreSQL.

## Quick Start

```bash
forge dev
```

Requires Docker. This single command:
- Starts PostgreSQL in Docker
- Compiles and runs the backend with hot reload
- Starts the frontend dev server

Backend: `http://localhost:8080` | Frontend: `http://localhost:5173`

MCP endpoint: `http://localhost:8080/_api/mcp`

## MCP Quick Check

Initialize:

```bash
curl -i -X POST http://localhost:8080/_api/mcp \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"curl","version":"1.0"}}}'
```

Use the returned `mcp-session-id` header with:
- `support.list_tickets`
- `support.create_ticket`
- `support.set_status`
- `support.set_priority`
- `support.add_note`

## Integration Tests

Run the full integration suite (boots `forge dev`, runs Playwright UI flow, then MCP script):

```bash
./scripts/run-integration-e2e.sh
```

Run only MCP validation (requires backend running):

```bash
bun scripts/test-mcp.ts
```

To stop and clean up:

```bash
forge dev down          # stop containers
forge dev down --clear  # stop and remove volumes
```

## Build

Single binary (backend + embedded frontend):
```bash
cd frontend && bun install && bun run build && cd ..
cargo build --release
```

## Test

```bash
# Requires a running PostgreSQL (forge dev provides one)
TEST_DATABASE_URL=postgres://localhost/test cargo test
```

See `src/functions/` for test examples.

## Deployment

For deployment options (Docker, VM, etc.), see the [Deployment Guide](https://tryforge.dev/docs/ship/deploy).

## Project Structure

```
support-desk/
├── src/
│   ├── main.rs              # Entry point
│   ├── schema/              # Data models
│   └── functions/           # Queries, mutations, jobs, crons, workflows
├── migrations/              # SQL migrations
├── frontend/                # SvelteKit frontend
├── forge.toml               # FORGE configuration
├── docker-compose.yml       # Development containers
└── Dockerfile               # Production image
```
