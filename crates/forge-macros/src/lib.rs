use proc_macro::TokenStream;

mod cron;
mod daemon;
mod enum_type;
mod job;
mod model;
mod mutation;
mod query;
mod sql_extractor;
mod webhook;
mod workflow;

/// Marks a struct as a FORGE model, generating schema metadata for TypeScript codegen.
///
/// # Example
/// ```ignore
/// #[forge::model]
/// pub struct User {
///     pub id: Uuid,
///     pub email: String,
///     pub name: String,
///     pub created_at: DateTime<Utc>,
/// }
/// ```
#[proc_macro_attribute]
pub fn model(attr: TokenStream, item: TokenStream) -> TokenStream {
    model::expand_model(attr, item)
}

/// Marks an enum for database storage as a PostgreSQL ENUM type.
///
/// # Example
/// ```ignore
/// #[forge::forge_enum]
/// pub enum ProjectStatus {
///     Draft,
///     Active,
///     Paused,
///     Completed,
/// }
/// ```
#[proc_macro_attribute]
pub fn forge_enum(attr: TokenStream, item: TokenStream) -> TokenStream {
    enum_type::expand_enum(attr, item)
}

/// Marks a function as a query (read-only, cacheable, subscribable).
///
/// # Authentication
/// By default, queries require an authenticated user. Override with:
/// - `public` - No authentication required
/// - `require_role("admin")` - Require specific role
///
/// # Attributes
/// - `cache = "5m"` - Cache TTL (duration like "30s", "5m", "1h")
/// - `log` - Enable logging for this query
/// - `timeout = 30` - Timeout in seconds
/// - `tables = ["users", "projects"]` - Explicit table dependencies (for dynamic SQL)
///
/// # Table Dependency Extraction
/// By default, table dependencies are automatically extracted from SQL strings
/// in the function body at compile time. This enables accurate reactive
/// subscription invalidation for queries that join multiple tables.
///
/// For dynamic SQL (e.g., table names built at runtime), use the `tables`
/// attribute to explicitly specify dependencies.
///
/// # Example
/// ```ignore
/// #[forge::query]  // Requires authenticated user (default)
/// pub async fn get_user(ctx: &QueryContext, user_id: Uuid) -> Result<User> {
///     // Tables automatically extracted from SQL
/// }
///
/// #[forge::query(public)]  // No auth required
/// pub async fn get_public_data(ctx: &QueryContext) -> Result<Data> {
///     // ...
/// }
///
/// #[forge::query(require_role("admin"), cache = "5m", log)]
/// pub async fn admin_stats(ctx: &QueryContext) -> Result<Stats> {
///     // Requires admin role
/// }
///
/// #[forge::query(tables = ["users", "audit_log"])]
/// pub async fn dynamic_query(ctx: &QueryContext, table: String) -> Result<Vec<Row>> {
///     // Explicit tables for dynamic SQL
/// }
/// ```
#[proc_macro_attribute]
pub fn query(attr: TokenStream, item: TokenStream) -> TokenStream {
    query::expand_query(attr, item)
}

/// Marks a function as a mutation (transactional write).
///
/// Mutations run within a database transaction. All changes commit together or roll back on error.
///
/// # Authentication
/// By default, mutations require an authenticated user. Override with:
/// - `public` - No authentication required
/// - `require_role("admin")` - Require specific role
///
/// # Attributes
/// - `log` - Enable logging for this mutation
/// - `timeout = 30` - Timeout in seconds
///
/// # Example
/// ```ignore
/// #[forge::mutation]  // Requires authenticated user (default)
/// pub async fn create_project(ctx: &MutationContext, input: CreateProjectInput) -> Result<Project> {
///     let user_id = ctx.require_user_id()?;
///     // ...
/// }
///
/// #[forge::mutation(public)]  // No auth required
/// pub async fn submit_feedback(ctx: &MutationContext, input: FeedbackInput) -> Result<()> {
///     // ...
/// }
///
/// #[forge::mutation(require_role("admin"), log)]
/// pub async fn delete_user(ctx: &MutationContext, user_id: Uuid) -> Result<()> {
///     // Requires admin role
/// }
/// ```
#[proc_macro_attribute]
pub fn mutation(attr: TokenStream, item: TokenStream) -> TokenStream {
    mutation::expand_mutation(attr, item)
}

