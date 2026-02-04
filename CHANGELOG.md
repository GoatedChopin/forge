# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[unreleased]: https://github.com/isala404/forge/compare/v0.0.7...HEAD
[0.0.7]: https://github.com/isala404/forge/compare/v0.0.6...v0.0.7
[0.0.6]: https://github.com/isala404/forge/compare/v0.0.5...v0.0.6
[0.0.5]: https://github.com/isala404/forge/compare/v0.0.4...v0.0.5
[0.0.4]: https://github.com/isala404/forge/compare/v0.0.3...v0.0.4
[0.0.3]: https://github.com/isala404/forge/compare/v0.0.2...v0.0.3
[0.0.2]: https://github.com/isala404/forge/compare/v0.0.1...v0.0.2
[0.0.1]: https://github.com/isala404/forge/releases/tag/v0.0.1
