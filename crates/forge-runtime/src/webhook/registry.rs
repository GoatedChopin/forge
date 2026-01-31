//! Registry for webhook handlers.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use forge_core::webhook::{ForgeWebhook, WebhookContext, WebhookInfo, WebhookResult};
use forge_core::Result;
use serde_json::Value;

/// Type alias for boxed webhook handler functions.
pub type BoxedWebhookHandler = Arc<
    dyn Fn(&WebhookContext, Value) -> Pin<Box<dyn Future<Output = Result<WebhookResult>> + Send + '_>>
        + Send
        + Sync,
>;

/// Entry in the webhook registry.
pub struct WebhookEntry {
    /// Webhook metadata.
    pub info: WebhookInfo,
    /// Webhook handler function.
    pub handler: BoxedWebhookHandler,
}

/// Registry for storing and retrieving webhook handlers.
#[derive(Clone, Default)]
pub struct WebhookRegistry {
    webhooks: HashMap<String, Arc<WebhookEntry>>,
    /// Index by path for routing.
    by_path: HashMap<String, String>,
}

impl WebhookRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a webhook handler.
    pub fn register<W: ForgeWebhook>(&mut self) {
        let info = W::info();
        let name = info.name.to_string();
        let path = info.path.to_string();

        let handler: BoxedWebhookHandler = Arc::new(move |ctx, payload| W::execute(ctx, payload));

        self.by_path.insert(path, name.clone());
        self.webhooks.insert(name, Arc::new(WebhookEntry { info, handler }));
    }

    /// Get a webhook entry by name.
    pub fn get(&self, name: &str) -> Option<Arc<WebhookEntry>> {
        self.webhooks.get(name).cloned()
    }

    /// Get a webhook entry by path.
    pub fn get_by_path(&self, path: &str) -> Option<Arc<WebhookEntry>> {
        self.by_path.get(path).and_then(|name| self.get(name))
    }

    /// Get webhook info by name.
    pub fn info(&self, name: &str) -> Option<&WebhookInfo> {
        self.webhooks.get(name).map(|e| &e.info)
    }

    /// Check if a webhook exists.
    pub fn exists(&self, name: &str) -> bool {
        self.webhooks.contains_key(name)
    }

    /// Check if a path is registered.
    pub fn path_exists(&self, path: &str) -> bool {
        self.by_path.contains_key(path)
    }

    /// Get all webhook names.
    pub fn webhook_names(&self) -> impl Iterator<Item = &str> {
        self.webhooks.keys().map(|s| s.as_str())
    }

    /// Get all registered paths.
    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.by_path.keys().map(|s| s.as_str())
    }

    /// Get all webhooks.
    pub fn webhooks(&self) -> impl Iterator<Item = (&str, &Arc<WebhookEntry>)> {
        self.webhooks.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// Get the number of registered webhooks.
    pub fn len(&self) -> usize {
        self.webhooks.len()
    }

    /// Check if registry is empty.
    pub fn is_empty(&self) -> bool {
        self.webhooks.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_new() {
        let registry = WebhookRegistry::new();
        assert!(registry.is_empty());
    }
}
