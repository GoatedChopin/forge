# Frontend Svelte 5 Playbook

Frontend is the default deliverable for full-stack or user-facing Forge work.

Only skip frontend when the user explicitly requests backend-only output or the existing task is clearly backend-scoped. Maintain a strict boundary between handwritten app UI and generated Forge client code.

Always sequence work as:
1. Backend correctness and tests
2. Thin frontend integration
3. UX polish after the vertical slice works

Do not start frontend implementation before backend behavior/tests are correct.

## 1) Delivery and Tooling Order

### Backend-first gate
Do not start frontend implementation until backend behavior is stable and tests are added.

### Greenfield vertical-slice gate
For a new app, ship the smallest usable path first:
- auth or entry flow
- one core read path
- one core write path
- one Playwright path proving the app works end to end

Do not jump straight into a large one-file UI with every feature and all styling decisions at once.

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
- Prefer consuming generated Forge stores directly in components over copying them into extra local state unless adaptation is necessary.

Do not add manual refetch loops after mutations when Forge reactivity can invalidate for you.

## SvelteKit Navigation

The default ESLint config enforces `svelte/no-navigation-without-resolve`. All navigation must use `resolve()` from `$app/paths` to handle base path correctly.

### goto() calls

```typescript
import { goto } from "$app/navigation"
import { resolve } from "$app/paths"

// correct
goto(resolve("/login"))

// wrong - lint error
goto("/login")
```

### Link hrefs

```svelte
<script>
  import { resolve } from "$app/paths"
</script>

<!-- correct -->
<a href={resolve("/register")}>Register</a>

<!-- wrong - lint error -->
<a href="/register">Register</a>
```

Note: `resolve` is the function name, not `resolveRoute` or `base`. The ESLint rule specifically checks for references to the `resolve` export from `$app/paths`.

## 3) Page and Component Structure

Forge apps use SvelteKit file-based routing. Split pages by feature so URLs are shareable and components stay maintainable.

### Route layout

Each distinct view gets its own `+page.svelte`. Group related pages under a shared layout.

```
frontend/src/
├── lib/
│   ├── forge/                    # Generated (DO NOT EDIT)
│   ├── components/
│   │   ├── TodoForm.svelte       # Create/edit form
│   │   ├── TodoItem.svelte       # Single item display + actions
│   │   └── EmptyState.svelte     # Reusable empty state
│   ├── auth.svelte.ts            # Auth store (handwritten)
│   └── utils/
│       └── format.ts             # Pure helpers
├── routes/
│   ├── +layout.svelte            # ForgeProvider + nav shell
│   ├── +layout.ts                # export const ssr = false
│   ├── +page.svelte              # Landing / dashboard
│   ├── login/
│   │   └── +page.svelte          # Login form
│   ├── register/
│   │   └── +page.svelte          # Registration form
│   ├── todos/
│   │   ├── +page.svelte          # Todo list (shareable URL)
│   │   └── [id]/
│   │       └── +page.svelte      # Single todo detail
│   └── settings/
│       └── +page.svelte          # User settings
└── tests/
    └── app.spec.ts               # Playwright E2E
```

### Why this matters

- Each page has a stable URL users can bookmark and share
- Components under `$lib/components/` are reusable across pages
- Page files stay small because logic is pushed into components
- Layout wraps everything with ForgeProvider and shared navigation

### Page file guidelines

Keep `+page.svelte` files thin. They wire up data subscriptions and compose components.

```svelte
<!-- routes/todos/+page.svelte -->
<script lang="ts">
  import { listTodos$ } from "$lib/forge"
  import { getAuthStore } from "$lib/auth.svelte"
  import TodoForm from "$lib/components/TodoForm.svelte"
  import TodoItem from "$lib/components/TodoItem.svelte"
  import EmptyState from "$lib/components/EmptyState.svelte"

  const auth = getAuthStore()
  const todos = listTodos$()
</script>

<main>
  <h1>My Todos</h1>
  <TodoForm />

  {#if todos.data?.length}
    {#each todos.data as todo (todo.id)}
      <TodoItem {todo} />
    {/each}
  {:else if !todos.loading}
    <EmptyState message="No todos yet" />
  {/if}
</main>
```

