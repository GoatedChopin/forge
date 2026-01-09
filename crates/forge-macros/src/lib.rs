use proc_macro::TokenStream;

mod action;
mod cron;
mod enum_type;
mod job;
mod model;
mod mutation;
mod query;
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
/// # Attributes
/// - `cache = "5m"` - Cache TTL (duration like "30s", "5m", "1h")
/// - `public` - No authentication required
/// - `require_auth` - Require authentication
/// - `timeout = 30` - Timeout in seconds
///
/// # Example
/// ```ignore
/// #[forge::query]
/// pub async fn get_user(ctx: &QueryContext, user_id: Uuid) -> Result<User> {
///     // ...
/// }
///
/// #[forge::query(cache = "5m", require_auth)]
/// pub async fn get_profile(ctx: &QueryContext) -> Result<Profile> {
///     let user_id = ctx.require_user_id()?;
///     // ...
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
/// # Attributes
/// - `require_auth` - Require authentication
/// - `require_role("admin")` - Require specific role
/// - `timeout = 30` - Timeout in seconds
///
/// # Example
/// ```ignore
/// #[forge::mutation]
/// pub async fn create_project(ctx: &MutationContext, input: CreateProjectInput) -> Result<Project> {
///     let user_id = ctx.require_user_id()?;
///     // ...
/// }
/// ```
#[proc_macro_attribute]
pub fn mutation(attr: TokenStream, item: TokenStream) -> TokenStream {
    mutation::expand_mutation(attr, item)
}

/// Marks a function as an action (side effects, external APIs).
///
/// Actions can call external APIs and perform side effects. NOT transactional.
///
/// # Attributes
/// - `require_auth` - Require authentication
/// - `require_role("admin")` - Require specific role
/// - `timeout = 60` - Timeout in seconds
///
/// # Example
/// ```ignore
/// #[forge::action(timeout = 60)]
/// pub async fn sync_with_stripe(ctx: &ActionContext, user_id: Uuid) -> Result<SyncResult> {
///     let customer = ctx.http().get("https://api.stripe.com/...").await?;
///     // ...
/// }
/// ```
#[proc_macro_attribute]
pub fn action(attr: TokenStream, item: TokenStream) -> TokenStream {
    action::expand_action(attr, item)
}

/// Marks a function as a background job.
///
/// Jobs are durable background tasks that survive server restarts and automatically retry on failure.
///
/// # Attributes
/// - `timeout = "30m"` - Job timeout
/// - `priority = "normal"` - background, low, normal, high, critical
/// - `max_attempts = 3` - Maximum retry attempts
/// - `worker_capability = "general"` - Required worker capability
///
/// # Example
/// ```ignore
/// #[forge::job]
/// #[timeout = "30m"]
/// #[priority = "high"]
/// pub async fn send_welcome_email(ctx: &JobContext, input: SendEmailInput) -> Result<()> {
///     // ...
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
/// - `timezone = "UTC"` - Timezone for the schedule
/// - `catch_up` - Run missed executions after downtime
/// - `timeout = "1h"` - Execution timeout
///
/// # Example
/// ```ignore
/// #[forge::cron("0 0 * * *")]
/// #[timezone = "UTC"]
/// #[catch_up]
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
/// # Attributes
/// - `version = 1` - Workflow version (increment for breaking changes)
/// - `timeout = "24h"` - Maximum execution time
///
/// # Example
/// ```ignore
/// #[forge::workflow]
/// #[version = 1]
/// pub async fn user_onboarding(ctx: &WorkflowContext, input: OnboardingInput) -> Result<OnboardingResult> {
///     let user = ctx.step("create_user", || async { /* ... */ }).await?;
///     ctx.step("send_welcome", || async { /* ... */ }).await;
///     Ok(OnboardingResult { user })
/// }
/// ```
#[proc_macro_attribute]
pub fn workflow(attr: TokenStream, item: TokenStream) -> TokenStream {
    workflow::workflow_impl(attr, item)
}
