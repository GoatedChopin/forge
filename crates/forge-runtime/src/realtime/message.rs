use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tokio::sync::{RwLock, mpsc};

use forge_core::cluster::NodeId;
use forge_core::realtime::{Delta, SessionId, SubscriptionId};

#[derive(Debug, Clone)]
pub struct RealtimeConfig {
    pub max_subscriptions_per_session: usize,
}

impl Default for RealtimeConfig {
    fn default() -> Self {
        Self {
            max_subscriptions_per_session: 50,
        }
    }
}

/// Job data sent to client (subset of internal JobRecord).
#[derive(Debug, Clone, Serialize)]
pub struct JobData {
    pub job_id: String,
    pub status: String,
    pub progress_percent: Option<i32>,
    pub progress_message: Option<String>,
    pub output: Option<serde_json::Value>,
    pub error: Option<String>,
}

/// Workflow data sent to client.
#[derive(Debug, Clone, Serialize)]
pub struct WorkflowData {
    pub workflow_id: String,
    pub status: String,
    pub current_step: Option<String>,
    pub steps: Vec<WorkflowStepData>,
    pub output: Option<serde_json::Value>,
    pub error: Option<String>,
}

/// Workflow step data sent to client.
#[derive(Debug, Clone, Serialize)]
pub struct WorkflowStepData {
    pub name: String,
    pub status: String,
    pub error: Option<String>,
}

/// Message types for real-time communication.
#[derive(Debug, Clone)]
pub enum RealtimeMessage {
    /// Subscribe to a query.
    Subscribe {
        id: String,
        query: String,
        args: serde_json::Value,
    },
    /// Unsubscribe from a subscription.
    Unsubscribe { subscription_id: SubscriptionId },
    /// Ping for keepalive.
    Ping,
    /// Pong response.
    Pong,
    /// Initial data for subscription.
    Data {
        subscription_id: String,
        data: serde_json::Value,
    },
    /// Delta update for subscription.
    DeltaUpdate {
        subscription_id: String,
        delta: Delta<serde_json::Value>,
    },
    /// Job progress update.
    JobUpdate { client_sub_id: String, job: JobData },
    /// Workflow progress update.
    WorkflowUpdate {
        client_sub_id: String,
        workflow: WorkflowData,
    },
    /// Error message.
    Error { code: String, message: String },
    /// Error message with subscription ID.
    ErrorWithId {
        id: String,
        code: String,
        message: String,
    },
    /// Authentication successful.
    AuthSuccess,
    /// Authentication failed.
    AuthFailed { reason: String },
}

#[derive(Debug)]
pub struct RealtimeSession {
    #[allow(dead_code)]
    pub session_id: SessionId,
    pub subscriptions: Vec<SubscriptionId>,
    pub sender: mpsc::Sender<RealtimeMessage>,
    #[allow(dead_code)]
    pub connected_at: chrono::DateTime<chrono::Utc>,
    pub last_active: chrono::DateTime<chrono::Utc>,
}

impl RealtimeSession {
    /// Create a new session.
    pub fn new(session_id: SessionId, sender: mpsc::Sender<RealtimeMessage>) -> Self {
        let now = chrono::Utc::now();
        Self {
            session_id,
            subscriptions: Vec::new(),
            sender,
            connected_at: now,
            last_active: now,
        }
    }

    /// Add a subscription.
    pub fn add_subscription(&mut self, subscription_id: SubscriptionId) {
        self.subscriptions.push(subscription_id);
        self.last_active = chrono::Utc::now();
    }

    /// Remove a subscription.
    pub fn remove_subscription(&mut self, subscription_id: SubscriptionId) {
        self.subscriptions.retain(|id| *id != subscription_id);
        self.last_active = chrono::Utc::now();
    }

    /// Send a message to the client.
    pub async fn send(
        &self,
        message: RealtimeMessage,
    ) -> Result<(), mpsc::error::SendError<RealtimeMessage>> {
        self.sender.send(message).await
    }
}

