
use std::marker::PhantomData;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::ForgeClient;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ForgeError {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ForgeClientError {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl ForgeClientError {
    pub fn new(
        code: impl Into<String>,
        message: impl Into<String>,
        details: Option<serde_json::Value>,
    ) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details,
        }
    }

    pub fn as_forge_error(&self) -> ForgeError {
        ForgeError {
            code: self.code.clone(),
            message: self.message.clone(),
            details: self.details.clone(),
        }
    }
}

impl std::fmt::Display for ForgeClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ForgeClientError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    #[default]
    Disconnected,
    Connecting,
    Connected,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueryState<T> {
    pub loading: bool,
    pub data: Option<T>,
    pub error: Option<ForgeError>,
}

impl<T> Default for QueryState<T> {
    fn default() -> Self {
        Self {
            loading: true,
            data: None,
            error: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubscriptionState<T> {
    pub loading: bool,
    pub data: Option<T>,
    pub error: Option<ForgeError>,
    pub stale: bool,
    pub connection_state: ConnectionState,
}

impl<T> Default for SubscriptionState<T> {
    fn default() -> Self {
        Self {
            loading: true,
            data: None,
            error: None,
            stale: false,
            connection_state: ConnectionState::Disconnected,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Pending,
    Claimed,
    Running,
    Completed,
    Retry,
    Failed,
    DeadLetter,
    CancelRequested,
    Cancelled,
    NotFound,
}

impl Default for JobStatus {
    fn default() -> Self {
        Self::Pending
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobState<TOutput> {
    pub job_id: String,
    pub status: JobStatus,
    pub progress: Option<f64>,
    pub message: Option<String>,
    pub output: Option<TOutput>,
    pub error: Option<String>,
}

impl<TOutput> Default for JobState<TOutput> {
    fn default() -> Self {
        Self {
            job_id: String::new(),
            status: JobStatus::Pending,
            progress: None,
            message: None,
            output: None,
            error: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobExecutionState<TOutput> {
    pub loading: bool,
    pub connection_state: ConnectionState,
    pub state: JobState<TOutput>,
}

impl<TOutput> Default for JobExecutionState<TOutput> {
    fn default() -> Self {
        Self {
            loading: true,
            connection_state: ConnectionState::Disconnected,
            state: JobState::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStatus {
    Created,
    Running,
    Waiting,
    Completed,
    Compensating,
    Compensated,
    Failed,
    NotFound,
}

impl Default for WorkflowStatus {
    fn default() -> Self {
        Self::Created
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WorkflowStepState {
    pub name: String,
    pub status: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowState<TOutput> {
    pub workflow_id: String,
    pub status: WorkflowStatus,
    pub step: Option<String>,
    pub waiting_for: Option<String>,
    pub steps: Vec<WorkflowStepState>,
    pub output: Option<TOutput>,
    pub error: Option<String>,
}

impl<TOutput> Default for WorkflowState<TOutput> {
    fn default() -> Self {
        Self {
            workflow_id: String::new(),
            status: WorkflowStatus::Created,
            step: None,
            waiting_for: None,
            steps: Vec::new(),
            output: None,
            error: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowExecutionState<TOutput> {
    pub loading: bool,
    pub connection_state: ConnectionState,
    pub state: WorkflowState<TOutput>,
}

impl<TOutput> Default for WorkflowExecutionState<TOutput> {
    fn default() -> Self {
        Self {
            loading: true,
            connection_state: ConnectionState::Disconnected,
            state: WorkflowState::default(),
        }
    }
}

/// An access token + refresh token pair returned by auth endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
}

/// Mutation handle returned by `use_forge_mutation`. Clone into event handlers,
/// call `.call(args)` to execute.
#[derive(Clone)]
pub struct Mutation<A, R> {
    client: ForgeClient,
    function_name: &'static str,
    _phantom: PhantomData<fn(A) -> R>,
}

impl<A, R> Mutation<A, R>
where
    A: Serialize + 'static,
    R: DeserializeOwned + 'static,
{
    pub(crate) fn new(client: ForgeClient, function_name: &'static str) -> Self {
        Self {
            client,
            function_name,
            _phantom: PhantomData,
        }
    }

    pub async fn call(&self, args: A) -> Result<R, ForgeClientError> {
        self.client.call(self.function_name, args).await
    }
}

#[derive(Debug, Clone)]
pub enum StreamEvent<T> {
    Connection(ConnectionState),
    Data(T),
    Error(ForgeClientError),
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RpcEnvelopeRaw {
    pub success: bool,
    #[serde(default)]
    pub data: Option<serde_json::Value>,
    #[serde(default)]
    pub error: Option<ForgeError>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ConnectedEvent {
    pub session_id: Option<String>,
    pub session_secret: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SseEnvelopeRaw {
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub payload: Option<serde_json::Value>,
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
}
