# FORGE

**The Full-Stack Framework for the Impatient**

Everything you need in one binary. No Redis, no Kafka, no message queues - just PostgreSQL.

[![Crates.io](https://img.shields.io/crates/v/forgex.svg)](https://crates.io/crates/forgex)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Documentation](https://img.shields.io/badge/docs-tryforge.dev-blue)](https://tryforge.dev/docs)

## What is FORGE?

FORGE is a Rust full-stack framework that compiles your entire backend into a single binary. One process handles API gateway, background jobs, cron scheduling, and durable workflows - all powered by PostgreSQL.

```rust
#[forge::mutation]
pub async fn create_order(ctx: &MutationContext, input: OrderInput) -> Result<Order> {
    let user_id = ctx.require_user_id()?;

    // Create order in database
    let order = create_order_in_db(ctx.db(), user_id, &input).await?;

    // Dispatch background job
    ctx.dispatch_job("process_order", json!({ "order_id": order.id })).await?;

    Ok(order)
}

#[forge::job]
#[retry(max_attempts = 3)]
pub async fn process_order(ctx: &JobContext, input: ProcessInput) -> Result<()> {
    ctx.progress(50, "Processing...")?;
    process_payment(&input.order_id).await?;
    ctx.progress(100, "Complete")?;
    Ok(())
}

#[forge::cron("0 9 * * *")]  // 9 AM daily
pub async fn daily_report(ctx: &CronContext) -> Result<()> {
    generate_and_send_report(ctx.db()).await
}
```

## Why FORGE?

| Traditional Stack | FORGE |
|-------------------|-------|
| API Server + Redis + Kafka + Celery + Cron daemon | **Single binary** |
| Multiple deployment configs | **One `forge run`** |
| N+1 failure points | **PostgreSQL only** |
| Complex local dev setup | **`forge dev`** |

### Key Features

- **Single Binary** - API gateway + workers + scheduler in one process
- **Type-Safe** - Rust proc macros with full IDE support
- **Background Jobs** - Built-in with retries, progress tracking, dashboard
- **Cron Jobs** - Built-in scheduler with timezone support
- **Durable Workflows** - Multi-step processes with automatic compensation (saga pattern)
- **Real-time** - PostgreSQL LISTEN/NOTIFY via WebSocket
- **TypeScript Codegen** - Auto-generated frontend types
- **Self-hosted** - Deploy anywhere: Railway, Render, Fly.io, or bare metal

## Quick Start

```bash
# Install
curl -fsSL https://tryforge.dev/install.sh | sh
# Or: cargo install forgex

# Create project
forge new my-app
cd my-app

# Start development
forge dev

# Open http://localhost:8080
```

**That's it.** No Docker compose with 10 services. No "install Redis first". Just PostgreSQL.

## Documentation

- **[Quick Start](https://tryforge.dev/docs/quick-start)** - Get running in 5 minutes
- **[Tutorials](https://tryforge.dev/docs/tutorials)** - Learn by building
- **[API Reference](https://tryforge.dev/docs/api)** - Complete API docs
- **[Compare](https://tryforge.dev/docs/compare)** - FORGE vs Supabase, Firebase, PocketBase

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Single FORGE Binary                       │
├─────────────┬─────────────┬─────────────┬──────────────────┤
│   Gateway   │   Workers   │  Scheduler  │    Dashboard     │
│  (HTTP/WS)  │   (Jobs)    │   (Cron)    │   (Built-in)     │
└──────┬──────┴──────┬──────┴──────┬──────┴────────┬─────────┘
       │             │             │               │
       └─────────────┴──────┬──────┴───────────────┘
                            │
                     ┌──────▼──────┐
                     │ PostgreSQL  │
                     │  (Only DB)  │
                     └─────────────┘
```

## Feature Matrix

| Feature | Status | Description |
|---------|--------|-------------|
| Queries | ✅ | Read-only database functions |
| Mutations | ✅ | Write operations with job dispatch |
| Actions | ✅ | Webhook handlers for external services |
| Background Jobs | ✅ | Async processing with retries |
| Cron Jobs | ✅ | Scheduled tasks with timezone |
| Workflows | ✅ | Durable multi-step with compensation |
| Real-time | ✅ | PostgreSQL NOTIFY subscriptions |
| Dashboard | ✅ | Built-in job/workflow monitoring |
| TypeScript Codegen | ✅ | Auto-generated frontend types |
| Multi-tenancy | ✅ | Built-in tenant isolation |
| Rate Limiting | ✅ | Per-user/tenant rate limits |

## Comparison

| | FORGE | Supabase | Firebase | PocketBase |
|--|-------|----------|----------|------------|
| Database | PostgreSQL | PostgreSQL | Firestore | SQLite |
| Backend Code | Rust | Edge Functions | Cloud Functions | Go/JS |
| Background Jobs | Built-in | External | Cloud Functions | None |
| Workflows | Built-in | None | None | None |
| Self-hosted | Simple | Complex | No | Simple |
| Vendor lock-in | None | Low | High | None |

[See detailed comparisons →](https://tryforge.dev/docs/compare)

## Example: E-Commerce Order Flow

```rust
#[forge::workflow]
#[timeout = "30m"]
pub async fn process_order(ctx: &WorkflowContext, input: OrderInput) -> Result<Order> {
    // Step 1: Reserve inventory (with compensation)
    let reservation = ctx.step("reserve_inventory")
        .run(|| reserve_items(&input.items))
        .compensate(|r| release_items(r))
        .await?;

    // Step 2: Charge payment (with compensation)
    let payment = ctx.step("charge_payment")
        .run(|| charge_card(&input.payment))
        .compensate(|p| refund(p.id))
        .await?;

    // Step 3: Create shipment
    let shipment = ctx.step("create_shipment")
        .run(|| ship_order(&input))
        .await?;

    // If any step fails, compensations run automatically in reverse!

    Ok(Order { id: input.id, status: "processing" })
}
```

## Deploy in 60 Seconds

### Railway
```bash
railway init
railway add postgres
railway up
```

### Render
Push to GitHub → Connect repo → Use included `render.yaml`

### Fly.io
```bash
fly launch
fly postgres create
fly deploy
```

### Docker
```bash
docker build -t my-app .
docker run -e DATABASE_URL=postgres://... my-app
```

## Community

- **[Discord](https://discord.gg/forge)** - Chat with the community
- **[GitHub Discussions](https://github.com/isala404/forge/discussions)** - Ask questions
- **[Twitter](https://twitter.com/forgex_dev)** - Updates and announcements

## Contributing

We welcome contributions! See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

```bash
# Clone and build
git clone https://github.com/isala404/forge
cd forge
cargo build

# Run tests
cargo test
```

## License

MIT License - see [LICENSE](LICENSE) for details.

---

**Built with Rust.** PostgreSQL only. Ship faster.

[Get Started →](https://tryforge.dev/docs/quick-start)
