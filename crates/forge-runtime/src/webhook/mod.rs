//! Webhook runtime components.

mod handler;
mod registry;

pub use handler::{WebhookState, webhook_handler};
pub use registry::{BoxedWebhookHandler, WebhookEntry, WebhookRegistry};
