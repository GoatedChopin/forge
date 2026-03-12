# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.7.0] - 2026-03-12

### Added

- Dioxus frontend support with template-driven project creation, codegen, and runtime client (`forge new --template with-dioxus/demo`)
- `forge test` command wrapping Playwright with prerequisite checks, `--ui` and `--headed` flags
- `forge prepare` command for sqlx compile-time query checking with offline cache support
- Published `@forge-rs/svelte` npm package and `forge-dioxus` crate as standalone runtime packages

### Changed

- Frontend runtimes extracted from embedded CLI templates into published packages (`@forge-rs/svelte`, `forge-dioxus`)
- `forge generate` no longer writes runtime files to `.forge/`; projects depend on published packages instead
- Runtime config and docker-compose template defaults simplified
- Playwright test suites run sequentially by default for reliability
- Example docker-compose switched from named volumes to bind mounts for host LSP visibility
- RPC error handling improved in test fixtures

### Removed

- Embedded frontend runtime templates (`.forge/svelte/`, `.forge/dioxus/`); replaced by published packages

## [0.6.0] - 2026-03-09

### Added

- `ctx.issue_token()` on all context types for generating HMAC-signed JWTs without external auth providers
- Generated file checksums (`.forge/checksums.json`) to detect manual modifications to forge-managed frontend files
- Per-layer trace filtering for fine-grained observability control per tracing target
- PostgreSQL `application_name` connection parameter for identifying forge connections in database monitoring tools
- `has_input_args` flag on `FunctionInfo` to distinguish functions that accept user input from context-only functions

### Changed

- Identity scope enforcement skipped for functions with no input parameters (only `ctx`), removing the need for dummy input structs
- `forge check` recognizes standard `#[derive(Serialize, FromRow)]` patterns alongside `#[forge::model]`
- Observability log levels upgraded: RPC request logs demoted to debug, function args demoted to debug, removed redundant success field
- Forge-idiomatic-engineer skill reference docs consolidated from 12 files into 8 topic-focused references
- Fluent builder registration methods (`register_query()`, etc.) now used in scaffolded `main.rs` templates

### Fixed

- 53 documentation discrepancies found via comprehensive code-to-docs audit across all doc pages

## [0.5.1] - 2026-03-07

### Added

- `${VAR-default}` and `${VAR:-default}` syntax in config env var substitution for fallback values when variables are unset
- Per-function metrics: `fn.executions_total` counter and `fn.duration_seconds` histogram with function name, kind, and status labels
- `db.query` tracing spans on `DbConn` methods (`fetch_one`, `fetch_all`, `fetch_optional`, `execute`) so database calls appear in traces
- `db.transaction` tracing span around transactional mutation lifecycle (BEGIN, handler, COMMIT)
- SSE connection tracking via `active_connections` gauge (increment on connect, decrement on disconnect)
- Per-signal env var control for observability: `FORGE_OTEL_TRACES`, `FORGE_OTEL_METRICS`, `FORGE_OTEL_LOGS`

### Changed

- OTLP telemetry export disabled by default; enabled via `FORGE_OTEL_ENABLED=true` env var (docker compose sets this automatically)
- RPC request log demoted to debug level since `fn.execute` already logs at info with richer context
- Function input args demoted from info to debug level to reduce log noise and avoid PII exposure
- Removed redundant `success` field from function execution logs (message already distinguishes executed vs failed)
- Config templates use env var defaults (`${FORGE_OTEL_ENABLED-false}`) instead of hardcoded `enabled = true`

## [0.5.0] - 2026-03-06

### Added

- Observability instrumentation: tracing spans on RPC handlers, job workers, and cron ticks with structured fields; Prometheus-style metrics for request count, latency, and queue depth; slow query logging with configurable threshold; startup summary banner
- Consistent query routing via `#[forge::query(consistent)]` attribute to force reads from primary, bypassing replicas for read-after-write consistency
- Health-aware replica selection: background monitor pings replicas every 15s, automatically skips unhealthy replicas and falls back to primary
- Workload-isolated connection pools (`pools.default`, `pools.jobs`, `pools.observability`, `pools.analytics`) with independent size and timeout configuration
- Coalesced real-time subscriptions: identical query subscriptions share a single re-execution instead of running per-client
- Hybrid rate limiting combining in-memory token bucket with PostgreSQL-backed sliding window for cluster-wide consistency
- Cluster-aware cache invalidation via `forge_invalidations` table so nodes only re-execute queries affected by changes on other nodes
- `forge-idiomatic-engineer` Claude Code skill shipped with scaffolded projects for AI-assisted development

### Changed

- Removed 20 direct dependencies by reimplementing minimal usages inline (async-stream, hex, regex, regex-lite, walkdir, darling, dialoguer, indicatif, hostname, arc-swap, sysinfo, slab, smallvec, once_cell, futures, axum-extra, tonic, prost, hyper, and gRPC features from opentelemetry-otlp)
- Switched OTLP telemetry transport from gRPC (port 4317) to HTTP (port 4318), eliminating duplicate transitive dependency trees (axum 0.7, tower 0.4, matchit 0.7)
- Dropped bundled `AGENTS.md` from project templates in favor of the installed skill