### Component guidelines

- One responsibility per component. If you need "and" to describe it, split it.
- Keep components under 150 lines. Extract subcomponents when they grow.
- Colocate related components in feature folders when the app grows large.
- Components subscribe to Forge stores independently when they need their own data.
- Pass data down via `$props()`, emit events up via callback props.

```svelte
<!-- $lib/components/TodoItem.svelte -->
<script lang="ts">
  import { deleteTodo } from "$lib/forge"
  import type { Todo } from "$lib/forge"

  let { todo }: { todo: Todo } = $props()
  let deleting = $state(false)

  async function handleDelete() {
    deleting = true
    await deleteTodo({ todo_id: todo.id, user_id: todo.user_id })
    deleting = false
  }
</script>

<div>
  <span>{todo.title}</span>
  <button onclick={handleDelete} disabled={deleting}>Delete</button>
</div>
```

### Shared layouts

Use `+layout.svelte` at route group boundaries for shared chrome (nav, sidebar, auth guards).

```svelte
<!-- routes/+layout.svelte -->
<script lang="ts">
  import { ForgeProvider } from "$lib/forge"
  import { PUBLIC_API_URL } from "$env/static/public"
  import { createAuthStore } from "$lib/auth.svelte"
  import { resolve } from "$app/paths"

  let { children } = $props()
  const auth = createAuthStore()
  auth.hydrate()
</script>

<ForgeProvider url={PUBLIC_API_URL} getToken={() => auth.getToken()}>
  <nav>
    <a href={resolve("/")}>Home</a>
    <a href={resolve("/todos")}>Todos</a>
    {#if auth.isAuthenticated}
      <button onclick={() => auth.logout()}>Logout</button>
    {:else}
      <a href={resolve("/login")}>Login</a>
    {/if}
  </nav>

  {@render children()}
</ForgeProvider>
```

### When to split vs. inline

| Signal | Action |
|---|---|
| Two pages need the same UI block | Extract to `$lib/components/` |
| Page file exceeds 150 lines | Extract the largest section into a component |
| A section has its own state + handlers | It's a component |
| Pure display, no state, under 30 lines | Inline is fine |

## 4) Accessibility Baseline (Non-Negotiable)

- semantic landmarks (`header`, `main`, `nav`, `footer`)
- correct heading hierarchy
- accessible names/labels for controls
- keyboard navigation and visible focus indicators
- WCAG-compliant contrast
- reduced-motion support for users preferring less animation
- `aria-live` announcements for async loading/errors/progress

## 5) SEO and Content Quality (Mandatory)

### SEO requirements
- descriptive page titles and meta descriptions
- canonical URLs when relevant
- open graph / social preview metadata
- semantic content structure for crawlability
- meaningful internal link structure when multi-page

### Copy requirements
- domain-specific, human, concise copy
- avoid generic "AI-sounding" filler and vague marketing cliches
- write for the target audience and task context
- error/help text must be actionable and precise

## 6) Visual and UX Quality

Choose a clear design direction before coding and execute it intentionally. The goal is to produce interfaces that feel crafted, not generated.

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

## 7) Generated Client Boundary

Never edit generated:
- `frontend/src/lib/forge/*`
- `frontend/.forge/svelte/*`
- `frontend/.forge/version`

Instead:
- edit app code in `frontend/src/routes/*` and non-generated `frontend/src/lib/*`
- re-run `forge generate` after backend changes

If the generated client is missing, resolve `forge generate` instead of authoring fake generated bindings by hand in those paths.

## 8) Svelte + Forge Integration Patterns

- use generated runes/stores from Forge client
- keep subscription args stable to reduce unnecessary resubscribe churn
- derive UI from server state; minimize duplicate client truth
- validate on client for UX, rely on backend validation for correctness
- explicitly render loading/error/stale/empty states

## 9) Frontend Acceptance Checklist

- backend completed and tested first
- frontend lint and `svelte-check` pass
- keyboard and screen-reader baseline verified
- loading/error/stale/empty states present
- no manual refetch anti-pattern where generated reactivity exists
- SEO metadata and semantic structure present
- copy is concise, specific, and human
- visual direction is intentional and distinctive
