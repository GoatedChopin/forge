# Review Rubric

Use this for review tasks. Findings first, summary second.

## Severity levels

- P0: security breach/data corruption/tenant isolation break
- P1: correctness bug likely in normal usage
- P2: reliability/operability/maintainability issue with meaningful impact
- P3: style or minor quality issue

## Review order

1. Auth/authz and scope isolation
2. Transaction and side-effect atomicity
3. Data correctness and edge-case design
4. Read consistency (primary vs replica assumptions)
5. Retry/timeouts/idempotency for async surfaces
6. Observability and trace correlation quality
7. Test coverage for changed behavior
8. Readability and future change cost
9. Frontend a11y/reactivity quality when UI changes are in scope

## Required checks by primitive

### Query
- scope-safe?
- explicit `tables` when SQL dynamic?
- bounded data path?
- `consistent` attribute present on read-after-write paths?
- explicit column list on hot-path queries (avoids unnecessary re-execution via column-aware invalidation)?
- consistency expectation documented when replicas in play?

### Mutation
- should be transactional?
- precise error mapping?
- side effects asserted in tests?

### Job
- retry policy appropriate?
- cancellation/progress for long-running work?
- idempotency and observability context present?

### Cron
- timezone intentional?
- catch-up intentional?
- late-run behavior visible in logs/metrics?

### Workflow
- step names stable?
- compensation needed/present?
- timeout realistic?

### Webhook
- signature + idempotency configured?
- minimal sync work and safe async dispatch?

### MCP
- least privilege?
- authz + rate limit + timeout present?

### Frontend (if changed)
- generated code untouched?
- runes/reactivity idiomatic?
- accessible states and keyboard support present?

## Review output template

- Finding title
- Severity
- File and line reference
- Why it matters
- Concrete fix suggestion