## [0.4.1] - 2026-02-28

### Added

- `db_conn()` method on all context types for shared helper functions across queries, mutations, jobs, webhooks, crons, MCP tools, and daemons
- Daemon contexts can now dispatch jobs and start workflows via `dispatch_job()` and `start_workflow()`
- Testcontainers support: `from_env()` auto-starts a PostgreSQL container when `TEST_DATABASE_URL` is unset and the `testcontainers` feature is enabled
- `IsolatedTestDb::setup()` convenience method combining env detection, isolation, internal SQL, and migrations in one call
- Builder API: `register_query()`, `register_mutation()`, `register_job()`, `register_cron()`, `register_workflow()`, `register_daemon()`, `register_webhook()`, `register_mcp_tool()` methods on `ForgeBuilder`
- Webhook and MCP documentation improvements

### Changed

- Simplified example `main.rs` files to use new builder registration methods
- Test database helpers cleaned up with less verbose error formatting

## [0.4.0] - 2026-02-22

### Added

- Custom HTTP handler support via `#[forge::handler]` for raw request/response control
- Prebuilt Svelte runtime shipped with the CLI (no more regenerating on every codegen)
- Configurable log level with auto-initialized tracing subscriber
- Kanban board example (renamed from trellix) with redesigned UI
- Comprehensive Playwright test suites for all examples
- MCP and custom handler documentation
- Examples README with overview of all example projects

### Changed

- `forge dev` is now docker-only, removed embedded PostgreSQL support
- Examples moved into workspace with shared workspace dependencies
- Release pipeline overhauled for docker-only dev workflow
- Example UIs redesigned, replaced JS dialogs with inline UI
- Auth middleware fixes for edge cases
- AGENTS template updated for new context methods
- Docs updated to reflect docker-only dev and new context API

### Removed

- Embedded PostgreSQL support (use Docker Compose instead)
- Standalone `Cargo.lock` files from examples (now workspace members)

## [0.3.0] - 2026-02-20

### Added

- MCP server support with `#[mcp_tool]` macro for exposing functions as MCP tools
- MCP tool registry with JSON-RPC transport over stdio and SSE
- MCP configuration in `forge.toml` (`[mcp]` section)
- MCP security documentation
- Support-desk example project demonstrating MCP integration
- Enum variant description support in `#[forge_enum]` macro via `#[description]` attribute

### Changed

- Codegen parser extracts MCP tool metadata alongside API types

## [0.2.1] - 2026-02-09

### Added

- Example project (todo) e2e testing in CI release pipeline
- Playwright test suite for the todo example

### Changed

- Reactor invalidation uses periodic flush interval instead of inline check per change
- Todo example updated to `ctx.db()` transaction-aware query API
- Docker Compose template simplified: shorter cargo-watch command, PG 18 volume path fix

### Fixed

- `forge dev` crash when `.env` file doesn't exist (cargo-watch canonicalize error)
- `forge dev` now copies `frontend/.env.example` to `frontend/.env` when missing (fixes fresh clones)

### Removed

- Stale dashboard references from templates, docs, and config

## [0.2.0] - 2026-02-07

### Added

- OTLP-based observability with tracing, metrics, and database instrumentation
- Principal ownership tracking for jobs and workflows
- Job heartbeats for stale job detection
- Configuration validation at parse time for database, cluster, and auth settings
- Error reference page and contexts reference in documentation
- Stricter clippy lints across all crates

### Changed

- `forge dev` revamped with strict-by-default ports, takeover mode, and scoped reloads
- Gateway defaults hardened for production readiness
- Logging levels reconfigured for cleaner defaults
- Macro utilities extracted into shared `forge-macros/utils.rs`
- Documentation rewritten for conciseness and consistent tone

### Fixed

- Webhook idempotency race condition in concurrent request handling
- Advisory lock session pinning for leader election reliability
- Integration tests using local workspace path instead of published crates
- Bare `unwrap()` calls replaced with `expect()` for better panic diagnostics

## [0.1.0] - 2026-02-04

### Added

- Webhook support with signatures and macro generation
- Daemon support for long-running background processes
- Job cancellation with save/saved API and TTL cleanup
- Circuit Breaker pattern for HTTP client
- Multipart file uploads with unified duration parsing
- Read replica routing
- Upload type handling in TypeScript code generation
- Mutation transaction wrapping with outbox pattern
- Playwright e2e tests for project templates
- Full-stack todo example application

### Changed

- API routes now use consistent `/_api` prefix

### Removed

- Observability system and dashboard

## [0.0.7] - 2026-01-30

### Added

- Built-in JWT auth store generation for Svelte with localStorage persistence
- Svelte 5 runes-native reactive query bindings with automatic subscription management

