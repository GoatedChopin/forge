# FORGE

**Stop Assembling. Start Building.**

You didn't sign up to be a distributed systems engineer. You signed up to build products.

Yet here you are, wiring up Redis for caching, Kafka for events, BullMQ for jobs, a separate cron daemon, and praying they all stay in sync. Your `docker-compose.yml` has more services than your app has features.

FORGE compiles your entire backend into **one binary**: API, jobs, crons, workflows, real-time subscriptions. The only dependency? PostgreSQL. That's it.

```bash
curl -fsSL https://tryforge.dev/install.sh | sh
forge new my-app --demo && cd my-app
forge dev
```

[![Crates.io](https://img.shields.io/crates/v/forgex.svg)](https://crates.io/crates/forgex)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Docs](https://img.shields.io/badge/docs-tryforge.dev-blue)](https://tryforge.dev/docs)

---

## The Problem

Modern backend development has become infrastructure theater:

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

PostgreSQL already does all of this. [SKIP LOCKED](https://www.inferable.ai/blog/posts/postgres-skip-locked) for job queues. [LISTEN/NOTIFY](https://neon.com/guides/pub-sub-listen-notify) for pub/sub. Advisory locks for coordination. You just need a framework that uses them properly.

---

## What FORGE Actually Does

### 1. Queries and Mutations (Your API)

```rust
#[forge::query(cache = "30s")]
pub async fn get_user(ctx: &QueryContext, id: Uuid) -> Result<User> {
    sqlx::query_as("SELECT * FROM users WHERE id = $1")
        .bind(id)
        .fetch_one(ctx.db())
        .await
        .map_err(Into::into)
}

#[forge::mutation]
pub async fn create_user(ctx: &MutationContext, input: CreateUser) -> Result<User> {
    let user = sqlx::query_as("INSERT INTO users (email) VALUES ($1) RETURNING *")
        .bind(&input.email)
        .fetch_one(ctx.db())
        .await?;

    // Dispatch a background job
    ctx.dispatch_job("send_welcome_email", json!({ "user_id": user.id })).await?;

    Ok(user)
}
```

These become `/rpc/get_user` and `/rpc/create_user` automatically. A fully typed TypeScript client is generated. Call `api.get_user()` and get autocomplete, type checking, and error handling. No routing. No fetch wrappers. No manual type definitions.

### 2. Background Jobs (Things That Take Time)

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

Jobs are persisted in PostgreSQL, survive restarts, retry with backoff, and report progress in real-time. No Redis. No separate worker process.

### 3. Scheduled Tasks (Cron Without the Daemon)

```rust
#[forge::cron("0 9 * * *")]  // 9 AM daily
#[timezone = "America/New_York"]
pub async fn daily_digest(ctx: &CronContext) -> Result<()> {
    if ctx.is_late() {
        ctx.log.warn("Running late", json!({ "delay": ctx.delay() }));
    }

    generate_and_send_digest(ctx.db()).await
}
```

Cron scheduling with timezone support, catch-up for missed runs, and structured logging. Runs in the same process.

### 4. Durable Workflows (Multi-Step Processes That Don't Break)

```rust
#[forge::workflow]
#[version = 1]  // Bump when changing step order. In-flight workflows keep their original version.
#[timeout = "60d"]
pub async fn free_trial_flow(ctx: &WorkflowContext, user: User) -> Result<()> {
    // Each step can define compensation (rollback) logic
    ctx.step("start_trial")
        .run(|| activate_trial(&user))
        .compensate(|_| deactivate_trial(&user))
        .await?;

    ctx.step("send_welcome").run(|| send_email(&user, "Welcome!")).await?;

    ctx.sleep(Duration::from_days(45)).await;  // 45 days. Survives deployments.

    ctx.step("trial_ending").run(|| send_email(&user, "3 days left!")).await?;

    ctx.sleep(Duration::from_days(3)).await;

    ctx.step("convert_or_expire").run(|| end_trial(&user)).await?;
    Ok(())
    // If any step fails, previous steps compensate in reverse order
}
```

Deploy new code, restart servers, scale up or down. The workflow picks up right where it left off. Sleep for 45 days, and it just works. Compensation (rollback) runs automatically if later steps fail. This is durable execution without running a separate orchestration cluster.

### 5. Real-Time Subscriptions (Live Data, No Extra Work)

```svelte
<script lang="ts">
  import { subscribe } from '$lib/forge';

  // This auto-updates when data changes. Any client, anywhere.
  const users = subscribe('list_users', {});
</script>

{#each $users.data ?? [] as user}
  <div>{user.email}</div>
{/each}
```

Under the hood: Compile-time SQL parsing extracts all table dependencies (including JOINs and subqueries) → PostgreSQL triggers fire NOTIFY on changes → FORGE re-runs affected queries → WebSocket pushes diffs to clients.

No WebSocket code. No manual cache invalidation. Just reactive queries.

---

## The Architecture

```
┌────────────────────────────────────────────────────────────┐
│                       forge run                            │
├─────────────┬─────────────┬─────────────┬──────────────────┤
│   Gateway   │   Workers   │  Scheduler  │    Dashboard     │
│  (HTTP/WS)  │   (Jobs)    │   (Cron)    │   (Built-in)     │
└──────┬──────┴──────┬──────┴──────┬──────┴────────┬─────────┘
       │             │             │               │
       └─────────────┴──────┬──────┴───────────────┘
                            │
                     ┌──────▼──────┐
                     │ PostgreSQL  │
                     └─────────────┘
```

One process. Multiple subsystems handle different concerns:

- **Gateway**: HTTP/WebSocket server (built on [Axum](https://github.com/tokio-rs/axum))
- **Workers**: Pull jobs from PostgreSQL using `SKIP LOCKED`
- **Scheduler**: Leader-elected cron runner (advisory locks prevent duplicate runs)
- **Dashboard**: Built-in UI for monitoring jobs, workflows, and metrics

Scale horizontally by running multiple instances. They coordinate through PostgreSQL. No service mesh, no gossip protocol, no Redis cluster.

### Crate Layout

```
forge              → Public API, Forge::builder(), prelude, CLI
├── forge-runtime  → Gateway, function router, job worker, workflow executor, cron scheduler
│   ├── forge-core → Types, traits, error types, contexts, schema definitions
│   └── forge-macros → #[query], #[mutation], #[job], #[workflow], #[cron]
└── forge-codegen  → TypeScript/Svelte client generator
```

---

## Type Safety, End to End

FORGE generates TypeScript types from your Rust models:

```rust
// Rust: your source of truth
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
// TypeScript: generated automatically
export interface User {
  id: string;
  email: string;
  role: UserRole;
  created_at: string;
}

export type UserRole = 'Admin' | 'Member' | 'Guest';

// API client is also generated
import { api } from '$lib/forge';
const user = await api.get_user({ id: '...' });  // Fully typed
```

If your Rust code compiles, your frontend types are correct. This eliminates an entire class of "worked in dev, broke in prod" bugs.

---

## Why Not Just Use...

| | FORGE | Supabase | Firebase | PocketBase |
|--|:-----:|:--------:|:--------:|:----------:|
| **Background Jobs** | Built-in | External | Cloud Functions | ❌ |
| **Durable Workflows** | Built-in | ❌ | ❌ | ❌ |
| **Cron Scheduling** | Built-in | External | Cloud Scheduler | ❌ |
| **Query Caching** | Built-in | ❌ | ❌ | ❌ |
| **Rate Limiting** | Built-in | ❌ | ❌ | ❌ |
| **Real-time** | Built-in | Built-in | Built-in | ❌ |
| **Full Type Safety** | Rust → TS | Partial | ❌ | ❌ |
| **Self-Hosted** | One binary | Complex | ❌ | One binary |
| **Vendor Lock-in** | None | Low | High | None |
| **Database** | PostgreSQL | PostgreSQL | Firestore | SQLite |

**vs. Temporal/Inngest**: FORGE workflows are simpler (no separate service) but less feature-complete. If you need advanced workflow features (versioning, signals, child workflows), use Temporal. If you need "good enough" workflows without the operational overhead, use FORGE.

**vs. Node.js + BullMQ + etc.**: FORGE trades ecosystem breadth for operational simplicity. You get fewer npm packages but also fewer 3 AM pages about Redis running out of memory.

---

## Getting Started

```bash
# Install
curl -fsSL https://tryforge.dev/install.sh | sh
# Or: cargo install forgex

# Create a project
forge new my-app --demo

# Set up PostgreSQL
docker run -rm -d --name forge-postgres -e POSTGRES_PASSWORD=forge -e POSTGRES_DB=my-app -p 5432:5432 postgres:18

# Start backend
cd my-app
cargo run
# → API at http://localhost:8080
# → Dashboard at http://localhost:8080/_dashboard

# Start frontend (in another terminal)
cd my-app/frontend
bun install
bun run dev
# → Frontend at http://localhost:5173
```

The `--demo` flag scaffolds a working app with examples of queries, mutations, jobs, crons, and workflows. Or use `--minimal` for a clean slate.

[**Read the docs →**](https://tryforge.dev/docs)

---

## Who's This For

FORGE is opinionated. It's designed for:

- **Solo developers and small teams** building SaaS products who don't want to manage infrastructure
- **Teams who value reliability**: no null pointer exceptions, no "undefined is not a function", errors caught at compile time
- **Anyone tired of gluing together** 7 different services for basic backend functionality

Probably not the right fit if:

- You have a dedicated platform team and need fine-grained control over each component
- You're building for millions of concurrent users (FORGE targets ~100k MAU comfortably)
- You need deep integration with cloud-native services (Lambda, DynamoDB, Pub/Sub)

---

## Status

FORGE is in **alpha**. The API is stabilizing but may change. It's been used in production for small projects, but you should evaluate it yourself before betting your company on it.

We're actively working on these. [Contributions welcome](CONTRIBUTING.md).

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
