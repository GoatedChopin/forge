use std::future::Future;
use std::pin::Pin;
use std::str::FromStr;
use std::time::Duration;

use crate::error::Result;
use crate::metadata::HandlerMetadata;

use super::context::DaemonContext;

/// Trait for FORGE daemon handlers.
///
/// Daemons are long-running singleton tasks that run continuously in the background.
/// They support leader election (only one instance in cluster), automatic restart
/// on panic, and graceful shutdown.
pub trait ForgeDaemon: crate::__sealed::Sealed + Send + Sync + 'static {
    /// Get daemon metadata.
    fn info() -> DaemonInfo;

    /// Unified metadata for uniform consumers (observability, admin, codegen).
    fn metadata() -> HandlerMetadata {
        HandlerMetadata::from(&Self::info())
    }

    /// Execute the daemon.
    ///
    /// The daemon should run in a loop and check `ctx.shutdown_signal()` to handle
    /// graceful shutdown. Example:
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
    fn execute(ctx: &DaemonContext) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>>;
}

/// Daemon metadata.
///
/// Constructed by the `#[daemon]` macro. Adding a field is a breaking change
/// for hand-written `ForgeDaemon` impls; stage extensions through a builder or
/// major bump.
#[derive(Debug, Clone)]
pub struct DaemonInfo {
    /// Daemon name (used for identification and leader election).
    pub name: &'static str,
    /// Whether only one instance should run across the cluster.
    pub leader_elected: bool,
    /// Whether to restart the daemon if it panics.
    pub restart_on_panic: bool,
    /// Delay before restarting after a failure.
    pub restart_delay: Duration,
    /// Delay before first execution after startup.
    pub startup_delay: Duration,
    /// Default timeout for outbound HTTP requests made by the daemon.
    pub http_timeout: Option<Duration>,
    /// Maximum number of restarts (None = unlimited).
    pub max_restarts: Option<u32>,
}

impl Default for DaemonInfo {
    fn default() -> Self {
        Self {
            name: "",
            leader_elected: true,
            restart_on_panic: true,
            restart_delay: Duration::from_secs(5),
            startup_delay: Duration::from_secs(0),
            http_timeout: None,
            max_restarts: None,
        }
    }
}

/// Daemon status in the cluster.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DaemonStatus {
    /// Waiting to start (startup delay).
    Pending,
    /// Acquiring leader lock.
    Acquiring,
    /// Currently running.
    Running,
    /// Stopped gracefully.
    Stopped,
    /// Stopped due to failure.
    Failed,
    /// Waiting to restart after failure.
    Restarting,
}

impl DaemonStatus {
    /// Convert to database string.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Acquiring => "acquiring",
            Self::Running => "running",
            Self::Stopped => "stopped",
            Self::Failed => "failed",
            Self::Restarting => "restarting",
        }
    }
}

/// Error returned when parsing an unknown daemon status string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseDaemonStatusError(pub String);

impl std::fmt::Display for ParseDaemonStatusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown daemon status: {:?}", self.0)
    }
}

impl std::error::Error for ParseDaemonStatusError {}

impl FromStr for DaemonStatus {
    type Err = ParseDaemonStatusError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "pending" => Ok(Self::Pending),
            "acquiring" => Ok(Self::Acquiring),
            "running" => Ok(Self::Running),
            "stopped" => Ok(Self::Stopped),
            "failed" => Ok(Self::Failed),
            "restarting" => Ok(Self::Restarting),
            _ => Err(ParseDaemonStatusError(s.to_string())),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn test_default_daemon_info() {
        let info = DaemonInfo::default();
        assert!(info.leader_elected);
        assert!(info.restart_on_panic);
        assert_eq!(info.restart_delay, Duration::from_secs(5));
        assert_eq!(info.startup_delay, Duration::from_secs(0));
        assert_eq!(info.http_timeout, None);
        assert!(info.max_restarts.is_none());
    }

    #[test]
    fn test_status_conversion() {
        assert_eq!(DaemonStatus::Running.as_str(), "running");
        assert_eq!("running".parse::<DaemonStatus>(), Ok(DaemonStatus::Running));
        assert_eq!(DaemonStatus::Failed.as_str(), "failed");
        assert_eq!("failed".parse::<DaemonStatus>(), Ok(DaemonStatus::Failed));
    }
}
