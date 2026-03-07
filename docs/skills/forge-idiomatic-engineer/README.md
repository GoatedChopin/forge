# forge-idiomatic-engineer

Forge-specific engineering skill for maintainable, secure, observable, testable, and scalable Rust backends with frontend-inclusive Svelte 5 delivery by default.

## Package Contents

- `SKILL.md`: Non-negotiable rules, inline quick reference for common tasks, signal-based reference routing
- `references/auth.md`: complete auth setup (config, backend, frontend, verification)
- `references/patterns.md`: backend patterns (jobs, crons, workflows, DbConn, validation, consistency, logging)
- `references/integrations.md`: webhooks, HTTP handlers, daemons, MCP tools, file uploads, external services
- `references/frontend.md`: Svelte 5 playbook (reactivity, components, accessibility, design quality)
- `references/testing.md`: test strategy, examples, Playwright patterns
- `references/quality.md`: review rubric, anti-patterns, security/scale checklists, clarity gate
- `references/config.md`: forge.toml schema, context API, error types, CLI workflow, project structure
- `references/operations.md`: read replicas, observability, tracing, production hardening
- `evals/evals.json`: starter eval prompts

## Progressive Discovery

SKILL.md is self-contained for common tasks (adding queries, mutations, small refactors). References load only when the task requires specialized depth. See the "Reference Loading" section in SKILL.md for the signal-based routing table.

## Core Guarantees

- Testability-first architecture
- Strict auth/scope and transactional correctness
- Read replica and observability awareness
- Trace-friendly operational behavior
- `forge generate`-first backend/frontend generation workflow
- Generated-code-safe implementation
- Mandatory `forge check` gate before completion
- Mandatory Playwright integration coverage for UI work
- Intentional, non-generic design at every layer
