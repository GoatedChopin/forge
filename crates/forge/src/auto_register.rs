//! Automatic function registration via the `inventory` crate.
//!
//! Each `#[forge::query]`, `#[forge::mutation]`, etc. macro emits an
//! `inventory::submit!` call that registers a factory function. Calling
//! `ForgeBuilder::auto_register()` collects all submitted entries and
//! registers them with the appropriate registry.

use forge_runtime::cron::CronRegistry;
use forge_runtime::daemon::DaemonRegistry;
use forge_runtime::function::FunctionRegistry;
use forge_runtime::jobs::JobRegistry;
use forge_runtime::mcp::McpToolRegistry;
use forge_runtime::webhook::WebhookRegistry;
use forge_runtime::workflow::WorkflowRegistry;

pub struct AutoQuery(pub fn(&mut FunctionRegistry));
pub struct AutoMutation(pub fn(&mut FunctionRegistry));
pub struct AutoJob(pub fn(&mut JobRegistry));
pub struct AutoCron(pub fn(&mut CronRegistry));
pub struct AutoWorkflow(pub fn(&mut WorkflowRegistry));
pub struct AutoDaemon(pub fn(&mut DaemonRegistry));
pub struct AutoWebhook(pub fn(&mut WebhookRegistry));
pub struct AutoMcpTool(pub fn(&mut McpToolRegistry));

inventory::collect!(AutoQuery);
inventory::collect!(AutoMutation);
inventory::collect!(AutoJob);
inventory::collect!(AutoCron);
inventory::collect!(AutoWorkflow);
inventory::collect!(AutoDaemon);
inventory::collect!(AutoWebhook);
inventory::collect!(AutoMcpTool);

/// Register all auto-discovered functions into the builder's registries.
pub fn auto_register_all(
    functions: &mut FunctionRegistry,
    jobs: &mut JobRegistry,
    crons: &mut CronRegistry,
    workflows: &mut WorkflowRegistry,
    daemons: &mut DaemonRegistry,
    webhooks: &mut WebhookRegistry,
    mcp_tools: &mut McpToolRegistry,
) {
    for entry in inventory::iter::<AutoQuery> {
        (entry.0)(functions);
    }
    for entry in inventory::iter::<AutoMutation> {
        (entry.0)(functions);
    }
    for entry in inventory::iter::<AutoJob> {
        (entry.0)(jobs);
    }
    for entry in inventory::iter::<AutoCron> {
        (entry.0)(crons);
    }
    for entry in inventory::iter::<AutoWorkflow> {
        (entry.0)(workflows);
    }
    for entry in inventory::iter::<AutoDaemon> {
        (entry.0)(daemons);
    }
    for entry in inventory::iter::<AutoWebhook> {
        (entry.0)(webhooks);
    }
    for entry in inventory::iter::<AutoMcpTool> {
        (entry.0)(mcp_tools);
    }
}
