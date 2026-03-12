# FORGE

**Stop Assembling. Start Building.**

You didn't sign up to be a distributed systems engineer. You signed up to build products.

Yet here you are, wiring up Redis for caching, Kafka for events, BullMQ for jobs, a separate cron daemon, and praying they all stay in sync. Your `docker-compose.yml` has more services than your app has features.

FORGE compiles your entire backend into **one binary**: API, jobs, crons, workflows, real-time subscriptions. The only dependency? PostgreSQL. That's it.

```bash
curl -fsSL https://tryforge.dev/install.sh | sh
forge new my-app --template with-svelte/minimal && cd my-app
forge dev
```

[![Crates.io](https://img.shields.io/crates/v/forgex.svg)](https://crates.io/crates/forgex)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Docs](https://img.shields.io/badge/docs-tryforge.dev-blue)](https://tryforge.dev/docs)

---

## The Problem

Modern backend development is infrastructure theater:

```
Your Typical Stack                    What You Actually Need
───────────────────                   ────────────────────────
API Server (Express/FastAPI)          Handle HTTP requests
Redis                                 Remember things temporarily
Kafka/RabbitMQ                        Process things later
BullMQ/Celery                         Run background jobs
Cron daemon                           Do things on schedule
WebSocket server                      Push updates to clients
Prometheus + Grafana                  Know what's happening
```

Seven systems. Seven failure points. Seven things to deploy, monitor, and debug at 3 AM.

PostgreSQL already does all of this. [SKIP LOCKED](https://www.inferable.ai/blog/posts/postgres-skip-locked) for job queues. [LISTEN/NOTIFY](https://neon.com/guides/pub-sub-listen-notify) for pub/sub. Advisory locks for coordination. You just need a runtime that actually uses them.

![Real-time sync between iOS and web](docs/static/demo/web-ios-sync.gif)

One mutation. Both clients update instantly. No manual cache busting, no fetch wrappers, no Redux.

---

## How It Works

### Queries and Mutations

```rust
#[forge::query(cache = "30s")]
pub async fn get_user(ctx: &QueryContext, id: Uuid) -> Result<User> {
    sqlx::query_as!(User, "SELECT * FROM users WHERE id = $1", id)
        .fetch_one(ctx.db())
        .await
        .map_err(Into::into)
}

#[forge::mutation]
pub async fn create_user(ctx: &MutationContext, input: CreateUser) -> Result<User> {
    let mut conn = ctx.conn().await?;
    let user = sqlx::query_as!(User, "INSERT INTO users (email) VALUES ($1) RETURNING *", &input.email)
        .fetch_one(&mut *conn)
        .await?;

    ctx.dispatch_job("send_welcome_email", json!({ "user_id": user.id })).await?;

    Ok(user)
}
```

These become typed RPC endpoints automatically. Forge generates framework bindings from the same Rust source of truth. Today that means TypeScript bindings for SvelteKit and Rust bindings plus hooks for Dioxus. No routing files, no fetch wrappers, no manual type definitions.

Mutations run inside a database transaction. The `dispatch_job` call gets buffered and inserted atomically when the transaction commits. If the mutation fails, the job never exists.

### Background Jobs

```rust
#[forge::job]
#[retry(max_attempts = 3, backoff = "exponential")]
pub async fn send_welcome_email(ctx: &JobContext, input: EmailInput) -> Result<()> {
    ctx.progress(0, "Starting...")?;

    let user = fetch_user(ctx.db(), input.user_id).await?;
    send_email(&user.email, "Welcome!").await?;

    ctx.progress(100, "Sent")?;
    Ok(())
}
```

Persisted in PostgreSQL. Survive restarts. Retry with backoff. Report progress in real-time. No Redis. No separate worker process.

### Cron

```rust
#[forge::cron("0 9 * * *")]
#[timezone = "America/New_York"]
pub async fn daily_digest(ctx: &CronContext) -> Result<()> {
    if ctx.is_late() {
        ctx.log.warn("Running late", json!({ "delay": ctx.delay() }));
    }

    generate_and_send_digest(ctx.db()).await
}
```

Timezone support. Catch-up for missed runs. Leader-elected so it runs exactly once across all instances.

### Durable Workflows

```rust
#[forge::workflow]
#[version = 1]
#[timeout = "60d"]
pub async fn free_trial_flow(ctx: &WorkflowContext, user: User) -> Result<()> {
    ctx.step("start_trial")
        .run(|| activate_trial(&user))
        .compensate(|_| deactivate_trial(&user))
        .await?;

    ctx.step("send_welcome").run(|| send_email(&user, "Welcome!")).await?;

    ctx.sleep(Duration::from_days(45)).await;  // Survives deployments.

    ctx.step("trial_ending").run(|| send_email(&user, "3 days left!")).await?;

    ctx.sleep(Duration::from_days(3)).await;

    ctx.step("convert_or_expire").run(|| end_trial(&user)).await?;
    Ok(())
    // If any step fails, previous steps compensate in reverse order
}
```

Sleep for 45 days, deploy new code, restart servers, scale up. The workflow picks up exactly where it left off. Compensation runs automatically if later steps fail. No separate orchestration cluster.

### Real-Time Subscriptions

```svelte
<script lang="ts">
  import { subscribe } from '$lib/forge';

  const users = subscribe('list_users', {});
</script>

{#each $users.data ?? [] as user}
  <div>{user.email}</div>
{/each}
```

Compile-time SQL parsing extracts table dependencies (including JOINs and subqueries). PostgreSQL triggers fire NOTIFY on changes. FORGE re-runs affected queries. SSE pushes diffs to clients. No manual cache invalidation. No pub/sub wiring.

### Frontend Bindings

Frontend support is no longer hard-wired to Svelte. The CLI now treats frontend targets as framework specs, so codegen, scaffolding, formatting, and runtime package wiring all hang off the selected target.

Current first-class targets:

- `sveltekit` -> generated TypeScript bindings in `frontend/src/lib/forge` backed by `@forge-rs/svelte`
- `dioxus` -> generated Rust bindings in `frontend/src/forge` backed by `forge-dioxus`

Forge ships template ids for both SvelteKit and Dioxus examples:

```bash
forge new my-app --template with-dioxus/demo
forge generate --target dioxus
```

The frontend target architecture is designed so more framework bindings can be added without reworking the CLI around another pile of match statements.

### Webhooks

```rust
#[forge::webhook(
    path = "/hooks/stripe",
    signature = WebhookSignature::hmac_sha256("Stripe-Signature", "STRIPE_WEBHOOK_SECRET"),
    idempotency = "header:Idempotency-Key",
)]
pub async fn stripe(ctx: &WebhookContext, payload: Value) -> Result<WebhookResult> {
    ctx.dispatch_job("process_payment", payload.clone()).await?;
    Ok(WebhookResult::Accepted)
}
```

Signature validation, idempotency tracking, and job dispatch. One handler.

### MCP Tools

```rust
#[forge::mcp_tool(
    name = "tickets.list",
    title = "List Support Tickets",
    read_only,
)]
pub async fn list_tickets(ctx: &McpToolContext) -> Result<Vec<Ticket>> {
    sqlx::query_as("SELECT * FROM tickets")
        .fetch_all(ctx.db())
        .await
        .map_err(Into::into)
}
```

Expose any function as an MCP tool. Same auth, rate limiting, and validation as your API. AI agents get first-class access without a separate integration layer.

---

## Type Safety, End to End

```rust
#[forge::model]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub role: UserRole,
    pub created_at: DateTime<Utc>,
}

#[forge::model]
pub enum UserRole {
    Admin,
    Member,
    Guest,
}
```

```typescript
// Generated automatically
export interface User {
  id: string;
  email: string;
  role: UserRole;
  created_at: string;
}

export type UserRole = 'Admin' | 'Member' | 'Guest';

import { api } from '$lib/forge';
const user = await api.get_user({ id: '...' });  // Fully typed
```

If your Rust code compiles, your frontend types are correct.

### SQLx Compile-Time Checking

FORGE leans on `sqlx` macros for query validation at compile time.

- New projects include `sqlx.toml` with offline mode enabled.
- `forge migrate prepare` runs pending migrations and then executes `cargo sqlx prepare --workspace`.
- `forge check` verifies that `.sqlx/` exists and is not older than your migrations.

That gives you typed frontend bindings and compile-time-checked SQL from the same Rust source.

---

## Architecture

```
┌──────────────────────────────────────────────────┐
│                    forge run                     │
├─────────────┬─────────────┬─────────────┤
│   Gateway   │   Workers   │  Scheduler  │
│ (HTTP/SSE)  │   (Jobs)    │   (Cron)    │
└──────┬──────┴──────┬──────┴──────┬──────┘
       │             │             │
       └─────────────┴──────┬──────┘
                            │
                     ┌──────▼──────┐
                     │ PostgreSQL  │
                     └─────────────┘
```

One process, multiple subsystems:

- **Gateway**: HTTP/SSE server built on [Axum](https://github.com/tokio-rs/axum)
- **Workers**: Pull jobs from PostgreSQL using `SKIP LOCKED`
- **Scheduler**: Leader-elected cron runner via advisory locks
- **Daemons**: Long-running singleton processes with leader election

Scale horizontally by running more instances. They coordinate through PostgreSQL. No service mesh, no gossip protocol, no Redis cluster.

```
forge              → Public API, Forge::builder(), prelude, CLI
├── forge-runtime  → Gateway, function router, job worker, workflow executor, cron scheduler
│   ├── forge-core → Types, traits, error types, contexts, schema definitions
│   └── forge-macros → #[query], #[mutation], #[job], #[workflow], #[cron]
└── forge-codegen  → Framework binding generators (SvelteKit, Dioxus)
```

---

## Why Not Just Use...

| | FORGE | Supabase | Firebase | PocketBase |
|--|:-----:|:--------:|:--------:|:----------:|
| **Background Jobs** | Built-in | External | Cloud Functions | - |
| **Durable Workflows** | Built-in | - | - | - |
| **Cron Scheduling** | Built-in | External | Cloud Scheduler | - |
| **Query Caching** | Built-in | - | - | - |
| **Rate Limiting** | Built-in | - | - | - |
| **Real-time** | Built-in | Built-in | Built-in | - |
| **Webhooks** | Built-in | - | Cloud Functions | - |
| **MCP Tools** | Built-in | - | - | - |
| **Full Type Safety** | Rust to TS | Partial | - | - |
| **Self-Hosted** | One binary | Complex | - | One binary |
| **Vendor Lock-in** | None | Low | High | None |
| **Database** | PostgreSQL | PostgreSQL | Firestore | SQLite |

**vs. Temporal/Inngest**: FORGE workflows run in-process with no separate orchestration service. If you need child workflows, signals, or advanced versioning, use Temporal. If you need durable multi-step processes without the ops overhead, FORGE handles it.

**vs. Node.js + BullMQ + the rest**: FORGE trades ecosystem breadth for operational simplicity. Fewer npm packages, fewer 3 AM pages about Redis running out of memory.

---

## CLI

`forge dev` starts PostgreSQL, a cargo-watch backend, and the selected frontend target. `forge new` now requires a checked-in template id such as `with-svelte/minimal`, `with-svelte/demo`, or `with-dioxus/realtime-todo-list`. `forge generate --target dioxus` still forces Dioxus binding generation when detection is not enough.

```bash
forge generate                   # generate frontend/runtime bindings from backend code
forge check                      # validate config, migrations, project health
forge migrate status             # check which migrations have run
forge migrate up                 # apply pending migrations
forge migrate down               # rollback the last migration
forge migrate prepare            # refresh the .sqlx offline cache for sqlx macros
```

### Deploy

```bash
cargo build --release
./target/release/my-app
```

One binary. Embeds the frontend build and the entire runtime. Point it at PostgreSQL and it runs. [Read the docs](https://tryforge.dev/docs) for more.

---

## Debugging

Everything runs through PostgreSQL. That means everything is queryable.

### Health Endpoints

```
GET /health    → { "status": "healthy", "version": "0.4.1" }
GET /ready     → { "ready": true, "database": true, "reactor": true }
```

### Inspect Jobs

```sql
-- pending jobs
SELECT id, job_type, status, attempts, max_attempts, scheduled_at
FROM forge_jobs WHERE status = 'pending' ORDER BY scheduled_at;

-- failed jobs with error messages
SELECT id, job_type, last_error, attempts, failed_at
FROM forge_jobs WHERE status IN ('failed', 'dead_letter') ORDER BY failed_at DESC;

-- running jobs with progress
SELECT id, job_type, progress_percent, progress_message, worker_id
FROM forge_jobs WHERE status = 'running';
```

### Inspect Workflows

```sql
-- active workflows
SELECT id, workflow_name, status, current_step, started_at
FROM forge_workflow_runs WHERE status IN ('created', 'running');

-- step-by-step details for a specific run
SELECT step_name, status, error, started_at, completed_at
FROM forge_workflow_steps WHERE workflow_run_id = $1 ORDER BY started_at;
```

### Inspect Cron Runs

```sql
SELECT cron_name, scheduled_time, status, error
FROM forge_cron_runs ORDER BY scheduled_time DESC LIMIT 20;
```

### Logging

Configure in `forge.toml`:

```toml
[observability]
log_level = "debug"   # debug, info, warn, error
```

Or override with environment variables:

```bash
RUST_LOG=debug forge dev                     # everything
RUST_LOG=warn,my_app=debug forge dev         # your code only
```

Queries slower than 500ms are logged as warnings automatically. Distributed tracing is built in via OpenTelemetry (OTLP over HTTP).

### Realtime Subscriptions

If subscriptions aren't updating after mutations:

1. Make sure the SSE connection is established before mutating (check the network tab for `/events`)
2. Verify reactivity is enabled for the table: `SELECT forge_enable_reactivity('table_name');`
3. Don't manually call `refetch()` after mutations. The SSE pipeline handles invalidation automatically.

### System Tables

All FORGE state lives in PostgreSQL. The full set of system tables:

| Table | What it tracks |
|-------|---------------|
| `forge_jobs` | Job queue, status, errors, progress |
| `forge_cron_runs` | Cron execution history |
| `forge_workflow_runs` | Workflow instances and state |
| `forge_workflow_steps` | Individual step results |
| `forge_nodes` | Cluster node registry |
| `forge_leaders` | Leader election state |
| `forge_daemons` | Long-running process status |
| `forge_sessions` | Active SSE connections |
| `forge_subscriptions` | Live query subscriptions |
| `forge_rate_limits` | Token bucket state |
| `forge_webhook_events` | Webhook idempotency tracking |

---

## Who's This For

FORGE is opinionated. It's for:

- **Solo developers and small teams** building SaaS products who don't want to manage infrastructure
- **Teams who value correctness**: errors caught at compile time, not at 3 AM
- **Anyone tired of gluing together** seven services for basic backend functionality

Not the right fit if:

- You have a dedicated platform team that wants fine-grained control over each component
- You're building for millions of concurrent users (FORGE targets ~100k MAU comfortably)
- You need deep integration with cloud-native services (Lambda, DynamoDB, Pub/Sub)

---

## AI Agents

If you're using an AI coding agent to build with FORGE, install the [`forge-idiomatic-engineer`](docs/skills/forge-idiomatic-engineer) skill for Forge-aware code generation:

```bash
bunx skills add https://github.com/isala404/forge/tree/main/docs/skills/forge-idiomatic-engineer
```

This is installed automatically when you run `forge new`.

---

## Project Maturity

FORGE is pre-1.0. Breaking changes happen between releases. Good for side projects, internal tools, and kicking the tires. Not production yet.

Breaking changes are documented in [CHANGELOG.md](CHANGELOG.md). Pin your version if you need stability. Once the core API settles, we cut 1.0 and commit to semver.

[Contributions welcome](CONTRIBUTING.md).

---

## License

MIT. Do whatever you want.

---

<p align="center">
  <strong>PostgreSQL is enough.</strong><br>
  <a href="https://tryforge.dev/docs/quick-start">Get Started</a> ·
  <a href="https://tryforge.dev/docs">Documentation</a> ·
  <a href="https://github.com/isala404/forge/discussions">Discussions</a>
</p>