### Changed

- Authentication required by default, removed `allow_anonymous` config option
- Job macro validates `priority` and `backoff` attributes at compile time
- Mutation macro enforces `transactional` attribute when dispatching jobs or workflows

### Fixed

- Workflow macro validation with better error messages for `tokio::sleep()` usage

## [0.0.6] - 2026-01-29

### Added

- Inline syntax for macro attributes (e.g., `#[forge::cron("0 9 * * *", timezone = "America/New_York")]`)

### Changed

- Authentication required by default for queries, mutations, and jobs
- Null arguments normalized to empty object for proper struct deserialization

### Fixed

- Null args handling in function, job, and workflow registries

## [0.0.5] - 2026-01-24

### Added

- Token change detection for automatic SSE reconnection
- Async JWT validation with reconnection handling
- JWKS caching and external RSA provider support (Firebase, Auth0, Clerk, Supabase)

### Changed

- Auth config moved to top level in `forge.toml`
- JWT field naming prefixed (`algorithm` → `jwt_algorithm`, etc.)
- Frontend env vars renamed to `PUBLIC_API_URL` following SvelteKit conventions

### Fixed

- Docker PostgreSQL volume path corrected
- Cargo watch polling in containerized environments
- TypeScript type checking in project template

## [0.0.4] - 2026-01-20

### Added

- Datetime types: `Instant`, `LocalDate`, `LocalTime` for type-safe date/time handling
- File upload type with multipart form data support
- Auth attributes for jobs and workflows (`#[public]`, `#[require_role]`)
- Server-Sent Events (SSE) gateway for real-time communication

### Changed

- Replaced WebSocket gateway with SSE for simpler deployment
- TypeScript codegen refactored with improved type inference

### Fixed

- TypeScript codegen for single-argument functions

## [0.0.3] - 2026-01-18

### Added

- WebSocket authentication with JWT support
- RS256/JWKS asymmetric algorithm support
- Role-based access control with `require_role` attribute
- Client IP and user agent in request metadata
- Flexible JWT subject handling for non-UUID values

### Changed

- Consolidated `#[forge::action]` into `#[forge::mutation]`
- System migrations use version-based naming (v001, v002, etc.)
- Added cargo-watch hot reload to `forge dev`

### Fixed

- Authentication verification order in router
- ESLint configuration in TypeScript scaffolding

## [0.0.2] - 2026-01-11

### Added

- Per-function logging with configurable levels (trace, debug, info, warn, error, off)
- Bare metal development mode without Docker dependency
- DTO struct parsing in TypeScript codegen

### Changed

- `forge dev` runs natively by default, Docker Compose via `--docker` flag
- Dockerfile template optimized with frontend build before embedding

### Fixed

- Dockerfile build order for frontend embedding

## [0.0.1] - 2026-01-09

### Added

- Full-stack framework compiling backend into single binary with PostgreSQL
- Query and mutation system with `/rpc/` endpoints and automatic caching
- Background job queue with retry logic and exponential backoff
- Cron scheduler with timezone support and leader election
- Durable workflows with compensation logic and version tracking
- Real-time subscriptions via PostgreSQL LISTEN/NOTIFY
- Type-safe environment variable access for all function contexts
- Built-in observability dashboard with metrics, logs, and traces
- TypeScript code generation from Rust models
- CLI tool for scaffolding and codegen (`forge new`, `forge codegen`)
- Svelte 5 frontend runtime library
- Automated testing framework with TestContext

### Changed

- Library renamed from `forgex` to `forge` for cleaner imports

### Fixed

- Rust 2024 edition unsafe block compatibility
- Release workflow cargo-edit installation

[unreleased]: https://github.com/isala404/forge/compare/v0.7.0...HEAD
[0.7.0]: https://github.com/isala404/forge/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/isala404/forge/compare/v0.5.1...v0.6.0
[0.5.1]: https://github.com/isala404/forge/compare/v0.5.0...v0.5.1
[0.5.0]: https://github.com/isala404/forge/compare/v0.4.1...v0.5.0
[0.4.1]: https://github.com/isala404/forge/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/isala404/forge/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/isala404/forge/compare/v0.2.1...v0.3.0
[0.2.1]: https://github.com/isala404/forge/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/isala404/forge/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/isala404/forge/compare/v0.0.7...v0.1.0
[0.0.7]: https://github.com/isala404/forge/compare/v0.0.6...v0.0.7
[0.0.6]: https://github.com/isala404/forge/compare/v0.0.5...v0.0.6
[0.0.5]: https://github.com/isala404/forge/compare/v0.0.4...v0.0.5
[0.0.4]: https://github.com/isala404/forge/compare/v0.0.3...v0.0.4
[0.0.3]: https://github.com/isala404/forge/compare/v0.0.2...v0.0.3
[0.0.2]: https://github.com/isala404/forge/compare/v0.0.1...v0.0.2
[0.0.1]: https://github.com/isala404/forge/releases/tag/v0.0.1
