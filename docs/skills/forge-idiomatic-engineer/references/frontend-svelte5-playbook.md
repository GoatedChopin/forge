# Frontend Svelte 5 Playbook

Frontend is the default deliverable for this skill.

Only skip frontend when the user explicitly requests backend-only output.

Always sequence work as:
1. Backend correctness and tests
2. Frontend integration and UX polish

## 1) Delivery and Tooling Order

### Backend-first gate
Do not start frontend implementation until backend behavior is stable and tests are added.

### Frontend CLI-first gate
Use CLI tooling for frontend quality checks.

Prefer `bun` when available:

```bash
bun install
bun run lint
bunx svelte-check
```

Fallback if bun is unavailable: use the project's configured package manager equivalents.

## 2) Svelte 5 Reactivity Defaults

Prefer runes-driven state:
- `$state` for local mutable state
- `$derived` for computed state
- `$effect` only as a last resort (non-trivial side effects that cannot be expressed declaratively)

Guideline:
- If logic can be expressed as state derivation, use `$derived`.
- If logic is event-driven, use explicit handlers.
- Use `$effect` only for unavoidable imperative side effects.

Do not add manual refetch loops after mutations when Forge reactivity can invalidate for you.

## 3) Accessibility Baseline (Non-Negotiable)

- semantic landmarks (`header`, `main`, `nav`, `footer`)
- correct heading hierarchy
- accessible names/labels for controls
- keyboard navigation and visible focus indicators
- WCAG-compliant contrast
- reduced-motion support for users preferring less animation
- `aria-live` announcements for async loading/errors/progress

## 4) SEO and Content Quality (Mandatory)

### SEO requirements
- descriptive page titles and meta descriptions
- canonical URLs when relevant
- open graph / social preview metadata
- semantic content structure for crawlability
- meaningful internal link structure when multi-page

### Copy requirements
- domain-specific, human, concise copy
- avoid generic “AI-sounding” filler and vague marketing clichés
- write for the target audience and task context
- error/help text must be actionable and precise

## 5) Visual and UX Quality (Top frontend-design advice)

Choose a clear design direction before coding and execute it intentionally.

### Design thinking before implementation
- define purpose: what user outcome this UI must enable
- define tone: pick a concrete style direction (minimal, editorial, brutalist, playful, luxury, etc.)
- define differentiation: one memorable visual/interaction signature

### Typography
- use expressive, purposeful font choices
- avoid default/generic stacks (e.g., Arial/Roboto/Inter as automatic defaults)
- pair display and body typography intentionally

### Color and Theme
- commit to a cohesive visual system
- use CSS variables for color/spacing/radius/shadow tokens
- avoid generic overused palettes, cookie-cutter gradients, and purple-on-white defaults

### Motion
- prefer a few meaningful, high-impact animations over random micro-motion
- use staggered reveals and intentional transitions
- ensure motion respects reduced-motion preferences

### Spatial Composition
- use purposeful layout decisions (rhythm, hierarchy, contrast)
- avoid interchangeable template-like sections

### Background and Atmosphere
- create depth with subtle gradients/textures/shapes when appropriate
- avoid flat, forgettable backgrounds by default

### Existing design systems
- when working in an existing app/design system, preserve established visual language and component patterns
- otherwise, avoid generic AI-slop patterns and interchangeable layouts

## 6) Generated Client Boundary

Never edit generated:
- `frontend/src/lib/forge/*`
- `frontend/.forge/svelte/*`
- `frontend/.forge/version`

Instead:
- edit app code in `frontend/src/routes/*` and non-generated `frontend/src/lib/*`
- re-run `forge generate` after backend changes

## 7) Svelte + Forge Integration Patterns

- use generated runes/stores from Forge client
- keep subscription args stable to reduce unnecessary resubscribe churn
- derive UI from server state; minimize duplicate client truth
- validate on client for UX, rely on backend validation for correctness
- explicitly render loading/error/stale/empty states

## 8) Frontend Acceptance Checklist

- backend completed and tested first
- frontend lint and `svelte-check` pass
- keyboard and screen-reader baseline verified
- loading/error/stale/empty states present
- no manual refetch anti-pattern where generated reactivity exists
- SEO metadata and semantic structure present
- copy is concise, specific, and human
- visual direction is intentional and distinctive
