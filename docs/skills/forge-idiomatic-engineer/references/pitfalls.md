# Pitfalls

This reference documents common mistakes, build failures, and runtime errors encountered when developing with Forge. Adhering to these guidelines will help you avoid wasted iterations and maintain a stable codebase.

**Murphy's Law**: Always assume the happy path will fail. Entities will vanish, tokens will expire, and networks will drop. Code defensively by checking existence and handling errors explicitly.

## 1. Generated Code

- **Do not edit generated files**: Never modify files in `frontend/src/lib/forge/` (SvelteKit) or `frontend/src/forge/` (Dioxus) because they are automatically overwritten by `forge generate`. Instead, fix the issue in the Rust source and regenerate the bindings.
- **Always run `forge generate` after backend changes**: Failing to regenerate after modifying a query, mutation, model, or enum will cause runtime deserialization errors due to stale frontend types.
- **Ensure correct attribute ordering**: The `#[forge::model]` macro must be the first attribute applied to a struct. Placing other derives above it will cause compiler errors because the model macro needs to process the struct before the derives are expanded.

```rust
// INCORRECT
#[derive(Debug, Clone)]
#[forge::model]
pub struct Item { ... }

// CORRECT
#[forge::model]
#[derive(Debug, Clone)]
pub struct Item { ... }
```

## 2. Environment & Configuration

- **Use `ctx.env_*()` instead of `std::env::var()`**: See [API Reference](./api.md#environment-variables).
- **Avoid duplicating configuration logic**: Copying logic such as `app_url()` across handlers creates maintenance debt. Extract shared helpers into a utility module and pass them as parameters where necessary.
- **Use `ctx.http()` for external requests**: See [API Reference](./api.md#http-client).

## 3. Macros & Registration

- **Avoid redundant handler naming**: Do not include the handler type in the function name (e.g., use `heartbeat`, not `heartbeat_daemon`). Forge macros automatically append the appropriate suffix, preventing generated names like `HeartbeatDaemonDaemon`.
- **Declare handlers as `pub async fn`**: Forge generates public structs for these handlers; if the function itself is private, the generated code will fail to compile.
- **Use quoted strings for log levels**: Attributes like `#[forge::query(log = "info")]` require the log level to be a quoted string.
- **Omit unnecessary parameters**: Do not use dummy structs or `Option<()>` if a handler requires no input. Omit the parameter entirely to reduce boilerplate and validation overhead.
- **Explicitly mark handlers as `public`**: Handlers require authentication by default. Only use the `public` attribute when you explicitly intend to allow unauthenticated access.
- **Always register handlers in `main.rs`**: Defining a handler with a macro is insufficient; it must be registered via `.register_query::<NameQuery>()` or `auto_register()` to be reachable at runtime.

## 4. Database & Transactions

- **Use compile-time SQL macros**: Avoid `sqlx::query()` or `sqlx::query_as()` because they lack build-time type safety. Always use the bang-macros like `sqlx::query!()` and run `forge migrate prepare` after any changes.
- **Use explicit casts for SQL enums**: Explicitly cast enum columns (e.g., `status as "status: ScanStatus"`) to help SQLx map types correctly. Use the `"column?"` suffix only to override nullability inference.
- **Dispatch jobs only inside transactions**: See [Patterns](./patterns.md#background-job-implementation).
- **Use `ctx.conn()` for transactional mutations**: Mutations require a transaction-aware connection. Use `ctx.conn()` and pass it to your queries to ensure they participate in the active transaction.

```rust
let mut conn = ctx.conn().await?;
sqlx::query_as!(User, "...", id).fetch_one(&mut conn).await
```

- **Use a real database for tests**: Mocking the database hides migration bugs and constraint violations. Use `TestQueryContext` or `TestMutationContext` with an `IsolatedTestDb` to run tests against a real PostgreSQL instance.
- **Enable reactivity via migrations**: Do not hand-write reactivity triggers. Use `SELECT forge_enable_reactivity('table_name');` in your migrations to ensure the Reactor correctly detects data changes.
- **Avoid `SELECT *` in subscriptions**: Returning all columns triggers unnecessary table-level invalidations. Use explicit column lists to enable granular, column-aware invalidation.
- **Create dedicated structs for enriched queries**: When joining data from multiple tables, do not return base models. Create a dedicated struct (e.g., `SiteSummary`) to ensure joined data is not silently dropped.

## 5. Workflows

- **Use `ctx.sleep()` instead of `tokio::sleep`**: Standard library sleep functions do not persist across restarts. Use `ctx.sleep()` to ensure the workflow can resume correctly after a process restart.
- **Keep step names stable**: Steps are cached by their name to enable resumption. Renaming a step will break the cache and prevent the workflow from resuming correctly.
- **Version workflows upon contract changes**: If you change step keys, wait keys, or data types, you must bump the workflow version. A signature mismatch will prevent the application from starting.
- **Verify active runs before removing workflow versions**: Removing a version that still has active runs will cause them to be blocked, and `/_api/ready` will return a 503 error.

## 6. Frontend

- **Do not manually refetch reactive queries**: Calling `refetch()` on an SSE-backed store causes unnecessary traffic and UI flickering. The SSE stream handles updates automatically.
- **Guard against null subscription states**: See [Frontend Playbook](./frontend.md#subscription-state-shape).
- **Recreate the Forge client on authentication changes**: See [Frontend Playbook](./frontend.md#authentication-and-session-management).
- **Avoid fetching data in effects**: Fetching inside `$effect` or `use_effect` leads to race conditions and memory leaks. Use reactive stores or Forge hooks instead.
- **Handle mutation errors globally**: Users must be notified when an operation fails. Use a global `onMutationError` handler to provide consistent feedback for all mutation failures.

## 7. Authentication

- **Define TTLs in `forge.toml`**: The `issue_token_pair()` function will fail at runtime if `access_token_ttl` or `refresh_token_ttl` are not configured.
- **Release database connections before token issuance**: See [Patterns Reference](./patterns.md#2-authentication-and-authorization).
- **Use anonymous clients for token refresh**: Including expired tokens in headers during a refresh call will cause the request to be rejected.
- **Reserve `Forbidden` for permission violations**: Do not use `Forbidden` errors for business logic (e.g., "account needs upgrade"), as this triggers the global `onAuthError` handler and logs the user out.

## 8. Resilience and Hygiene

- **Consistently check for ID existence**: See [Resilience Patterns](./resilience.md#2-database-and-data-integrity).
- **Include context in error messages**: Always include relevant IDs and context in your error messages to make debugging easier for both developers and users.
- **Maintain clean code**: Delete unused code immediately and run `forge check` constantly to identify orphaned modules or functions. Do not leave commented-out code in the repository; rely on Git for history.
- **Survive stale frontend state**: Ensure the UI can handle scenarios where data is deleted or tokens expire during a user's session. Always provide clear, visible feedback for loading and error states.
