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

impl From<crate::http::CircuitBreakerError> for ForgeError {
    fn from(e: crate::http::CircuitBreakerError) -> Self {
        match e {
            crate::http::CircuitBreakerError::CircuitOpen(open) => {
                ForgeError::Timeout(open.to_string())
            }
            crate::http::CircuitBreakerError::Request(err) if err.is_timeout() => {
                ForgeError::Timeout(err.to_string())
            }
            crate::http::CircuitBreakerError::Request(err) => ForgeError::Internal(err.to_string()),
        }
    }
}

/// Result type alias using ForgeError.
pub type Result<T> = std::result::Result<T, ForgeError>;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]
mod tests {
    use super::*;

    // --- Display / error messages ---

    #[test]
    fn display_preserves_inner_message() {
        let cases: Vec<(ForgeError, &str)> = vec![
            (
                ForgeError::Config("bad toml".into()),
                "Configuration error: bad toml",
            ),
            (
                ForgeError::Database("conn refused".into()),
                "Database error: conn refused",
            ),
            (
                ForgeError::Function("handler panic".into()),
                "Function error: handler panic",
            ),
            (ForgeError::Job("timeout".into()), "Job error: timeout"),
            (
                ForgeError::JobCancelled("user request".into()),
                "Job cancelled: user request",
            ),
            (
                ForgeError::Cluster("split brain".into()),
                "Cluster error: split brain",
            ),
            (
                ForgeError::Serialization("bad json".into()),
                "Serialization error: bad json",
            ),
            (
                ForgeError::Deserialization("missing field".into()),
                "Deserialization error: missing field",
            ),
            (
                ForgeError::InvalidArgument("negative id".into()),
                "Invalid argument: negative id",
            ),
            (ForgeError::NotFound("user 42".into()), "Not found: user 42"),
            (
                ForgeError::Unauthorized("expired token".into()),
                "Unauthorized: expired token",
            ),
            (
                ForgeError::Forbidden("admin only".into()),
                "Forbidden: admin only",
            ),
            (
                ForgeError::Validation("email required".into()),
                "Validation error: email required",
            ),
            (
                ForgeError::Timeout("5s exceeded".into()),
                "Timeout: 5s exceeded",
            ),
            (
                ForgeError::Internal("null pointer".into()),
                "Internal error: null pointer",
            ),
            (
                ForgeError::InvalidState("already completed".into()),
                "Invalid state: already completed",
            ),
            (ForgeError::WorkflowSuspended, "Workflow suspended"),
        ];

        for (error, expected) in cases {
            assert_eq!(error.to_string(), expected, "Display mismatch for variant");
        }
    }

    #[test]
    fn display_rate_limit_includes_retry_after() {
        let err = ForgeError::RateLimitExceeded {
            retry_after: Duration::from_secs(30),
            limit: 100,
            remaining: 0,
        };
        let msg = err.to_string();
        assert!(msg.contains("30"), "Expected retry_after in message: {msg}");
    }

    // --- From implementations ---

    #[test]
    fn from_serde_json_error_maps_to_serialization() {
        let bad_json = serde_json::from_str::<serde_json::Value>("not json").unwrap_err();
        let forge_err: ForgeError = bad_json.into();
        match forge_err {
            ForgeError::Serialization(msg) => assert!(!msg.is_empty()),
            other => panic!("Expected Serialization, got: {other:?}"),
        }
    }

    #[test]
    fn from_io_error_maps_to_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let forge_err: ForgeError = io_err.into();
        match forge_err {
            ForgeError::Io(e) => assert_eq!(e.kind(), std::io::ErrorKind::NotFound),
            other => panic!("Expected Io, got: {other:?}"),
        }
    }

    #[test]
    fn from_circuit_breaker_open_maps_to_timeout() {
        let open = crate::http::CircuitBreakerError::CircuitOpen(crate::http::CircuitBreakerOpen {
            host: "api.example.com".into(),
            retry_after: Duration::from_secs(60),
        });
        let forge_err: ForgeError = open.into();
        match forge_err {
            ForgeError::Timeout(msg) => {
                assert!(
                    msg.contains("api.example.com"),
                    "Expected host in message: {msg}"
                );
            }
            other => panic!("Expected Timeout, got: {other:?}"),
        }
    }

    // --- Variant matching (critical for downstream error handling) ---

    #[test]
    fn variants_are_distinguishable_via_pattern_match() {
        let errors: Vec<ForgeError> = vec![
            ForgeError::NotFound("x".into()),
            ForgeError::Unauthorized("x".into()),
            ForgeError::Forbidden("x".into()),
            ForgeError::Validation("x".into()),
            ForgeError::InvalidArgument("x".into()),
            ForgeError::Timeout("x".into()),
            ForgeError::Internal("x".into()),
        ];

        // Each variant must match only its own pattern
        for (i, err) in errors.iter().enumerate() {
            let matched = match err {
                ForgeError::NotFound(_) => 0,
                ForgeError::Unauthorized(_) => 1,
                ForgeError::Forbidden(_) => 2,
                ForgeError::Validation(_) => 3,
                ForgeError::InvalidArgument(_) => 4,
                ForgeError::Timeout(_) => 5,
                ForgeError::Internal(_) => 6,
                _ => usize::MAX,
            };
            assert_eq!(matched, i, "Variant at index {i} matched wrong pattern");
        }
    }

    #[test]
    fn rate_limit_fields_accessible() {
        let err = ForgeError::RateLimitExceeded {
            retry_after: Duration::from_secs(60),
            limit: 100,
            remaining: 0,
        };

        match err {
            ForgeError::RateLimitExceeded {
                retry_after,
                limit,
                remaining,
            } => {
                assert_eq!(retry_after, Duration::from_secs(60));
                assert_eq!(limit, 100);
                assert_eq!(remaining, 0);
            }
            _ => panic!("Expected RateLimitExceeded"),
        }
    }

    #[test]
    fn error_is_send_and_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        // ForgeError must be Send+Sync for use across async task boundaries
        assert_send::<ForgeError>();
        assert_sync::<ForgeError>();
    }
}
