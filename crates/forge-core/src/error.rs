use std::time::Duration;

use thiserror::Error;

/// Core error type for Forge operations.
///
/// Each variant maps to an HTTP status code and error code for consistent client handling.
#[derive(Error, Debug)]
pub enum ForgeError {
    /// Configuration file parsing or validation failed.
    #[error("Configuration error: {0}")]
    Config(String),

    /// Database operation failed.
    #[error("Database error: {0}")]
    Database(String),

    /// Function execution failed.
    #[error("Function error: {0}")]
    Function(String),

    /// Job execution failed.
    #[error("Job error: {0}")]
    Job(String),

    /// Job was cancelled before completion.
    #[error("Job cancelled: {0}")]
    JobCancelled(String),

    /// Cluster coordination failed.
    #[error("Cluster error: {0}")]
    Cluster(String),

    /// Failed to serialize data to JSON.
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Failed to deserialize JSON input.
    #[error("Deserialization error: {0}")]
    Deserialization(String),

    /// File system operation failed.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// SQL execution failed.
    #[error("SQL error: {0}")]
    Sql(#[from] sqlx::Error),

    /// Invalid argument provided (400).
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),

    /// Requested resource not found (404).
    #[error("Not found: {0}")]
    NotFound(String),

    /// Authentication required or failed (401).
    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    /// Permission denied (403).
    #[error("Forbidden: {0}")]
    Forbidden(String),

    /// Input validation failed (400).
    #[error("Validation error: {0}")]
    Validation(String),

    /// Operation timed out (504).
    #[error("Timeout: {0}")]
    Timeout(String),

    /// Unexpected internal error (500).
    #[error("Internal error: {0}")]
    Internal(String),

    /// Invalid state transition attempted.
    #[error("Invalid state: {0}")]
    InvalidState(String),

    /// Internal signal for workflow suspension. Never returned to clients.
    #[error("Workflow suspended")]
    WorkflowSuspended,

    /// Rate limit exceeded (429).
    #[error("Rate limit exceeded: retry after {retry_after:?}")]
    RateLimitExceeded {
        /// How long to wait before retrying.
        retry_after: Duration,
        /// The configured request limit.
        limit: u32,
        /// Remaining requests (always 0 when exceeded).
        remaining: u32,
    },
}

impl From<serde_json::Error> for ForgeError {
    fn from(e: serde_json::Error) -> Self {
        ForgeError::Serialization(e.to_string())
    }
}

/// Result type alias using ForgeError.
pub type Result<T> = std::result::Result<T, ForgeError>;
