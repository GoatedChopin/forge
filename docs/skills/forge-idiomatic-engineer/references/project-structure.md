# Project Structure Standard

Preferred app layout:

```text
src/
  main.rs
  functions/
    mod.rs
    ...
  schema/
    mod.rs
    ...
  utils/
    mod.rs
    ...
```

## Folder Responsibilities

## `src/functions/`
- Forge handlers only:
  - queries
  - mutations
  - jobs
  - crons
  - workflows
  - webhooks
  - MCP tools
- locality-of-behavior default:
  - keep function-specific validation, orchestration, and helper logic near that function
  - colocate related behavior until clear reuse appears
  - extract only when genuinely shared

## `src/schema/`
- domain structs/enums and data contracts
- `#[forge::model]` and `#[forge::forge_enum]` definitions
- input/output DTOs used across handlers

## `src/utils/`
- pure helper functions
- input normalization and validation helpers
- formatting/mapping utilities
- no framework-heavy coupling unless justified

## Example module wiring

```rust
// src/main.rs
mod functions;
mod schema;
mod utils;
```

```rust
// src/functions/mod.rs
pub mod orders;
pub mod users;
```

```rust
// src/schema/mod.rs
pub mod order;
pub mod user;
```

```rust
// src/utils/mod.rs
pub mod validation;
pub mod ids;
```

## Migration Guidance for Existing Repos

If existing code does not match this layout:
- migrate intentionally in one refactor with module wiring updates + tests
- avoid partial folder moves that leave mixed conventions

## Rule of thumb

- If code talks to context and performs side effects, it belongs in `functions/`.
- If code models business entities/contracts, it belongs in `schema/`.
- If code is reusable, mostly pure logic, it belongs in `utils/`.
- Do not prematurely abstract one-off function logic into `utils/`.
