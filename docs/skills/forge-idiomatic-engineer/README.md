# forge-idiomatic-engineer

Forge-specific engineering skill for maintainable, secure, observable, testable, and scalable Rust backends with frontend-inclusive Svelte 5 delivery by default (backend-only only when explicitly requested).

## Package Contents

- `SKILL.md`: Non-negotiable rules and reference loading map (kept lean)
- `references/forge-defaults.md`: auth, scope, error, observability, and frontend defaults
- `references/implementation-workflow.md`: step-by-step implementation and delivery process
- `references/decision-tree.md`: primitive and implementation choices
- `references/feature-differentiation.md`: function-type comparison matrix
- `references/pattern-cookbook.md`: idiomatic Forge code patterns
- `references/testing-playbook.md`: test strategy and examples
- `references/security-and-scale.md`: security and performance checklists
- `references/read-replicas-observability-tracing.md`: production operations guidance
- `references/config-context-error-reference.md`: config options, contexts, and error conditions
- `references/cli-and-generated-code.md`: CLI workflow, generated-code boundaries, project layout, migration cleanup
- `references/project-structure.md`: preferred `functions/schema/utils` layout
- `references/frontend-svelte5-playbook.md`: reactive, accessible, polished frontend guidance
- `references/context7-live-docs.md`: live documentation lookup workflow
- `references/anti-patterns.md`: failure patterns and corrections
- `references/clarity-refinement-gate.md`: second-pass simplification and anti-pattern review
- `references/review-rubric.md`: structured review criteria
- `references/scenarios.md`: realistic task examples
- `evals/evals.json`: starter eval prompts

## Core Guarantees

- Testability-first architecture
- Strict auth/scope and transactional correctness
- Read replica and observability awareness
- Trace-friendly operational behavior
- `forge generate`-first backend/frontend generation workflow
- Generated-code-safe implementation
- Mandatory app-root `forge check` gate before completion
- Mandatory changed-scope tests, coverage proof, and Playwright integration coverage for UI work
