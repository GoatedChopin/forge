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
- **all** input/output structs, domain models, enums, and data contracts live here
- `#[forge::model]` and `#[forge::forge_enum]` definitions
- input/output DTOs for handlers (even if used by only one handler)
- this is the single source of truth for type shapes; handlers import from here
- keeps function files focused on behavior, not data definitions

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

## Test placement

Tests live with the code they test using inline `#[cfg(test)] mod tests` blocks at the bottom of each file. No separate `tests/` directory for unit tests.

```rust
// src/functions/orders.rs
use crate::schema::order::CreateOrderInput;

#[forge::mutation(transactional)]
pub async fn create_order(ctx: &MutationContext, input: CreateOrderInput) -> Result<Order> {
    // ...
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn create_order_dispatches_receipt_job() {
        // ...
    }
}
```

## Rule of thumb

- If code talks to context and performs side effects, it belongs in `functions/`.
- If code models business entities/contracts, it belongs in `schema/`.
- If code is reusable, mostly pure logic, it belongs in `utils/`.
- Tests go in the same file as the code they cover.
- Do not prematurely abstract one-off function logic into `utils/`.