/// Marks a function as a background job.
///
/// Jobs are durable background tasks that survive server restarts and automatically retry on failure.
///
/// # Authentication
/// By default, jobs require an authenticated user to dispatch. Override with:
/// - `public` - Can be dispatched without authentication
/// - `require_role("admin")` - Requires specific role to dispatch
///
/// # Attributes
/// - `timeout = "30m"` - Job timeout (supports s, m, h suffixes)
/// - `priority = "normal"` - background, low, normal, high, critical
/// - `max_attempts = 3` - Maximum retry attempts
/// - `backoff = "exponential"` - fixed, linear, or exponential
/// - `max_backoff = "5m"` - Maximum backoff duration
/// - `retry(max_attempts = 3, backoff = "exponential", max_backoff = "5m")` - Grouped retry config
/// - `worker_capability = "media"` - Required worker capability
/// - `idempotent` - Mark job as idempotent
/// - `idempotent(key = "input.id")` - Idempotent with custom key
/// - `name = "custom_name"` - Override job name
///
/// # Example
/// ```ignore
/// #[forge::job(timeout = "30m", priority = "high")]  // Requires authenticated user (default)
/// pub async fn send_welcome_email(ctx: &JobContext, input: SendEmailInput) -> Result<()> {
///     // ...
/// }
///
/// #[forge::job(public)]  // Can be dispatched without auth
/// pub async fn process_webhook(ctx: &JobContext, input: WebhookInput) -> Result<()> {
///     // ...
/// }
///
/// #[forge::job(retry(max_attempts = 5, backoff = "exponential"), require_role("admin"))]
/// pub async fn process_payment(ctx: &JobContext, input: PaymentInput) -> Result<()> {
///     // Requires admin role to dispatch
/// }
/// ```
#[proc_macro_attribute]
pub fn job(attr: TokenStream, item: TokenStream) -> TokenStream {
    job::job_impl(attr, item)
}

/// Marks a function as a scheduled cron task.
///
/// Cron jobs run on a schedule, exactly once per scheduled time across the cluster.
///
/// # Attributes
/// All attributes are specified inline within the macro:
/// - First argument: Cron schedule expression (required)
/// - `timezone = "UTC"` - Timezone for the schedule
/// - `timeout = "1h"` - Execution timeout
/// - `catch_up` - Run missed executions after downtime
/// - `catch_up_limit = 10` - Maximum number of catch-up runs
///
/// # Example
/// ```ignore
/// #[forge::cron("0 0 * * *", timezone = "America/New_York", timeout = "30m", catch_up)]
/// pub async fn daily_cleanup(ctx: &CronContext) -> Result<()> {
///     // ...
/// }
/// ```
#[proc_macro_attribute]
pub fn cron(attr: TokenStream, item: TokenStream) -> TokenStream {
    cron::cron_impl(attr, item)
}

/// Marks a function as a durable workflow.
///
/// Workflows are multi-step processes that survive restarts and handle failures with compensation.
///
/// # Authentication
/// By default, workflows require an authenticated user to start. Override with:
/// - `public` - Can be started without authentication
/// - `require_role("admin")` - Requires specific role to start
///
/// # Attributes
/// - `version = 1` - Workflow version (increment for breaking changes)
/// - `timeout = "24h"` - Maximum execution time
/// - `name = "custom_name"` - Override workflow name
///
/// # Example
/// ```ignore
/// #[forge::workflow(version = 1, timeout = "24h")]  // Requires authenticated user (default)
/// pub async fn user_onboarding(ctx: &WorkflowContext, input: OnboardingInput) -> Result<OnboardingResult> {
///     let user = ctx.step("create_user", || async { /* ... */ }).await?;
///     ctx.step("send_welcome", || async { /* ... */ }).await;
///     Ok(OnboardingResult { user })
/// }
///
/// #[forge::workflow(public)]  // Can be started without auth
/// pub async fn process_webhook(ctx: &WorkflowContext, input: WebhookInput) -> Result<()> {
///     // ...
/// }
///
/// #[forge::workflow(version = 2, require_role("admin"))]
/// pub async fn admin_workflow(ctx: &WorkflowContext, input: AdminInput) -> Result<AdminResult> {
///     // Requires admin role to start
/// }
/// ```
#[proc_macro_attribute]
pub fn workflow(attr: TokenStream, item: TokenStream) -> TokenStream {
    workflow::workflow_impl(attr, item)
}

