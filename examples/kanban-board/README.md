# kanban-board

Built with [FORGE](https://tryforge.dev). One binary, one database, everything else is just code.

A kanban board with auth, real-time project/task management, a daily overdue check cron, CSV export as a background job, and a project archival workflow.

## Development

```bash
forge dev
```

Starts PostgreSQL, the Rust backend, and the SvelteKit frontend. All three.

- Frontend: http://localhost:5173
- Backend: http://localhost:8080
- PostgreSQL: localhost:5432

```bash
forge dev down          # stop everything
forge dev down --clear  # stop + remove volumes and target/
```

### Useful Commands

```bash
forge generate                     # regenerate SvelteKit bindings from Rust models/functions
forge check                        # validate config, migrations, and project health
forge migrate status               # check which migrations have run
forge migrate up                   # apply pending migrations
forge migrate down                 # rollback the last migration
forge migrate prepare              # refresh the .sqlx offline cache
```

### Running Tests

```bash
TEST_DATABASE_URL=postgres://localhost/test cargo test
```

See `src/functions/` for test examples.

## Production Build

```bash
cd frontend && bun install && bun run build && cd ..
cargo build --release
```

The release binary embeds the compiled frontend and the full runtime. One file to deploy. Point it at a PostgreSQL instance and it runs.

For Docker, VM, and other deployment options: [Deployment Guide](https://tryforge.dev/docs/ship/deploy)

## Project Structure

```text
kanban-board/
├── src/
│   ├── main.rs              # Entry point
│   ├── schema/              # Data models (Rust types that generate TS types)
│   │   ├── user.rs          # User model
│   │   ├── project.rs       # Project model
│   │   └── task.rs          # Task model with status/priority enums
│   └── functions/
│       ├── auth.rs          # Register and login mutations
│       ├── projects.rs      # Project CRUD queries and mutations
│       ├── tasks.rs         # Task CRUD with drag-and-drop reordering
│       ├── overdue.rs       # Daily cron: flag overdue tasks
│       ├── export.rs        # Background job: CSV export
│       └── archive.rs       # Durable workflow: project archival
├── migrations/              # SQL migrations (applied on startup)
├── frontend/                # SvelteKit app
├── forge.toml               # Runtime configuration
├── docker-compose.yml       # Development environment
└── Dockerfile               # Production image
```

## Debugging

**Logs**: Set `log_level = "debug"` in `forge.toml` under `[observability]`, or run with `RUST_LOG=debug`. Queries slower than 500ms are warned automatically.

**Health check**: `GET /health` (liveness) and `GET /ready` (checks DB + realtime reactor).

**Inspect jobs and workflows** directly in PostgreSQL:

```sql
-- failed jobs
SELECT job_type, last_error, attempts FROM forge_jobs
WHERE status = 'failed' ORDER BY failed_at DESC;

-- active workflows
SELECT workflow_name, status, current_step FROM forge_workflow_runs
WHERE status IN ('created', 'running');

-- recent cron runs
SELECT cron_name, status, error FROM forge_cron_runs
ORDER BY scheduled_time DESC LIMIT 10;
```

**Realtime not updating?** Check that the SSE connection is open (network tab, `/events` endpoint) and that reactivity is enabled on the table (`SELECT forge_enable_reactivity('table_name');`). Don't call `refetch()` after mutations, the SSE pipeline handles it.

**Traces**: FORGE exports OpenTelemetry spans over HTTP. Point `otlp_endpoint` in `forge.toml` at your collector (Jaeger, Grafana, etc.).

## AI Agents

If you're using an AI coding agent, install the `forge-idiomatic-engineer` skill for Forge-aware code generation:

```bash
bunx skills add https://github.com/isala404/forge/tree/main/docs/skills/forge-idiomatic-engineer
```

[Documentation](https://tryforge.dev/docs)
