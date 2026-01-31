//! Registry for daemon handlers.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use forge_core::daemon::{DaemonContext, DaemonInfo, ForgeDaemon};
use forge_core::Result;

/// Type alias for boxed daemon handler functions.
pub type BoxedDaemonHandler = Arc<
    dyn Fn(&DaemonContext) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>>
        + Send
        + Sync,
>;

/// Entry in the daemon registry.
pub struct DaemonEntry {
    /// Daemon metadata.
    pub info: DaemonInfo,
    /// Daemon handler function.
    pub handler: BoxedDaemonHandler,
}

/// Registry for storing and retrieving daemon handlers.
#[derive(Clone, Default)]
pub struct DaemonRegistry {
    daemons: HashMap<String, Arc<DaemonEntry>>,
}

impl DaemonRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a daemon handler.
    pub fn register<D: ForgeDaemon>(&mut self) {
        let info = D::info();
        let name = info.name.to_string();

        let handler: BoxedDaemonHandler = Arc::new(move |ctx| D::execute(ctx));

        self.daemons.insert(name, Arc::new(DaemonEntry { info, handler }));
    }

    /// Get a daemon entry by name.
    pub fn get(&self, name: &str) -> Option<Arc<DaemonEntry>> {
        self.daemons.get(name).cloned()
    }

    /// Get daemon info by name.
    pub fn info(&self, name: &str) -> Option<&DaemonInfo> {
        self.daemons.get(name).map(|e| &e.info)
    }

    /// Check if a daemon exists.
    pub fn exists(&self, name: &str) -> bool {
        self.daemons.contains_key(name)
    }

    /// Get all daemon names.
    pub fn daemon_names(&self) -> impl Iterator<Item = &str> {
        self.daemons.keys().map(|s| s.as_str())
    }

    /// Get all daemons.
    pub fn daemons(&self) -> impl Iterator<Item = (&str, &Arc<DaemonEntry>)> {
        self.daemons.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// Get the number of registered daemons.
    pub fn len(&self) -> usize {
        self.daemons.len()
    }

    /// Check if registry is empty.
    pub fn is_empty(&self) -> bool {
        self.daemons.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_new() {
        let registry = DaemonRegistry::new();
        assert!(registry.is_empty());
    }
}
