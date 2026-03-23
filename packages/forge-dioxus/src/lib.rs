pub mod auth;
mod client;
mod context;
mod hooks;
mod types;
mod upload;

pub use auth::{ForgeAuth, ForgeAuthProvider, ForgeAuthState, use_auth_key, use_forge_auth, use_viewer};
#[cfg(feature = "router")]
pub use auth::use_require_auth;
pub use client::{ForgeClient, ForgeClientConfig, SubscriptionHandle};
pub use context::{ForgeProvider, use_connection_state, use_forge_client};
pub use hooks::{
    use_forge_job, use_forge_job_signal, use_forge_mutation, use_forge_query,
    use_forge_query_signal, use_forge_subscription, use_forge_subscription_signal,
    use_forge_workflow, use_forge_workflow_signal,
};
pub use types::{
    ConnectionState, ForgeClientError, ForgeError, JobExecutionState, JobState, JobStatus,
    Mutation, QueryState, StreamEvent, SubscriptionState, WorkflowExecutionState, WorkflowState,
    WorkflowStatus, WorkflowStepState,
};
pub use upload::ForgeUpload;
