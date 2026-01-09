Created proposal documentation for FORGE framework.
- proposal/ directory with architecture, core systems, cluster, database, frontend, observability, CLI specs

Implemented core framework (Phases 1-16).
- Phase 1: Cargo workspace (forge, forge-core, forge-macros, forge-runtime, forge-codegen), ForgeConfig, Database pool, ForgeError
- Phase 2: Schema system with #[forge::model], #[forge::forge_enum], SchemaRegistry, MigrationGenerator
- Phase 3: Function system with ForgeQuery/ForgeMutation/ForgeAction traits, contexts, proc macros, FunctionRouter
- Phase 4: HTTP gateway with JWT Claims, GatewayServer (axum), RpcHandler, AuthMiddleware, CORS
- Phase 5: Job queue with ForgeJob trait, JobContext, SKIP LOCKED claiming, Worker, #[forge::job] macro
- Phase 6: Cron scheduler with ForgeCron trait, CronSchedule, leader-only CronRunner, exactly-once via UNIQUE
- Phase 7: Workflow engine with ForgeWorkflow trait, WorkflowContext, step state tracking, #[forge::workflow] macro
- Phase 8: Clustering with NodeRegistry, HeartbeatLoop, LeaderElection (pg_try_advisory_lock), GracefulShutdown
- Phase 9: Reactivity with ReadSet, ChangeListener (PgListener), InvalidationEngine, WebSocketServer
- Phase 10: Observability with MetricKind, LogLevel, Span, collectors, PostgreSQL batch stores
- Phase 11: TypeScript codegen with TypeGenerator, ApiGenerator, ClientGenerator, StoreGenerator
- Phase 12: Frontend runtime @forge/svelte with ForgeClient, ForgeProvider.svelte, query/subscribe/mutate/action
- Phase 13: Dashboard at /_dashboard/ and /_api/ with metrics, logs, traces, alerts, jobs, workflows, cluster pages
- Phase 14: CLI with forge new/init/add/generate/run commands
- Phase 15: Single binary assembly with Forge struct, ForgeBuilder pattern, graceful shutdown
- Phase 16: Testing infrastructure with TestContext, MockHttp, assertion macros

Fixed framework issues for working scaffolded projects.
- Added [lib] to forge/Cargo.toml, proc macros use forge::forge_core:: paths
- axum routes {param} not :param, Svelte 5 ForgeProvider context timing
- Functions use &QueryContext references, schema uses sqlx::FromRow directly
- query() async Promise-based, DATABASE_URL from .env via dotenvy
- Embedded @forge/svelte runtime in CLI templates

Implemented mesh-safe migration system.
- MigrationRunner with PostgreSQL advisory lock, forge_migrations table
- Split SQL on semicolons, up/down markers (-- @up, -- @down)
- forge migrate up/down/status commands

Implemented full reactivity pipeline.
- PostgreSQL NOTIFY triggers (forge_notify_change function)
- forge_enable_reactivity(table), forge_disable_reactivity(table)
- Reactor orchestrates ChangeListener → InvalidationEngine → Query re-execution → WebSocket push
- Fixed subscription race with pendingSubscriptions queue

Wired dashboard to real queries.
- get_system_info(), get_system_stats() query forge_metrics
- list_traces(), get_trace() extract service.name from JSONB
- 5s auto-refresh, SSE live log stream

Implemented job/workflow dispatch and subscriptions.
- JobDispatch, WorkflowDispatch traits, dispatch_job(), start_workflow() on contexts
- POST /_api/jobs/{type}/dispatch, /_api/workflows/{name}/start
- WebSocket subscriptions via NOTIFY triggers
- createJobTracker(), createWorkflowTracker() factories with localStorage persistence

Implemented fluent workflow step API.
- StepRunner with ctx.step(name, fn).timeout().compensate().run()
- Automatic resume, compensation in reverse order

Updated dependencies.
- toml 0.9, tonic 0.14, prost 0.14, sysinfo 0.37, darling 0.23, jsonwebtoken 10, tokio 1.48

Created Docusaurus documentation website.
- docs/ with MDX files: concepts/, tutorials/, background/, frontend/, api/, cli/
- Dark mode default, routes: / (landing), /docs, /tutorials, /blog

Implemented durable workflows (Phase P0).
- WorkflowContext.sleep(), sleep_until(), wait_for_event() for durable suspension
- WorkflowScheduler polls for ready workflows, EventStore for events
- Database: suspended_at, wake_at, waiting_for_event columns

Implemented rate limiting + parallel workflows + partitioning (Phase P1).
- RateLimiter with PostgreSQL token bucket (forge_rate_limits table)
- ParallelBuilder for concurrent workflow steps
- PartitionManager for time-based observability table partitions

Implemented adaptive tracking (Phase P2).
- AdaptiveTracker switches Row↔Table mode based on subscription counts
- TrackingMode enum: None, Table, Row, Adaptive

Implemented multi-tenancy.
- TenantContext with None/Strict isolation modes
- Claims.tenant_id() from JWT custom claims

Refactored CLI scaffolding to use template files.
- Created crates/forge/templates/ with .tmpl files
- template::render() for {{var}} replacement, template_vars! macro
- Reduced new.rs from 1500 → 200 lines using include_str!

