# Clarity Refinement Gate

Second-pass review for recently modified code, adapted from the `code-simplifier` skill.

Use this after implementation and tests, before final delivery.

## Goal

Improve clarity, consistency, and maintainability without changing functionality.

## Scope

- Focus on recently touched files in the current task.
- Do not broad-refactor unrelated code unless explicitly requested.

## Non-Negotiable Rule

Functionality must remain identical.

- no behavior changes
- no contract changes
- no hidden side-effect changes

## Anti-Pattern Checks

## 1) Complexity and Nesting
- reduce unnecessary nesting with early returns/guards
- flatten deeply nested conditional trees where possible
- avoid branching that duplicates the same end behavior

## 2) Clarity Over Brevity
- avoid dense one-liners that reduce readability
- avoid clever-but-opaque constructions
- prefer explicit and debuggable control flow

## 3) Conditional Expression Hygiene
- avoid nested ternary operators in TS/Svelte code
- use clear `if/else` or `switch` for multi-condition logic

## 4) Redundancy and Duplication
- remove duplicate logic introduced during edits
- consolidate repeated code into focused helpers only when it improves clarity
- avoid premature abstraction for one-off code

## 5) Naming and Intent
- use meaningful names for variables/functions/components
- avoid generic placeholders (`handle`, `data`, `tmp`) when domain names exist

## 6) Comments Quality
- remove comments that describe obvious code
- keep comments that explain rationale, invariants, and tradeoffs

## 7) Balance Check
Do not “simplify” in ways that hurt maintainability:
- do not merge unrelated concerns into one function
- do not delete useful structure that aids extension/testing
- do not optimize for fewer lines over readability

## 8) Error-Handling Clarity
- keep error mapping explicit and precise (`ForgeError` variants)
- avoid broad catch-all patterns that hide actionable causes
- preserve structured context in error logs/messages

## 9) Forge Boundary Safety
- preserve generated-code boundaries (`frontend/src/lib/forge/*` read-only)
- keep handler orchestration in `functions`, contracts in `schema`, pure helpers in `utils`
- preserve locality of behavior in `functions`; extract to `utils` only when reuse is demonstrated

## Practical Pass Checklist (Run in Order)

1. Confirm behavior unchanged (re-check tests/expected contracts).
2. Inspect recently edited blocks only.
3. Remove unnecessary nesting and duplicate branches.
4. Replace nested ternaries (frontend) with clearer control flow.
5. Improve names where meaning is ambiguous.
6. Delete obvious comments; keep rationale comments.
7. Ensure no over-compaction harmed debuggability.
8. Re-run relevant checks/tests.

## Output Requirement

When this gate is applied, summarize only meaningful refinements:
- what was simplified
- why readability/maintainability improved
- confirmation that behavior stayed the same
