# Decision Tree

Pick the smallest correct solution.

## 1) Is this backend-only or full-stack?

- User explicitly requests backend-only: do backend-only.
- Otherwise: deliver backend + frontend integration by default.
- Execution order is strict: backend correctness/tests first, frontend second.

## 2) Which Forge primitive fits?

- read data: `#[forge::query]`
- write data: `#[forge::mutation]`
- async work: `#[forge::job]`
- recurring schedule: `#[forge::cron]`
- multi-step durable process: `#[forge::workflow]`
- external callback endpoint: `#[forge::webhook]`
- agent-callable tool: `#[forge::mcp_tool]`

If confused, check `feature-differentiation.md`.

## 3) Auth and scope decision

- Keep default auth unless truly public.
- Need sensitive operation? add `require_role("...")`.
- UUID identity available? use `require_user_id()`.
- Non-UUID provider? use `require_subject()`.
- Client sends `user_id`/`tenant_id`? compare with principal and reject mismatch.

## 4) Transaction decision for mutations

Use `transactional` if:
- multiple related writes
- dispatching jobs/start_workflow
- consistency must be all-or-nothing

## 5) Read consistency decision

- Need immediate read-your-write: use `#[forge::query(consistent)]` to force primary reads
- Eventual consistency acceptable: default replica routing is fine
- Hot-path query on replicated table: use explicit column list (not `SELECT *`) to benefit from column-aware invalidation
- Mixed workloads risk pool starvation: configure `database.pools.*` isolation

## 6) Reactivity decision for queries

- Static SQL: rely on extraction.
- Dynamic SQL: set `tables = [...]` explicitly.

## 7) Async resilience decision

- external I/O or flakiness: set retries + timeouts
- duplication risk: idempotent key strategy
- long jobs: progress + cancellation checks

## 8) Docs certainty decision

- API may have changed or uncertain? use `context7-live-docs.md` workflow.

## 9) Frontend implementation decision

For non-backend-only requests:
- follow `frontend-svelte5-playbook.md`
- use CLI checks (`eslint`, `svelte-check`, prefer `bun` when available)
- enforce SEO + accessibility + copy-quality standards
- use `$effect` minimally and only when needed

## 10) Testing depth decision

Minimum always:
- happy-path tests for main success scenarios
- failure-path tests for each distinct error condition (validation, authz, not found)
- boundary value tests when inputs have limits or special values

Add pure unit tests when:
- handler contains non-trivial business logic (extract to utils, test there)
- validation has boundary conditions (empty, max length, negative values)
- calculations or state transitions have edge cases

Add integration tests when:
- SQL ownership/joins/CTEs are complex
- transaction/outbox behavior matters
- read consistency behavior changed

The test suite should make regressions structurally difficult. If someone can break behavior without a test failing, coverage is insufficient.