MVP release preparation.
- Rate limiting wired to function router, query cache with TTL
- #[soft_delete] attribute generates deleted_at column
- Docker templates: Dockerfile.tmpl, docker-compose.yml.tmpl
- /ready endpoint with database connectivity check
- GIN indexes on all JSONB columns

Fixed WebSocket reactivity bug for last item deletion.
- last_result_hash in Reactor.handle_change() never updated after re-execution
- Restructured to update hashes via write lock after processing

Fixed workflow durable sleep resumption bug causing infinite loop.
- ctx.sleep() suspended BEFORE record_step_complete(), so on resume sleep re-executed infinitely
- Added resumed_from_sleep flag, with_resumed_from_sleep() builder
- Changed to call resume_from_sleep() instead of resume()

Wired WorkflowScheduler to runtime for durable workflow resumption.
- Scheduler polls every 1s for workflows with wake_at <= NOW()
- Added graceful shutdown with CancellationToken

Fixed workflow step status race condition on resume.
- record_step_start spawned background DB update that could overwrite "completed" back to "running"
- Changed SQL from ON CONFLICT DO UPDATE to ON CONFLICT DO NOTHING

Implemented first-class unit testing infrastructure.
- TestDatabase with explicit config (from_url, from_env, embedded)
- Per-function-type test contexts with builders
- MockHttp with pattern matching, MockJobDispatch, MockWorkflowDispatch
- Assertion macros: assert_ok!, assert_err!, assert_job_dispatched!, etc.
- Tests inline with function files (#[cfg(test)] mod tests)

Rewrote README.md as compelling product narrative.
- Opens with pain point (infrastructure complexity), honest comparison table
- Includes "Who's this for" and "Who's this NOT for" sections

Added AGENTS.md to all scaffolded projects.
- World-class agent guide for FORGE development

Implemented --demo and --minimal flags for scaffolding.
- Reorganized templates: empty/, populated/, runtime/
- One of --demo or --minimal required
- Both modes always include frontend

Removed unused schema field attributes.
- #[forge::model] only generates struct + ModelMeta for TypeScript codegen
- Migrations are source of truth for indexes, constraints, defaults

Simplified feature flags and added embedded PostgreSQL.
- embedded-db feature optional to reduce compile time
- database.embedded = true in forge.toml for zero-dependency deployment

Implemented typesafe environment variable access.
- EnvAccess trait: env(), env_or(), env_require(), env_parse(), env_parse_or(), env_contains()
- All contexts implement EnvAccess, test contexts use MockEnvProvider
- Verification: ctx.env_mock().assert_accessed("KEY")

Refactored CLI: forge dev with embedded postgres.
- forge dev starts embedded postgres, compiles backend, starts frontend
- --no-pg flag to skip embedded postgres when using external DATABASE_URL
- Backend uses cargo-watch watching src/, migrations/, Cargo.toml
- Deleted forge run command

Improved forge check validation.
- forge.toml: Valid TOML, [project] name, [database] url format, [gateway] port range
- Cargo.toml: forge/forgex dependency exists
- Migrations: NNNN_name.sql naming, -- @up marker present
- Linting: cargo fmt --check, cargo clippy -D warnings, bun run lint, bun run format:check

Updated to Rust edition 2024 with Rust 1.92+ and Bun 1.3.1+ requirements.
- Workspace edition 2024, rust-version = "1.92"
- std::env::set_var wrapped in unsafe block
- Renamed `gen` variables to `generator` (reserved keyword)

Added git init and project name extraction to CLI scaffolding.
- extract_project_name() handles paths like "path/to/my-app" → "my-app"
- init_git_repo() only when not inside existing repo

Changed library name from forgex to forge.
- Package: forgex (crates.io), library: forge (imports)
- Users: cargo install forgex, then use forge::prelude::*

Implemented workflow_dispatch release pipeline.
- Input: version string, Jobs: validate → bump-versions → build → release → publish-crates
- Commits directly to main, auto-creates tag and GitHub release

Restructured docs site.
- Routes: / (landing), /docs (documentation), /tutorials (separate plugin), /blog

Fixed Docker templates for scaffolded projects.
- Removed non-existent schema/functions COPY, made Cargo.lock optional
- DATABASE_URL override for Docker networking

Replaced get_quote with get_bitcoin_price action.
- CoinGecko API for live Bitcoin price (USD/EUR/GBP + 24h change)

Standardized PostgreSQL credentials.
- postgres://postgres:forge@localhost:5432/{{project_name}}

Additional CLI refinements.
- Dropped Windows support
- Added linting checks to forge check
- Updated deployment docs for Docker/VM

Released v0.0.1 to GitHub and crates.io.
- Removed all 0.0.2-alpha version references
- Fixed Rust 2024 edition: unsafe blocks for std::env::set_var/remove_var
- Fixed clippy: field_reassign_with_default in postgresql_embedded Settings
- Fixed release workflow: removed vendored-openssl from cargo-edit install
- Fixed Dockerfile template: multi-stage build with frontend (bun) → backend (rust)
- Published forgex, forge-core, forge-macros, forge-runtime, forge-codegen to crates.io
- Binaries: Linux (x86_64), macOS (x86_64, aarch64), Windows (x86_64)
- Install: curl -fsSL https://raw.githubusercontent.com/isala404/forge/main/docs/static/install.sh | sh
