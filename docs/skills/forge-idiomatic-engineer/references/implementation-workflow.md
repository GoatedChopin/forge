# Implementation Workflow

Follow these steps in order for any Forge task.

## Step 1: Restate Contract

Before edits, state:
- input/output behavior
- auth and scope model
- transaction and durability boundary
- side effects
- failure behavior
- observability expectations

## Step 2: Design

Plan:
- invariants to preserve
- data ownership checks
- read consistency needs (primary vs replica)
- retry/idempotency/timeouts
- test strategy (unit + integration)

## Step 3: Implement and Verify in Order

3.1 Backend implementation first:
- author or update backend contracts/handlers in `src/schema/` and `src/functions/`
- implement backend behavior to match requirements exactly
- preserve trace context and structured identifiers in logs
- keep function-local behavior together in `src/functions/` unless reuse is proven

3.2 Backend verification before frontend:
- test happy path + critical failure path minimum
- test relevant edge cases explicitly
- add DB integration tests for SQL-heavy or transactional logic
- assert side effects (`assert_job_dispatched!`, `assert_workflow_started!`, HTTP assertions)

3.3 Generate clients after backend is verified:
- run `forge generate` once backend contracts are correct

3.4 Frontend implementation after client generation:
- implement or update frontend integration only after backend is validated

## Step 4: Tests and Coverage

- run backend/frontend tests for touched areas
- enforce 100% line coverage for changed modules (or fail explicitly with blocker details)
- for frontend work, run lint/type checks and verify accessibility + reactive states
- if UI exists or changed, add at least one basic Playwright integration path (happy path + one failure/empty/loading branch) and run Playwright
- **Playwright and test failures are blockers.** If any test fails, diagnose the root cause, fix the code or test, and rerun until all tests pass. Do not proceed to the next step or report the task as complete while tests are failing. A passing test suite is a hard prerequisite for delivery.

## Step 5: Security + Scale + Observability Gate

- scope safety, authz, and least privilege
- transaction correctness
- replica consistency safety
- traceable logs/telemetry fields
- bounded queries and operational limits

## Step 5.5: Clarity Refinement Gate

- Run `references/clarity-refinement-gate.md` against recently modified code.
- Remove clarity anti-patterns without changing behavior.
- Ensure simplification improves maintainability, not just line count.

## Step 6: Final `forge check` Gate (non-negotiable)

This is the absolute last step before delivery. Nothing else runs after this.

1. Run `forge check` from the app root (`forge.toml` directory).
2. Read the output. Fix every error and actionable warning.
3. Run `forge check` again.
4. Repeat until the output is fully clean. Do not proceed with any findings remaining.
5. If a finding cannot be resolved, report it as an explicit blocker in the delivery output.

**The task is not complete until `forge check` and all tests (including Playwright) pass cleanly.** Do not summarize, report delivery, or tell the user the work is done while any check or test is still failing. Fix first, then report.

## Step 7: Explain Delivery

Output:
1. Contract summary
2. Changes made
3. Test coverage
4. Security/scale/observability checks
5. `forge check` result (must be clean, or blocker explanation)
6. Remaining risks

For review tasks, findings first by severity.
