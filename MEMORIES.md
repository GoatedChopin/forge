Vision
FORGE is a framework for building full-stack applications where PostgreSQL is the only infrastructure dependency. Instead of assembling Redis, Kafka, and service meshes, adopters get auth, jobs, crons, workflows, real-time subscriptions, and observability out of the box. Nodes scale horizontally by sharing PostgreSQL as the coordination layer—no gRPC mesh, no gossip protocols. Workers register capabilities (GPU, high-CPU) and the scheduler assigns jobs intelligently via database queries. The framework handles multi-tenancy, rate limiting, and partitioning so adopters focus on business logic, not infrastructure. Target scale: ~100k MAU per deployment, 99% uptime, with the acceptable failure mode that DB down = service down.

Stack: Rust 1.92+ (edition 2024), Svelte 5 + TypeScript, PostgreSQL
Bun 1.3.1+ required for frontend
Test: LIBRARY_PATH="/opt/homebrew/opt/libiconv/lib" cargo test
Dev: forge dev (starts embedded postgres + backend + frontend)
CLI install: cargo install --path crates/forge
embedded-db feature: only for end-user apps (forge-core/forge-runtime), CLI always includes postgresql_embedded
Docs: cd docs && bun run start (routes: / landing, /docs, /tutorials, /blog)

Release: workflow_dispatch in GitHub Actions → validate → bump-versions → build → release → publish-crates
Version updates: Cargo.toml workspace, internal deps, scaffold templates, docs/package.json
Publishes to crates.io in dependency order with 30s delays

Crates: forge (CLI + runtime), forge-core (traits, types, contexts), forge-macros (proc macros), forge-runtime (executors, gateway), forge-codegen (TypeScript generator)
Package: `forgex` (crates.io), library: `forge` (imports), CLI binary: `forge`
Proc macros use forge::forge_core:: paths

#[forge::model] generates struct + ModelMeta for TypeScript codegen only
Migrations are source of truth - indexes, constraints, defaults in SQL files
User functions take &QueryContext, &MutationContext (references)
axum 0.7+ routes: {param} not :param
ctx.db() accessor, not ctx.pool | RPC: {} normalized to null, response uses `data` field

Advisory locks: leader election (0x464F5247), migrations (0x464F524745)
Tables: forge_nodes, forge_leaders, forge_jobs, forge_cron_runs, forge_workflow_runs/steps, forge_sessions, forge_metrics/logs/traces, forge_migrations
Migrations: migrations/ with 0001_xxx.sql, markers: `-- @up` and `-- @down`

Reactivity: ChangeListener → InvalidationEngine → Reactor → WebSocket
Triggers: forge_enable_reactivity(table), must update last_result_hash after sending

Frontend runtime in $lib/forge/ (types, client, stores, api)
query(): async Promise | subscribe(): Svelte store
Svelte 5: no props destructuring at module level, ForgeProvider sets context immediately

Dashboard: /_dashboard/ (pages), /_api/ (REST)
Dispatch: POST /_api/jobs/{type}/dispatch, /_api/workflows/{name}/start

Durable workflows: ctx.sleep(), ctx.sleep_until(), ctx.wait_for_event()
WorkflowScheduler polls suspended_at, wake_at, waiting_for_event
WorkflowState: Pending, Running, Suspended, Completed, Failed, Cancelled

Rate limiting: forge_rate_limits table with token bucket, keys: User, Ip, Tenant, UserAction, Global
Multi-tenancy: TenantContext with None/Strict isolation, Claims.tenant_id() from JWT
Parallel workflows: ParallelBuilder.step().step_with_compensate().run()
Adaptive tracking: AdaptiveTracker switches Row↔Table based on subscription counts

CLI Templates in crates/forge/templates/ with .tmpl extension
template::render() for {{var}} replacement, include_str!() embeds at compile time
Directories: empty/, populated/, runtime/
Flags: --demo (full examples) or --minimal (clean scaffolding), one required
create_project(dir, name, demo) - demo=true uses populated/, demo=false uses empty/
AGENTS.md included in all scaffolded projects

Template features: #[forge::model], #[forge::query(cache, public, timeout)], #[forge::mutation(timeout, transactional)]
Mutations: ctx.http() for external API calls, #[forge::mutation(transactional)] for opt-in transaction wrapping
Jobs: #[retry], #[idempotent], #[priority], #[worker_capability], ctx.progress(), ctx.heartbeat()
Crons: #[forge::cron("...")], #[timezone], #[catch_up], ctx.log.info/warn/error/debug()
Workflows: #[version], #[timeout], ctx.sleep(), ctx.is_resumed(), ctx.workflow_time()

EnvAccess trait: ctx.env(), ctx.env_or(), ctx.env_require(), ctx.env_parse()
All context types implement EnvAccess, test contexts use MockEnvProvider
Builder: .with_env("KEY", "value"), verification: ctx.env_mock().assert_accessed("KEY")

Testing: TestQueryContext, TestMutationContext, TestJobContext, TestCronContext, TestWorkflowContext
Builder: .as_user(), .with_role(), .with_claim(), .with_tenant(), .with_pool(), .with_env()
TestMutationContext: .mock_http(), .mock_http_json() for HTTP mocking, ctx.http() accessor
MockJobDispatch, MockWorkflowDispatch for dispatch verification
TestDatabase: EXPLICIT config (from_url, from_env, embedded), from_env uses TEST_DATABASE_URL
Tests inline with function files (#[cfg(test)] mod tests), import from forge::testing

Local dev scaffolding:
- Build CLI: cargo build --release -p forgex --features embedded-db
- Create demo: ./target/release/forge new <output-dir> --demo
- Run: cd <output-dir> && forge dev (uses cargo/bun + embedded PostgreSQL)
- Run with Docker: forge dev --docker (uses docker compose up --build)
- Stop: forge dev down (kills orphaned postgres/cargo processes)
- Stop and clean: forge dev down --clear (removes target/ and pg_data/)
- Bare metal requires: cargo 92+, bun 1.3+, embedded-db feature
- Docker mode requires: Docker only
- Backend runs with --no-default-features (no frontend embedding in dev)