pub struct SessionServer {
    config: RealtimeConfig,
    node_id: NodeId,
    /// Active connections by session ID.
    connections: Arc<RwLock<HashMap<SessionId, RealtimeSession>>>,
    /// Subscription to session mapping for fast lookup.
    subscription_sessions: Arc<RwLock<HashMap<SubscriptionId, SessionId>>>,
}

impl SessionServer {
    /// Create a new session server.
    pub fn new(node_id: NodeId, config: RealtimeConfig) -> Self {
        Self {
            config,
            node_id,
            connections: Arc::new(RwLock::new(HashMap::new())),
            subscription_sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get the node ID.
    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    /// Get the configuration.
    pub fn config(&self) -> &RealtimeConfig {
        &self.config
    }

    /// Register a new connection.
    pub async fn register_connection(
        &self,
        session_id: SessionId,
        sender: mpsc::Sender<RealtimeMessage>,
    ) {
        let connection = RealtimeSession::new(session_id, sender);
        let mut connections = self.connections.write().await;
        connections.insert(session_id, connection);
    }

    /// Remove a connection.
    pub async fn remove_connection(&self, session_id: SessionId) -> Option<Vec<SubscriptionId>> {
        let mut connections = self.connections.write().await;
        if let Some(conn) = connections.remove(&session_id) {
            let mut sub_sessions = self.subscription_sessions.write().await;
            for sub_id in &conn.subscriptions {
                sub_sessions.remove(sub_id);
            }
            Some(conn.subscriptions)
        } else {
            None
        }
    }

    /// Add a subscription to a connection.
    pub async fn add_subscription(
        &self,
        session_id: SessionId,
        subscription_id: SubscriptionId,
    ) -> forge_core::Result<()> {
        let mut connections = self.connections.write().await;
        let conn = connections
            .get_mut(&session_id)
            .ok_or_else(|| forge_core::ForgeError::Validation("Session not found".to_string()))?;

        if conn.subscriptions.len() >= self.config.max_subscriptions_per_session {
            return Err(forge_core::ForgeError::Validation(format!(
                "Maximum subscriptions per session ({}) exceeded",
                self.config.max_subscriptions_per_session
            )));
        }

        conn.add_subscription(subscription_id);

        let mut sub_sessions = self.subscription_sessions.write().await;
        sub_sessions.insert(subscription_id, session_id);

        Ok(())
    }

    /// Remove a subscription from a connection.
    pub async fn remove_subscription(&self, subscription_id: SubscriptionId) {
        let session_id = {
            let mut sub_sessions = self.subscription_sessions.write().await;
            sub_sessions.remove(&subscription_id)
        };

        if let Some(session_id) = session_id {
            let mut connections = self.connections.write().await;
            if let Some(conn) = connections.get_mut(&session_id) {
                conn.remove_subscription(subscription_id);
            }
        }
    }

    /// Send a message to a specific session.
    pub async fn send_to_session(
        &self,
        session_id: SessionId,
        message: RealtimeMessage,
    ) -> forge_core::Result<()> {
        let connections = self.connections.read().await;
        let conn = connections
            .get(&session_id)
            .ok_or_else(|| forge_core::ForgeError::Validation("Session not found".to_string()))?;

        conn.send(message)
            .await
            .map_err(|_| forge_core::ForgeError::Internal("Failed to send message".to_string()))
    }

    /// Send a delta to all sessions subscribed to a subscription.
    pub async fn broadcast_delta(
        &self,
        subscription_id: SubscriptionId,
        delta: Delta<serde_json::Value>,
    ) -> forge_core::Result<()> {
        let session_id = {
            let sub_sessions = self.subscription_sessions.read().await;
            sub_sessions.get(&subscription_id).copied()
        };

        if let Some(session_id) = session_id {
            let message = RealtimeMessage::DeltaUpdate {
                subscription_id: subscription_id.to_string(),
                delta,
            };
            self.send_to_session(session_id, message).await?;
        }

        Ok(())
    }

    /// Get connection count.
    pub async fn connection_count(&self) -> usize {
        self.connections.read().await.len()
    }

    /// Get subscription count.
    pub async fn subscription_count(&self) -> usize {
        self.subscription_sessions.read().await.len()
    }

    /// Get server statistics.
    pub async fn stats(&self) -> SessionStats {
        let connections = self.connections.read().await;
        let total_subscriptions: usize = connections.values().map(|c| c.subscriptions.len()).sum();

        SessionStats {
            connections: connections.len(),
            subscriptions: total_subscriptions,
            node_id: self.node_id,
        }
    }

    /// Cleanup stale connections.
    pub async fn cleanup_stale(&self, max_idle: Duration) {
        let cutoff = chrono::Utc::now()
            - chrono::Duration::from_std(max_idle).unwrap_or(chrono::TimeDelta::MAX);
        let mut connections = self.connections.write().await;
        let mut sub_sessions = self.subscription_sessions.write().await;

        connections.retain(|_, conn| {
            if conn.last_active < cutoff {
                for sub_id in &conn.subscriptions {
                    sub_sessions.remove(sub_id);
                }
                false
            } else {
                true
            }
        });
    }
}

/// Session server statistics.
#[derive(Debug, Clone)]
pub struct SessionStats {
    /// Number of active connections.
    pub connections: usize,
    /// Total subscriptions across all connections.
    pub subscriptions: usize,
    /// Node ID.
    pub node_id: NodeId,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_realtime_config_default() {
        let config = RealtimeConfig::default();
        assert_eq!(config.max_subscriptions_per_session, 50);
    }

    #[tokio::test]
    async fn test_session_server_creation() {
        let node_id = NodeId::new();
        let server = SessionServer::new(node_id, RealtimeConfig::default());

        assert_eq!(server.node_id(), node_id);
        assert_eq!(server.connection_count().await, 0);
        assert_eq!(server.subscription_count().await, 0);
    }

    #[tokio::test]
    async fn test_session_connection() {
        let node_id = NodeId::new();
        let server = SessionServer::new(node_id, RealtimeConfig::default());
        let session_id = SessionId::new();
        let (tx, _rx) = mpsc::channel(100);

        server.register_connection(session_id, tx).await;
        assert_eq!(server.connection_count().await, 1);

        let removed = server.remove_connection(session_id).await;
        assert!(removed.is_some());
        assert_eq!(server.connection_count().await, 0);
    }

    #[tokio::test]
    async fn test_session_subscription() {
        let node_id = NodeId::new();
        let server = SessionServer::new(node_id, RealtimeConfig::default());
        let session_id = SessionId::new();
        let subscription_id = SubscriptionId::new();
        let (tx, _rx) = mpsc::channel(100);

        server.register_connection(session_id, tx).await;
        server
            .add_subscription(session_id, subscription_id)
            .await
            .unwrap();

        assert_eq!(server.subscription_count().await, 1);

        server.remove_subscription(subscription_id).await;
        assert_eq!(server.subscription_count().await, 0);
    }

    #[tokio::test]
    async fn test_session_subscription_limit() {
        let node_id = NodeId::new();
        let config = RealtimeConfig {
            max_subscriptions_per_session: 2,
        };
        let server = SessionServer::new(node_id, config);
        let session_id = SessionId::new();
        let (tx, _rx) = mpsc::channel(100);

        server.register_connection(session_id, tx).await;

        server
            .add_subscription(session_id, SubscriptionId::new())
            .await
            .unwrap();
        server
            .add_subscription(session_id, SubscriptionId::new())
            .await
            .unwrap();

        let result = server
            .add_subscription(session_id, SubscriptionId::new())
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_session_stats() {
        let node_id = NodeId::new();
        let server = SessionServer::new(node_id, RealtimeConfig::default());
        let session_id = SessionId::new();
        let (tx, _rx) = mpsc::channel(100);

        server.register_connection(session_id, tx).await;
        server
            .add_subscription(session_id, SubscriptionId::new())
            .await
            .unwrap();
        server
            .add_subscription(session_id, SubscriptionId::new())
            .await
            .unwrap();

        let stats = server.stats().await;
        assert_eq!(stats.connections, 1);
        assert_eq!(stats.subscriptions, 2);
        assert_eq!(stats.node_id, node_id);
    }
}