/// Marks a function as a long-running daemon.
///
/// Daemons are singleton background tasks that run continuously. They support
/// leader election (only one instance runs across the cluster), automatic restart
/// on panic, and graceful shutdown handling.
///
/// # Attributes
/// - `leader_elected = true` - Only one instance runs across cluster (default: true)
/// - `restart_on_panic = true` - Restart if daemon panics (default: true)
/// - `restart_delay = "5s"` - Delay before restart after failure
/// - `startup_delay = "10s"` - Delay before first execution after startup
/// - `max_restarts = 10` - Maximum restart attempts (default: unlimited)
///
/// # Shutdown Handling
/// Daemons must handle graceful shutdown by checking `ctx.shutdown_signal()`:
///
/// ```ignore
/// loop {
///     // Do work
///     tokio::select! {
///         _ = tokio::time::sleep(Duration::from_secs(60)) => {}
///         _ = ctx.shutdown_signal() => break,
///     }
/// }
/// ```
///
/// # Example
/// ```ignore
/// #[forge::daemon(startup_delay = "5s", restart_on_panic = true)]
/// pub async fn heartbeat_daemon(ctx: &DaemonContext) -> Result<()> {
///     loop {
///         // Update heartbeat
///         sqlx::query("UPDATE app_status SET heartbeat = NOW()").execute(ctx.db()).await?;
///
///         tokio::select! {
///             _ = tokio::time::sleep(Duration::from_secs(30)) => {}
///             _ = ctx.shutdown_signal() => break,
///         }
///     }
///     Ok(())
/// }
///
/// #[forge::daemon(leader_elected = false, max_restarts = 5)]
/// pub async fn worker_daemon(ctx: &DaemonContext) -> Result<()> {
///     // Runs on all nodes, limited restarts
/// }
/// ```
#[proc_macro_attribute]
pub fn daemon(attr: TokenStream, item: TokenStream) -> TokenStream {
    daemon::daemon_impl(attr, item)
}

/// Marks a function as a webhook handler.
///
/// Webhooks are HTTP endpoints for receiving external events (e.g., from Stripe, GitHub).
/// They support signature validation, idempotency, and bypass authentication middleware.
///
/// # Attributes
/// - `path = "/webhooks/stripe"` - URL path (required)
/// - `signature = WebhookSignature::hmac_sha256("Header", "SECRET_ENV")` - Signature validation
/// - `idempotency = "header:X-Request-Id"` - Idempotency key source
/// - `timeout = "30s"` - Request timeout
///
/// # Signature Validation
/// Use `WebhookSignature` helper for common patterns:
/// - `WebhookSignature::hmac_sha256("X-Hub-Signature-256", "GITHUB_SECRET")` - GitHub
/// - `WebhookSignature::hmac_sha256("X-Stripe-Signature", "STRIPE_SECRET")` - Stripe
/// - `WebhookSignature::hmac_sha1("X-Signature", "SECRET")` - Legacy SHA1
///
/// # Idempotency
/// Specify source as `"header:Header-Name"` or `"body:$.json.path"`:
/// - `"header:X-GitHub-Delivery"` - From header
/// - `"body:$.id"` - From JSON body field
///
/// # Example
/// ```ignore
/// #[forge::webhook(
///     path = "/webhooks/github",
///     signature = WebhookSignature::hmac_sha256("X-Hub-Signature-256", "GITHUB_SECRET"),
///     idempotency = "header:X-GitHub-Delivery",
/// )]
/// pub async fn github_webhook(ctx: &WebhookContext, payload: Value) -> Result<WebhookResult> {
///     let event_type = ctx.header("X-GitHub-Event").unwrap_or("unknown");
///     ctx.dispatch_job("process_github_event", &payload).await?;
///     Ok(WebhookResult::Accepted)
/// }
///
/// #[forge::webhook(path = "/webhooks/stripe", timeout = "60s")]
/// pub async fn stripe_webhook(ctx: &WebhookContext, payload: Value) -> Result<WebhookResult> {
///     // Process Stripe event
///     Ok(WebhookResult::Ok)
/// }
/// ```
#[proc_macro_attribute]
pub fn webhook(attr: TokenStream, item: TokenStream) -> TokenStream {
    webhook::webhook_impl(attr, item)
}
