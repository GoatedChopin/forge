# Scenarios

Use these realistic scenarios as execution anchors.

## 0) Default full-stack delivery

User: "Add project status feature."

Expected:
- if no `[auth]` config exists and user didn't ask for public-only, set up HS256 auth with register/login mutations and a sample user
- implement backend behavior and tests first
- verify backend edge cases against requirements
- run `forge check` from app root and clear findings
- run `forge generate` after backend verification
- then implement frontend integration with generated Forge reactivity
- wire up auth store on frontend if auth was added
- run frontend CLI checks (`eslint`, `svelte-check`, prefer `bun` when available)
- if UI exists/changed, add and run at least one Playwright integration path

## 1) Owner-scoped query

User: "Add list query for my projects."

Expected:
- implement query in `src/functions/` and matching contracts in `src/schema/` if needed
- compare input scope with authenticated principal
- deterministic ordering
- tests for success and scope mismatch

## 1.5) Scaffold a new model or handler

User: "Add an invoice model and a create_invoice mutation."

Expected:
- create/update `src/schema/invoice.rs` and `src/functions/invoice.rs`
- update module wiring intentionally
- implement fields, behavior, tests, and rerun `forge generate`

## 2) Mutation with async side effect

User: "Create order and enqueue receipt email."

Expected:
- `#[forge::mutation(transactional)]`
- insert + dispatch in same transaction boundary
- dispatch assertion test

## 3) Stripe webhook

User: "Add Stripe webhook with retry-safe behavior."

Expected:
- hand-author webhook in `src/functions/` and wire it into registry
- signature verification + idempotency
- minimal sync work, dispatch job

## 4) Refactor duplicated handlers

User: "query/mutation/MCP share logic; clean this up."

Expected:
- extract shared `DbConn<'_>` helper
- keep handlers thin
- preserve behavior with tests

## 5) Read replica-safe design

User: "We enabled replicas; some pages look stale after writes."

Expected:
- add `#[forge::query(consistent)]` to strict read-after-write endpoints (confirmation screens, post-mutation lookups)
- leave dashboards and listings on default replica routing
- use explicit column lists on frequently-invalidated queries to leverage column-aware invalidation
- configure pool isolation if background jobs risk starving user-facing queries
- document lag assumptions

## 6) Observability hardening

User: "Incidents are hard to trace across requests/jobs."

Expected:
- include request/trace IDs and entity/run IDs in logs
- propagate correlation headers to external calls
- check observability config and sampling

## 7) Svelte 5 reactive UX

User: "Make dashboard reactive and beautiful."

Expected:
- use generated Forge reactivity + Svelte runes
- keep `$effect` usage minimal and only for unavoidable side effects
- preserve accessibility semantics and keyboard support
- clear loading/error/stale states
- include SEO metadata and careful human copy
- distinctive intentional visual direction

## 8) Enforce generated-code boundaries

User: "Can you patch frontend/src/lib/forge/api.ts directly?"

Expected:
- reject direct edit
- modify backend source and rerun `forge generate`

## 9) Project structure migration

User: "Move this app to functions/schema/utils layout."

Expected:
- move modules intentionally
- update `mod` wiring/import paths
- preserve behavior with tests

## 10) Docs uncertainty

User: "Use latest Svelte/SDK API behavior."

Expected:
- run Context7 lookup flow
- record what changed due to live docs

## 11) Mandatory quality gate execution

User: "Before merging, verify quality gates."

Expected:
- run `forge check` from app root (`forge.toml` location)
- fix findings and rerun until clean
- run changed-scope tests
- produce 100% line coverage for changed modules (or report explicit blocker)
- if UI exists or changed, run Playwright integration tests
