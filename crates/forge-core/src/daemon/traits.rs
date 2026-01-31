use std::future::Future;
use std::pin::Pin;
use std::str::FromStr;
use std::time::Duration;

use crate::error::Result;

use super::context::DaemonContext;

/// Trait for FORGE daemon handlers.
///
/// Daemons are long-running singleton tasks that run continuously in the background.
/// They support leader election (only one instance in cluster), automatic restart
/// on panic, and graceful shutdown.
pub trait ForgeDaemon: Send + Sync + 'static {
    /// Get daemon metadata.
    fn info() -> DaemonInfo;

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
            max_restarts: None,
        }
    }
}

/// Daemon status in the cluster.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

impl FromStr for DaemonStatus {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(match s {
            "pending" => Self::Pending,
            "acquiring" => Self::Acquiring,
            "running" => Self::Running,
            "stopped" => Self::Stopped,
            "failed" => Self::Failed,
            "restarting" => Self::Restarting,
            _ => Self::Pending,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_daemon_info() {
        let info = DaemonInfo::default();
        assert!(info.leader_elected);
        assert!(info.restart_on_panic);
        assert_eq!(info.restart_delay, Duration::from_secs(5));
        assert_eq!(info.startup_delay, Duration::from_secs(0));
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
