use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use tokio::sync::{RwLock, broadcast, mpsc};
use uuid::Uuid;

use forge_core::cluster::NodeId;
use forge_core::realtime::{Change, ReadSet, SessionId, SubscriptionId};

use super::invalidation::{InvalidationConfig, InvalidationEngine};
use super::listener::{ChangeListener, ListenerConfig};
use super::manager::SubscriptionManager;
use super::message::{
    JobData, RealtimeConfig, RealtimeMessage, SessionServer, WorkflowData, WorkflowStepData,
};
use crate::function::{FunctionEntry, FunctionRegistry};

#[derive(Debug, Clone)]
pub struct ReactorConfig {
    pub listener: ListenerConfig,
    pub invalidation: InvalidationConfig,
    pub realtime: RealtimeConfig,
    pub max_listener_restarts: u32,
    /// Doubles with each attempt for exponential backoff
    pub listener_restart_delay_ms: u64,
}

impl Default for ReactorConfig {
    fn default() -> Self {
        Self {
            listener: ListenerConfig::default(),
            invalidation: InvalidationConfig::default(),
            realtime: RealtimeConfig::default(),
            max_listener_restarts: 5,
            listener_restart_delay_ms: 1000,
        }
    }
}

/// Active subscription with execution context.
#[derive(Debug, Clone)]
pub struct ActiveSubscription {
    #[allow(dead_code)]
    pub subscription_id: SubscriptionId,
    pub session_id: SessionId,
    #[allow(dead_code)]
    pub client_sub_id: String,
    pub query_name: String,
    pub args: serde_json::Value,
    pub last_result_hash: Option<String>,
    #[allow(dead_code)]
    pub read_set: ReadSet,
    /// Auth context for re-executing the query on invalidation.
    pub auth_context: forge_core::function::AuthContext,
}

/// Job subscription tracking.
#[derive(Debug, Clone)]
pub struct JobSubscription {
    #[allow(dead_code)]
    pub subscription_id: SubscriptionId,
    pub session_id: SessionId,
    pub client_sub_id: String,
    #[allow(dead_code)]
    pub job_id: Uuid, // Validated UUID, not String
    pub auth_context: forge_core::function::AuthContext,
}

/// Workflow subscription tracking.
#[derive(Debug, Clone)]
pub struct WorkflowSubscription {
    #[allow(dead_code)]
    pub subscription_id: SubscriptionId,
    pub session_id: SessionId,
    pub client_sub_id: String,
    #[allow(dead_code)]
    pub workflow_id: Uuid, // Validated UUID, not String
    pub auth_context: forge_core::function::AuthContext,
}

/// ChangeListener -> InvalidationEngine -> Query Re-execution -> SSE Push
pub struct Reactor {
    node_id: NodeId,
    db_pool: sqlx::PgPool,
    registry: FunctionRegistry,
    subscription_manager: Arc<SubscriptionManager>,
    session_server: Arc<SessionServer>,
    change_listener: Arc<ChangeListener>,
    invalidation_engine: Arc<InvalidationEngine>,
    /// Active subscriptions with their execution context.
    active_subscriptions: Arc<RwLock<HashMap<SubscriptionId, ActiveSubscription>>>,
    /// Job subscriptions: job_id -> list of subscribers.
    job_subscriptions: Arc<RwLock<HashMap<Uuid, Vec<JobSubscription>>>>,
    /// Workflow subscriptions: workflow_id -> list of subscribers.
    workflow_subscriptions: Arc<RwLock<HashMap<Uuid, Vec<WorkflowSubscription>>>>,
    /// Shutdown signal.
    shutdown_tx: broadcast::Sender<()>,
    /// Listener restart configuration
    max_listener_restarts: u32,
    listener_restart_delay_ms: u64,
}

impl Reactor {
    /// Create a new reactor.
    pub fn new(
        node_id: NodeId,
        db_pool: sqlx::PgPool,
        registry: FunctionRegistry,
        config: ReactorConfig,
    ) -> Self {
        let subscription_manager = Arc::new(SubscriptionManager::new(
            config.realtime.max_subscriptions_per_session,
        ));
        let session_server = Arc::new(SessionServer::new(node_id, config.realtime.clone()));
        let change_listener = Arc::new(ChangeListener::new(db_pool.clone(), config.listener));
        let invalidation_engine = Arc::new(InvalidationEngine::new(
            subscription_manager.clone(),
            config.invalidation,
        ));
        let (shutdown_tx, _) = broadcast::channel(1);

        Self {
            node_id,
            db_pool,
            registry,
            subscription_manager,
            session_server,
            change_listener,
            invalidation_engine,
            active_subscriptions: Arc::new(RwLock::new(HashMap::new())),
            job_subscriptions: Arc::new(RwLock::new(HashMap::new())),
            workflow_subscriptions: Arc::new(RwLock::new(HashMap::new())),
            shutdown_tx,
            max_listener_restarts: config.max_listener_restarts,
            listener_restart_delay_ms: config.listener_restart_delay_ms,
        }
    }

    /// Get the node ID.
    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    /// Get the session server reference.
    pub fn session_server(&self) -> Arc<SessionServer> {
        self.session_server.clone()
    }

    /// Get the subscription manager reference.
    pub fn subscription_manager(&self) -> Arc<SubscriptionManager> {
        self.subscription_manager.clone()
    }

    /// Get a shutdown receiver.
    pub fn shutdown_receiver(&self) -> broadcast::Receiver<()> {
        self.shutdown_tx.subscribe()
    }

    /// Register a new session.
    pub async fn register_session(
        &self,
        session_id: SessionId,
        sender: mpsc::Sender<RealtimeMessage>,
    ) {
        self.session_server
            .register_connection(session_id, sender)
            .await;
        tracing::trace!(?session_id, "Session registered");
    }

    /// Remove a session and all its subscriptions.
    pub async fn remove_session(&self, session_id: SessionId) {
        if let Some(subscription_ids) = self.session_server.remove_connection(session_id).await {
            // Clean up query subscriptions
            for sub_id in subscription_ids {
                self.subscription_manager.remove_subscription(sub_id).await;
                self.active_subscriptions.write().await.remove(&sub_id);
            }
        }

        // Clean up job subscriptions for this session
        {
            let mut job_subs = self.job_subscriptions.write().await;
            for subscribers in job_subs.values_mut() {
                subscribers.retain(|s| s.session_id != session_id);
            }
            // Remove empty entries
            job_subs.retain(|_, v| !v.is_empty());
        }

        // Clean up workflow subscriptions for this session
        {
            let mut workflow_subs = self.workflow_subscriptions.write().await;
            for subscribers in workflow_subs.values_mut() {
                subscribers.retain(|s| s.session_id != session_id);
            }
            // Remove empty entries
            workflow_subs.retain(|_, v| !v.is_empty());
        }

        tracing::trace!(?session_id, "Session removed");
    }

    /// Subscribe to a query.
    pub async fn subscribe(
        &self,
        session_id: SessionId,
        client_sub_id: String,
        query_name: String,
        args: serde_json::Value,
        auth_context: forge_core::function::AuthContext,
    ) -> forge_core::Result<(SubscriptionId, serde_json::Value)> {
        let sub_info = self
            .subscription_manager
            .create_subscription(session_id, &query_name, args.clone())
            .await?;

        let subscription_id = sub_info.id;

        if let Err(error) = self
            .session_server
            .add_subscription(session_id, subscription_id)
            .await
        {
            self.subscription_manager
                .remove_subscription(subscription_id)
                .await;
            return Err(error);
        }

        let (data, read_set) = match self.execute_query(&query_name, &args, &auth_context).await {
            Ok(result) => result,
            Err(error) => {
                // Roll back optimistic subscription registration on auth/query failures.
                self.unsubscribe(subscription_id).await;
                return Err(error);
            }
        };

        let result_hash = Self::compute_hash(&data);

        tracing::trace!(
            ?subscription_id,
            query = %query_name,
            tables = ?read_set.tables.iter().collect::<Vec<_>>(),
            "Subscription read set"
        );

        self.subscription_manager
            .update_subscription(subscription_id, read_set.clone(), result_hash.clone())
            .await;

        let active = ActiveSubscription {
            subscription_id,
            session_id,
            client_sub_id,
            query_name,
            args,
            last_result_hash: Some(result_hash),
            read_set,
            auth_context,
        };
        self.active_subscriptions
            .write()
            .await
            .insert(subscription_id, active);

        tracing::trace!(?subscription_id, "Subscription created");

        Ok((subscription_id, data))
    }

    /// Unsubscribe from a query.
    pub async fn unsubscribe(&self, subscription_id: SubscriptionId) {
        self.session_server
            .remove_subscription(subscription_id)
            .await;
        self.subscription_manager
            .remove_subscription(subscription_id)
            .await;
        self.active_subscriptions
            .write()
            .await
            .remove(&subscription_id);
        tracing::trace!(?subscription_id, "Subscription removed");
    }

    /// Subscribe to job progress updates.
    pub async fn subscribe_job(
        &self,
        session_id: SessionId,
        client_sub_id: String,
        job_id: Uuid, // Pre-validated UUID
        auth_context: &forge_core::function::AuthContext,
    ) -> forge_core::Result<JobData> {
        let subscription_id = SubscriptionId::new();

        Self::ensure_job_access(&self.db_pool, job_id, auth_context).await?;

        // Fetch current job state from database
        let job_data = self.fetch_job_data(job_id).await?;

        // Register subscription
        let subscription = JobSubscription {
            subscription_id,
            session_id,
            client_sub_id: client_sub_id.clone(),
            job_id,
            auth_context: auth_context.clone(),
        };

        let mut subs = self.job_subscriptions.write().await;
        subs.entry(job_id).or_default().push(subscription);

        tracing::trace!(
            ?subscription_id,
            %job_id,
            "Job subscription created"
        );

        Ok(job_data)
    }

    /// Unsubscribe from job updates.
    pub async fn unsubscribe_job(&self, session_id: SessionId, client_sub_id: &str) {
        let mut subs = self.job_subscriptions.write().await;

        // Find and remove the subscription
        for subscribers in subs.values_mut() {
            subscribers
                .retain(|s| !(s.session_id == session_id && s.client_sub_id == client_sub_id));
        }

        // Remove empty entries
        subs.retain(|_, v| !v.is_empty());

        tracing::trace!(client_id = %client_sub_id, "Job subscription removed");
    }

    /// Subscribe to workflow progress updates.
    pub async fn subscribe_workflow(
        &self,
        session_id: SessionId,
        client_sub_id: String,
        workflow_id: Uuid, // Pre-validated UUID
        auth_context: &forge_core::function::AuthContext,
    ) -> forge_core::Result<WorkflowData> {
        let subscription_id = SubscriptionId::new();

        Self::ensure_workflow_access(&self.db_pool, workflow_id, auth_context).await?;

        // Fetch current workflow + steps from database
        let workflow_data = self.fetch_workflow_data(workflow_id).await?;

        // Register subscription
        let subscription = WorkflowSubscription {
            subscription_id,
            session_id,
            client_sub_id: client_sub_id.clone(),
            workflow_id,
            auth_context: auth_context.clone(),
        };

        let mut subs = self.workflow_subscriptions.write().await;
        subs.entry(workflow_id).or_default().push(subscription);

        tracing::trace!(
            ?subscription_id,
            %workflow_id,
            "Workflow subscription created"
        );

        Ok(workflow_data)
    }

    /// Unsubscribe from workflow updates.
    pub async fn unsubscribe_workflow(&self, session_id: SessionId, client_sub_id: &str) {
        let mut subs = self.workflow_subscriptions.write().await;

        // Find and remove the subscription
        for subscribers in subs.values_mut() {
            subscribers
                .retain(|s| !(s.session_id == session_id && s.client_sub_id == client_sub_id));
        }

        // Remove empty entries
        subs.retain(|_, v| !v.is_empty());

        tracing::trace!(client_id = %client_sub_id, "Workflow subscription removed");
    }

    /// Fetch current job data from database.
    #[allow(clippy::type_complexity)]
    async fn fetch_job_data(&self, job_id: Uuid) -> forge_core::Result<JobData> {
        let row: Option<(
            String,
            Option<i32>,
            Option<String>,
            Option<serde_json::Value>,
            Option<String>,
        )> = sqlx::query_as(
            r#"
                SELECT status, progress_percent, progress_message, output,
                       COALESCE(cancel_reason, last_error) as error
                FROM forge_jobs WHERE id = $1
                "#,
        )
        .bind(job_id)
        .fetch_optional(&self.db_pool)
        .await
        .map_err(forge_core::ForgeError::Sql)?;

        match row {
            Some((status, progress_percent, progress_message, output, error)) => Ok(JobData {
                job_id: job_id.to_string(),
                status,
                progress_percent,
                progress_message,
                output,
                error,
            }),
            None => Err(forge_core::ForgeError::NotFound(format!(
                "Job {} not found",
                job_id
            ))),
        }
    }

    /// Fetch current workflow + steps from database.
    #[allow(clippy::type_complexity)]
    async fn fetch_workflow_data(&self, workflow_id: Uuid) -> forge_core::Result<WorkflowData> {
        // Fetch workflow run
        let row: Option<(
            String,
            Option<String>,
            Option<serde_json::Value>,
            Option<String>,
        )> = sqlx::query_as(
            r#"
                SELECT status, current_step, output, error
                FROM forge_workflow_runs WHERE id = $1
                "#,
        )
        .bind(workflow_id)
        .fetch_optional(&self.db_pool)
        .await
        .map_err(forge_core::ForgeError::Sql)?;

        let (status, current_step, output, error) = match row {
            Some(r) => r,
            None => {
                return Err(forge_core::ForgeError::NotFound(format!(
                    "Workflow {} not found",
                    workflow_id
                )));
            }
        };

        // Fetch workflow steps
        let step_rows: Vec<(String, String, Option<String>)> = sqlx::query_as(
            r#"
            SELECT step_name, status, error
            FROM forge_workflow_steps
            WHERE workflow_run_id = $1
            ORDER BY started_at ASC NULLS LAST
            "#,
        )
        .bind(workflow_id)
        .fetch_all(&self.db_pool)
        .await
        .map_err(forge_core::ForgeError::Sql)?;

        let steps = step_rows
            .into_iter()
            .map(|(name, status, error)| WorkflowStepData {
                name,
                status,
                error,
            })
            .collect();

        Ok(WorkflowData {
            workflow_id: workflow_id.to_string(),
            status,
            current_step,
            steps,
            output,
            error,
        })
    }

    /// Execute a query and return data with read set.
    async fn execute_query(
        &self,
        query_name: &str,
        args: &serde_json::Value,
        auth_context: &forge_core::function::AuthContext,
    ) -> forge_core::Result<(serde_json::Value, ReadSet)> {
        match self.registry.get(query_name) {
            Some(FunctionEntry::Query { info, handler }) => {
                Self::check_query_auth(info, auth_context)?;
                Self::check_identity_args(query_name, args, auth_context, !info.is_public)?;

                let ctx = forge_core::function::QueryContext::new(
                    self.db_pool.clone(),
                    auth_context.clone(),
                    forge_core::function::RequestMetadata::new(),
                );

                // Normalize args
                let normalized_args = match args {
                    v if v.is_object() && v.as_object().unwrap().is_empty() => {
                        serde_json::Value::Null
                    }
                    v => v.clone(),
                };

                let data = handler(&ctx, normalized_args).await?;

                // Create read set from compile-time extracted table dependencies
                let mut read_set = ReadSet::new();

                if info.table_dependencies.is_empty() {
                    // Fallback: no tables extracted (dynamic SQL)
                    // Use naming convention as last resort
                    let table_name = Self::extract_table_name(query_name);
                    read_set.add_table(&table_name);
                    tracing::trace!(
                        query = %query_name,
                        fallback_table = %table_name,
                        "Using naming convention fallback for table dependency"
                    );
                } else {
                    // Use compile-time extracted tables
                    for table in info.table_dependencies {
                        read_set.add_table(*table);
                    }
                }

                Ok((data, read_set))
            }
            Some(_) => Err(forge_core::ForgeError::Validation(format!(
                "'{}' is not a query",
                query_name
            ))),
            None => Err(forge_core::ForgeError::Validation(format!(
                "Query '{}' not found",
                query_name
            ))),
        }
    }

    /// Compute a hash of the result for delta detection.
    fn compute_hash(data: &serde_json::Value) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let json = serde_json::to_string(data).unwrap_or_default();
        let mut hasher = DefaultHasher::new();
        json.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }

    /// Start the reactor (runs the change listener and invalidation loop).
    pub async fn start(&self) -> forge_core::Result<()> {
        let listener = self.change_listener.clone();
        let invalidation_engine = self.invalidation_engine.clone();
        let active_subscriptions = self.active_subscriptions.clone();
        let job_subscriptions = self.job_subscriptions.clone();
        let workflow_subscriptions = self.workflow_subscriptions.clone();
        let session_server = self.session_server.clone();
        let registry = self.registry.clone();
        let db_pool = self.db_pool.clone();
        let mut shutdown_rx = self.shutdown_tx.subscribe();
        let max_restarts = self.max_listener_restarts;
        let base_delay_ms = self.listener_restart_delay_ms;

        // Subscribe to changes
        let mut change_rx = listener.subscribe();

        // Main reactor loop
        tokio::spawn(async move {
            tracing::debug!("Reactor listening for changes");

            let mut restart_count: u32 = 0;
            let (listener_error_tx, mut listener_error_rx) = mpsc::channel::<String>(1);

            // Start initial listener
            let listener_clone = listener.clone();
            let error_tx = listener_error_tx.clone();
            let mut listener_handle = Some(tokio::spawn(async move {
                if let Err(e) = listener_clone.run().await {
                    let _ = error_tx.send(format!("Change listener error: {}", e)).await;
                }
            }));

            loop {
                tokio::select! {
                    result = change_rx.recv() => {
                        match result {
                            Ok(change) => {
                                Self::handle_change(
                                    &change,
                                    &invalidation_engine,
                                    &active_subscriptions,
                                    &job_subscriptions,
                                    &workflow_subscriptions,
                                    &session_server,
                                    &registry,
                                    &db_pool,
                                ).await;
                            }
                            Err(broadcast::error::RecvError::Lagged(n)) => {
                                tracing::warn!("Reactor lagged by {} messages", n);
                            }
                            Err(broadcast::error::RecvError::Closed) => {
                                tracing::debug!("Change channel closed");
                                break;
                            }
                        }
                    }
                    Some(error_msg) = listener_error_rx.recv() => {
                        if restart_count >= max_restarts {
                            tracing::error!(
                                attempts = restart_count,
                                last_error = %error_msg,
                                "Change listener failed permanently, real-time updates disabled"
                            );
                            break;
                        }

                        restart_count += 1;
                        let delay = base_delay_ms * 2u64.saturating_pow(restart_count - 1);
                        tracing::warn!(
                            attempt = restart_count,
                            max = max_restarts,
                            delay_ms = delay,
                            error = %error_msg,
                            "Change listener restarting"
                        );

                        tokio::time::sleep(std::time::Duration::from_millis(delay)).await;

                        // Restart listener
                        let listener_clone = listener.clone();
                        let error_tx = listener_error_tx.clone();
                        if let Some(handle) = listener_handle.take() {
                            handle.abort();
                        }
                        change_rx = listener.subscribe();
                        listener_handle = Some(tokio::spawn(async move {
                            if let Err(e) = listener_clone.run().await {
                                let _ = error_tx.send(format!("Change listener error: {}", e)).await;
                            }
                        }));
                    }
                    _ = shutdown_rx.recv() => {
                        tracing::debug!("Reactor shutting down");
                        break;
                    }
                }
            }

            if let Some(handle) = listener_handle {
                handle.abort();
            }
        });

        Ok(())
    }

    /// Handle a database change event.
    #[allow(clippy::too_many_arguments)]
    async fn handle_change(
        change: &Change,
        invalidation_engine: &Arc<InvalidationEngine>,
        active_subscriptions: &Arc<RwLock<HashMap<SubscriptionId, ActiveSubscription>>>,
        job_subscriptions: &Arc<RwLock<HashMap<Uuid, Vec<JobSubscription>>>>,
        workflow_subscriptions: &Arc<RwLock<HashMap<Uuid, Vec<WorkflowSubscription>>>>,
        session_server: &Arc<SessionServer>,
        registry: &FunctionRegistry,
        db_pool: &sqlx::PgPool,
    ) {
        tracing::trace!(table = %change.table, op = ?change.operation, row_id = ?change.row_id, "Processing change");

        // Handle job/workflow table changes first
        match change.table.as_str() {
            "forge_jobs" => {
                if let Some(job_id) = change.row_id {
                    Self::handle_job_change(job_id, job_subscriptions, session_server, db_pool)
                        .await;
                }
                return; // Don't process through query invalidation
            }
            "forge_workflow_runs" => {
                if let Some(workflow_id) = change.row_id {
                    Self::handle_workflow_change(
                        workflow_id,
                        workflow_subscriptions,
                        session_server,
                        db_pool,
                    )
                    .await;
                }
                return; // Don't process through query invalidation
            }
            "forge_workflow_steps" => {
                // For step changes, need to look up the parent workflow_id
                if let Some(step_id) = change.row_id {
                    Self::handle_workflow_step_change(
                        step_id,
                        workflow_subscriptions,
                        session_server,
                        db_pool,
                    )
                    .await;
                }
                return; // Don't process through query invalidation
            }
            _ => {}
        }

        // Process change through invalidation engine for query subscriptions
        invalidation_engine.process_change(change.clone()).await;

        // Check for subscriptions ready to invalidate based on debounce windows:
        // - 50ms quiet period after last change
        // - 200ms max wait from first change
        // This prevents flooding during high-frequency updates (bulk inserts, rapid edits)
        let invalidated = invalidation_engine.check_pending().await;

        if invalidated.is_empty() {
            return;
        }

        tracing::trace!(count = invalidated.len(), "Invalidating subscriptions");

        // Collect subscription info under read lock, then release before async operations
        let subs_to_process: Vec<_> = {
            let subscriptions = active_subscriptions.read().await;
            invalidated
                .iter()
                .filter_map(|sub_id| {
                    subscriptions.get(sub_id).map(|active| {
                        (
                            *sub_id,
                            active.session_id,
                            active.client_sub_id.clone(),
                            active.query_name.clone(),
                            active.args.clone(),
                            active.last_result_hash.clone(),
                            active.auth_context.clone(),
                        )
                    })
                })
                .collect()
        };

        // Track updates to apply after processing
        let mut updates: Vec<(SubscriptionId, String)> = Vec::new();

        // Re-execute invalidated queries and push updates (without holding locks)
        for (sub_id, session_id, client_sub_id, query_name, args, last_hash, auth_context) in
            subs_to_process
        {
            // Re-execute the query
            match Self::execute_query_static(registry, db_pool, &query_name, &args, &auth_context)
                .await
            {
                Ok((new_data, _read_set)) => {
                    let new_hash = Self::compute_hash(&new_data);

                    // Only push if data changed
                    if last_hash.as_ref() != Some(&new_hash) {
                        // Send updated data to client using client_sub_id for SSE target matching
                        let message = RealtimeMessage::Data {
                            subscription_id: client_sub_id.clone(),
                            data: new_data,
                        };

                        if let Err(e) = session_server.send_to_session(session_id, message).await {
                            tracing::debug!(client_id = %client_sub_id, error = %e, "Failed to send update");
                        } else {
                            tracing::trace!(client_id = %client_sub_id, "Pushed update to client");
                            // Track the hash update
                            updates.push((sub_id, new_hash));
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(client_id = %client_sub_id, error = %e, "Failed to re-execute query");
                }
            }
        }

        // Update hashes for successfully sent updates
        if !updates.is_empty() {
            let mut subscriptions = active_subscriptions.write().await;
            for (sub_id, new_hash) in updates {
                if let Some(active) = subscriptions.get_mut(&sub_id) {
                    active.last_result_hash = Some(new_hash);
                }
            }
        }
    }

    /// Handle a job table change event.
    async fn handle_job_change(
        job_id: Uuid,
        job_subscriptions: &Arc<RwLock<HashMap<Uuid, Vec<JobSubscription>>>>,
        session_server: &Arc<SessionServer>,
        db_pool: &sqlx::PgPool,
    ) {
        let subs = job_subscriptions.read().await;
        let subscribers = match subs.get(&job_id) {
            Some(s) if !s.is_empty() => s.clone(),
            _ => return, // No subscribers for this job
        };
        drop(subs); // Release lock before async operations

        // Fetch latest job state
        let job_data = match Self::fetch_job_data_static(job_id, db_pool).await {
            Ok(data) => data,
            Err(e) => {
                tracing::debug!(%job_id, error = %e, "Failed to fetch job data");
                return;
            }
        };

        let owner_subject = match Self::fetch_job_owner_subject_static(job_id, db_pool).await {
            Ok(owner) => owner,
            Err(e) => {
                tracing::debug!(%job_id, error = %e, "Failed to fetch job owner");
                return;
            }
        };

        let mut unauthorized_subscribers: HashSet<(SessionId, String)> = HashSet::new();

        // Push to all subscribers
        for sub in subscribers {
            if Self::check_owner_access(owner_subject.clone(), &sub.auth_context).is_err() {
                unauthorized_subscribers.insert((sub.session_id, sub.client_sub_id.clone()));
                continue;
            }

            let message = RealtimeMessage::JobUpdate {
                client_sub_id: sub.client_sub_id.clone(),
                job: job_data.clone(),
            };

            if let Err(e) = session_server
                .send_to_session(sub.session_id, message)
                .await
            {
                tracing::trace!(%job_id, error = %e, "Failed to send job update");
            } else {
                tracing::trace!(%job_id, "Job update sent");
            }
        }

        if !unauthorized_subscribers.is_empty() {
            let mut subs = job_subscriptions.write().await;
            if let Some(entries) = subs.get_mut(&job_id) {
                entries.retain(|entry| {
                    !unauthorized_subscribers
                        .contains(&(entry.session_id, entry.client_sub_id.clone()))
                });
            }
            subs.retain(|_, v| !v.is_empty());
        }
    }

    /// Handle a workflow table change event.
    async fn handle_workflow_change(
        workflow_id: Uuid,
        workflow_subscriptions: &Arc<RwLock<HashMap<Uuid, Vec<WorkflowSubscription>>>>,
        session_server: &Arc<SessionServer>,
        db_pool: &sqlx::PgPool,
    ) {
        let subs = workflow_subscriptions.read().await;
        let subscribers = match subs.get(&workflow_id) {
            Some(s) if !s.is_empty() => s.clone(),
            _ => return, // No subscribers for this workflow
        };
        drop(subs); // Release lock before async operations

        // Fetch latest workflow + steps state
        let workflow_data = match Self::fetch_workflow_data_static(workflow_id, db_pool).await {
            Ok(data) => data,
            Err(e) => {
                tracing::debug!(%workflow_id, error = %e, "Failed to fetch workflow data");
                return;
            }
        };

        let owner_subject =
            match Self::fetch_workflow_owner_subject_static(workflow_id, db_pool).await {
                Ok(owner) => owner,
                Err(e) => {
                    tracing::debug!(%workflow_id, error = %e, "Failed to fetch workflow owner");
                    return;
                }
            };

        let mut unauthorized_subscribers: HashSet<(SessionId, String)> = HashSet::new();

        // Push to all subscribers
        for sub in subscribers {
            if Self::check_owner_access(owner_subject.clone(), &sub.auth_context).is_err() {
                unauthorized_subscribers.insert((sub.session_id, sub.client_sub_id.clone()));
                continue;
            }

            let message = RealtimeMessage::WorkflowUpdate {
                client_sub_id: sub.client_sub_id.clone(),
                workflow: workflow_data.clone(),
            };

            if let Err(e) = session_server
                .send_to_session(sub.session_id, message)
                .await
            {
                tracing::trace!(%workflow_id, error = %e, "Failed to send workflow update");
            } else {
                tracing::trace!(%workflow_id, "Workflow update sent");
            }
        }

        if !unauthorized_subscribers.is_empty() {
            let mut subs = workflow_subscriptions.write().await;
            if let Some(entries) = subs.get_mut(&workflow_id) {
                entries.retain(|entry| {
                    !unauthorized_subscribers
                        .contains(&(entry.session_id, entry.client_sub_id.clone()))
                });
            }
            subs.retain(|_, v| !v.is_empty());
        }
    }

    /// Handle a workflow step change event.
    async fn handle_workflow_step_change(
        step_id: Uuid,
        workflow_subscriptions: &Arc<RwLock<HashMap<Uuid, Vec<WorkflowSubscription>>>>,
        session_server: &Arc<SessionServer>,
        db_pool: &sqlx::PgPool,
    ) {
        // Look up the workflow_run_id for this step
        let workflow_id: Option<Uuid> = match sqlx::query_scalar(
            "SELECT workflow_run_id FROM forge_workflow_steps WHERE id = $1",
        )
        .bind(step_id)
        .fetch_optional(db_pool)
        .await
        {
            Ok(id) => id,
            Err(e) => {
                tracing::debug!(%step_id, error = %e, "Failed to look up workflow for step");
                return;
            }
        };

        if let Some(wf_id) = workflow_id {
            // Delegate to workflow change handler
            Self::handle_workflow_change(wf_id, workflow_subscriptions, session_server, db_pool)
                .await;
        }
    }

    /// Static version of fetch_job_data for use in handle_change.
    #[allow(clippy::type_complexity)]
    async fn fetch_job_data_static(
        job_id: Uuid,
        db_pool: &sqlx::PgPool,
    ) -> forge_core::Result<JobData> {
        let row: Option<(
            String,
            Option<i32>,
            Option<String>,
            Option<serde_json::Value>,
            Option<String>,
        )> = sqlx::query_as(
            r#"
                SELECT status, progress_percent, progress_message, output, last_error
                FROM forge_jobs WHERE id = $1
                "#,
        )
        .bind(job_id)
        .fetch_optional(db_pool)
        .await
        .map_err(forge_core::ForgeError::Sql)?;

        match row {
            Some((status, progress_percent, progress_message, output, error)) => Ok(JobData {
                job_id: job_id.to_string(),
                status,
                progress_percent,
                progress_message,
                output,
                error,
            }),
            None => Err(forge_core::ForgeError::NotFound(format!(
                "Job {} not found",
                job_id
            ))),
        }
    }

    async fn fetch_job_owner_subject_static(
        job_id: Uuid,
        db_pool: &sqlx::PgPool,
    ) -> forge_core::Result<Option<String>> {
        let owner_subject: Option<Option<String>> =
            sqlx::query_scalar("SELECT owner_subject FROM forge_jobs WHERE id = $1")
                .bind(job_id)
                .fetch_optional(db_pool)
                .await
                .map_err(forge_core::ForgeError::Sql)?;

        owner_subject
            .ok_or_else(|| forge_core::ForgeError::NotFound(format!("Job {} not found", job_id)))
    }

    /// Static version of fetch_workflow_data for use in handle_change.
    #[allow(clippy::type_complexity)]
    async fn fetch_workflow_data_static(
        workflow_id: Uuid,
        db_pool: &sqlx::PgPool,
    ) -> forge_core::Result<WorkflowData> {
        let row: Option<(
            String,
            Option<String>,
            Option<serde_json::Value>,
            Option<String>,
        )> = sqlx::query_as(
            r#"
                SELECT status, current_step, output, error
                FROM forge_workflow_runs WHERE id = $1
                "#,
        )
        .bind(workflow_id)
        .fetch_optional(db_pool)
        .await
        .map_err(forge_core::ForgeError::Sql)?;

        let (status, current_step, output, error) = match row {
            Some(r) => r,
            None => {
                return Err(forge_core::ForgeError::NotFound(format!(
                    "Workflow {} not found",
                    workflow_id
                )));
            }
        };

        let step_rows: Vec<(String, String, Option<String>)> = sqlx::query_as(
            r#"
            SELECT step_name, status, error
            FROM forge_workflow_steps
            WHERE workflow_run_id = $1
            ORDER BY started_at ASC NULLS LAST
            "#,
        )
        .bind(workflow_id)
        .fetch_all(db_pool)
        .await
        .map_err(forge_core::ForgeError::Sql)?;

        let steps = step_rows
            .into_iter()
            .map(|(name, status, error)| WorkflowStepData {
                name,
                status,
                error,
            })
            .collect();

        Ok(WorkflowData {
            workflow_id: workflow_id.to_string(),
            status,
            current_step,
            steps,
            output,
            error,
        })
    }

    async fn fetch_workflow_owner_subject_static(
        workflow_id: Uuid,
        db_pool: &sqlx::PgPool,
    ) -> forge_core::Result<Option<String>> {
        let owner_subject: Option<Option<String>> =
            sqlx::query_scalar("SELECT owner_subject FROM forge_workflow_runs WHERE id = $1")
                .bind(workflow_id)
                .fetch_optional(db_pool)
                .await
                .map_err(forge_core::ForgeError::Sql)?;

        owner_subject.ok_or_else(|| {
            forge_core::ForgeError::NotFound(format!("Workflow {} not found", workflow_id))
        })
    }

    /// Static version of execute_query for use in async context.
    async fn execute_query_static(
        registry: &FunctionRegistry,
        db_pool: &sqlx::PgPool,
        query_name: &str,
        args: &serde_json::Value,
        auth_context: &forge_core::function::AuthContext,
    ) -> forge_core::Result<(serde_json::Value, ReadSet)> {
        match registry.get(query_name) {
            Some(FunctionEntry::Query { info, handler }) => {
                Self::check_query_auth(info, auth_context)?;
                Self::check_identity_args(query_name, args, auth_context, !info.is_public)?;

                let ctx = forge_core::function::QueryContext::new(
                    db_pool.clone(),
                    auth_context.clone(),
                    forge_core::function::RequestMetadata::new(),
                );

                let normalized_args = match args {
                    v if v.is_object() && v.as_object().unwrap().is_empty() => {
                        serde_json::Value::Null
                    }
                    v => v.clone(),
                };

                let data = handler(&ctx, normalized_args).await?;

                // Create read set from compile-time extracted table dependencies
                let mut read_set = ReadSet::new();

                if info.table_dependencies.is_empty() {
                    // Fallback for dynamic SQL
                    let table_name = Self::extract_table_name(query_name);
                    read_set.add_table(&table_name);
                    tracing::trace!(
                        query = %query_name,
                        fallback_table = %table_name,
                        "Using naming convention fallback for table dependency"
                    );
                } else {
                    for table in info.table_dependencies {
                        read_set.add_table(*table);
                    }
                }

                Ok((data, read_set))
            }
            _ => Err(forge_core::ForgeError::Validation(format!(
                "Query '{}' not found or not a query",
                query_name
            ))),
        }
    }

    /// Extract table name from query name using common patterns.
    fn extract_table_name(query_name: &str) -> String {
        if let Some(rest) = query_name.strip_prefix("get_") {
            rest.to_string()
        } else if let Some(rest) = query_name.strip_prefix("list_") {
            rest.to_string()
        } else if let Some(rest) = query_name.strip_prefix("find_") {
            rest.to_string()
        } else if let Some(rest) = query_name.strip_prefix("fetch_") {
            rest.to_string()
        } else {
            query_name.to_string()
        }
    }

    fn check_query_auth(
        info: &forge_core::function::FunctionInfo,
        auth: &forge_core::function::AuthContext,
    ) -> forge_core::Result<()> {
        if info.is_public {
            return Ok(());
        }

        if !auth.is_authenticated() {
            return Err(forge_core::ForgeError::Unauthorized(
                "Authentication required".into(),
            ));
        }

        if let Some(role) = info.required_role {
            if !auth.has_role(role) {
                return Err(forge_core::ForgeError::Forbidden(format!(
                    "Role '{}' required",
                    role
                )));
            }
        }

        Ok(())
    }

    fn check_identity_args(
        function_name: &str,
        args: &serde_json::Value,
        auth: &forge_core::function::AuthContext,
        enforce_scope: bool,
    ) -> forge_core::Result<()> {
        if auth.is_admin() {
            return Ok(());
        }

        let Some(obj) = args.as_object() else {
            if enforce_scope && auth.is_authenticated() {
                return Err(forge_core::ForgeError::Forbidden(format!(
                    "Function '{function_name}' must include identity or tenant scope arguments"
                )));
            }
            return Ok(());
        };

        let mut principal_values: Vec<String> = Vec::new();
        if let Some(user_id) = auth.user_id().map(|id| id.to_string()) {
            principal_values.push(user_id);
        }
        if let Some(subject) = auth.principal_id() {
            if !principal_values.iter().any(|v| v == &subject) {
                principal_values.push(subject);
            }
        }

        let mut has_scope_key = false;

        for key in [
            "user_id",
            "userId",
            "owner_id",
            "ownerId",
            "owner_subject",
            "ownerSubject",
            "subject",
            "sub",
            "principal_id",
            "principalId",
        ] {
            let Some(value) = obj.get(key) else {
                continue;
            };
            has_scope_key = true;

            if !auth.is_authenticated() {
                return Err(forge_core::ForgeError::Unauthorized(format!(
                    "Function '{function_name}' requires authentication for identity-scoped argument '{key}'"
                )));
            }

            let serde_json::Value::String(actual) = value else {
                return Err(forge_core::ForgeError::InvalidArgument(format!(
                    "Function '{function_name}' argument '{key}' must be a non-empty string"
                )));
            };

            if actual.trim().is_empty() || !principal_values.iter().any(|v| v == actual) {
                return Err(forge_core::ForgeError::Forbidden(format!(
                    "Function '{function_name}' argument '{key}' does not match authenticated principal"
                )));
            }
        }

        for key in ["tenant_id", "tenantId"] {
            let Some(value) = obj.get(key) else {
                continue;
            };
            has_scope_key = true;

            if !auth.is_authenticated() {
                return Err(forge_core::ForgeError::Unauthorized(format!(
                    "Function '{function_name}' requires authentication for tenant-scoped argument '{key}'"
                )));
            }

            let expected = auth
                .claim("tenant_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    forge_core::ForgeError::Forbidden(format!(
                        "Function '{function_name}' argument '{key}' is not allowed for this principal"
                    ))
                })?;

            let serde_json::Value::String(actual) = value else {
                return Err(forge_core::ForgeError::InvalidArgument(format!(
                    "Function '{function_name}' argument '{key}' must be a non-empty string"
                )));
            };

            if actual.trim().is_empty() || actual != expected {
                return Err(forge_core::ForgeError::Forbidden(format!(
                    "Function '{function_name}' argument '{key}' does not match authenticated tenant"
                )));
            }
        }

        if enforce_scope && auth.is_authenticated() && !has_scope_key {
            return Err(forge_core::ForgeError::Forbidden(format!(
                "Function '{function_name}' must include identity or tenant scope arguments"
            )));
        }

        Ok(())
    }

    async fn ensure_job_access(
        db_pool: &sqlx::PgPool,
        job_id: Uuid,
        auth: &forge_core::function::AuthContext,
    ) -> forge_core::Result<()> {
        let owner_subject_row: Option<(Option<String>,)> = sqlx::query_as(
            r#"
            SELECT owner_subject
            FROM forge_jobs
            WHERE id = $1
            "#,
        )
        .bind(job_id)
        .fetch_optional(db_pool)
        .await
        .map_err(forge_core::ForgeError::Sql)?;

        let owner_subject = owner_subject_row
            .ok_or_else(|| forge_core::ForgeError::NotFound(format!("Job {} not found", job_id)))?
            .0;

        Self::check_owner_access(owner_subject, auth)
    }

    async fn ensure_workflow_access(
        db_pool: &sqlx::PgPool,
        workflow_id: Uuid,
        auth: &forge_core::function::AuthContext,
    ) -> forge_core::Result<()> {
        let owner_subject_row: Option<(Option<String>,)> = sqlx::query_as(
            r#"
            SELECT owner_subject
            FROM forge_workflow_runs
            WHERE id = $1
            "#,
        )
        .bind(workflow_id)
        .fetch_optional(db_pool)
        .await
        .map_err(forge_core::ForgeError::Sql)?;

        let owner_subject = owner_subject_row
            .ok_or_else(|| {
                forge_core::ForgeError::NotFound(format!("Workflow {} not found", workflow_id))
            })?
            .0;

        Self::check_owner_access(owner_subject, auth)
    }

    fn check_owner_access(
        owner_subject: Option<String>,
        auth: &forge_core::function::AuthContext,
    ) -> forge_core::Result<()> {
        if auth.is_admin() {
            return Ok(());
        }

        let principal = auth.principal_id().ok_or_else(|| {
            forge_core::ForgeError::Unauthorized("Authentication required".to_string())
        })?;

        match owner_subject {
            Some(owner) if owner == principal => Ok(()),
            Some(_) => Err(forge_core::ForgeError::Forbidden(
                "Not authorized to access this resource".to_string(),
            )),
            None => Err(forge_core::ForgeError::Forbidden(
                "Resource has no owner; admin role required".to_string(),
            )),
        }
    }

    /// Stop the reactor.
    pub fn stop(&self) {
        let _ = self.shutdown_tx.send(());
        self.change_listener.stop();
    }

    /// Get reactor statistics.
    pub async fn stats(&self) -> ReactorStats {
        let session_stats = self.session_server.stats().await;
        let inv_stats = self.invalidation_engine.stats().await;

        ReactorStats {
            connections: session_stats.connections,
            subscriptions: session_stats.subscriptions,
            pending_invalidations: inv_stats.pending_subscriptions,
            listener_running: self.change_listener.is_running(),
        }
    }
}

/// Reactor statistics.
#[derive(Debug, Clone)]
pub struct ReactorStats {
    pub connections: usize,
    pub subscriptions: usize,
    pub pending_invalidations: usize,
    pub listener_running: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_reactor_config_default() {
        let config = ReactorConfig::default();
        assert_eq!(config.listener.channel, "forge_changes");
        assert_eq!(config.invalidation.debounce_ms, 50);
        assert_eq!(config.max_listener_restarts, 5);
        assert_eq!(config.listener_restart_delay_ms, 1000);
    }

    #[test]
    fn test_compute_hash() {
        let data1 = serde_json::json!({"name": "test"});
        let data2 = serde_json::json!({"name": "test"});
        let data3 = serde_json::json!({"name": "different"});

        let hash1 = Reactor::compute_hash(&data1);
        let hash2 = Reactor::compute_hash(&data2);
        let hash3 = Reactor::compute_hash(&data3);

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_check_identity_args_rejects_cross_user() {
        let user_id = uuid::Uuid::new_v4();
        let auth = forge_core::function::AuthContext::authenticated(
            user_id,
            vec!["user".to_string()],
            HashMap::from([(
                "sub".to_string(),
                serde_json::Value::String(user_id.to_string()),
            )]),
        );

        let result = Reactor::check_identity_args(
            "list_orders",
            &serde_json::json!({"user_id": uuid::Uuid::new_v4().to_string()}),
            &auth,
            true,
        );
        assert!(matches!(result, Err(forge_core::ForgeError::Forbidden(_))));
    }

    #[test]
    fn test_check_identity_args_requires_scope_for_non_public_queries() {
        let user_id = uuid::Uuid::new_v4();
        let auth = forge_core::function::AuthContext::authenticated(
            user_id,
            vec!["user".to_string()],
            HashMap::from([(
                "sub".to_string(),
                serde_json::Value::String(user_id.to_string()),
            )]),
        );

        let result = Reactor::check_identity_args("list_orders", &serde_json::json!({}), &auth, true);
        assert!(matches!(result, Err(forge_core::ForgeError::Forbidden(_))));
    }

    #[test]
    fn test_check_owner_access_allows_admin() {
        let auth = forge_core::function::AuthContext::authenticated_without_uuid(
            vec!["admin".to_string()],
            HashMap::from([(
                "sub".to_string(),
                serde_json::Value::String("admin-1".to_string()),
            )]),
        );

        let result = Reactor::check_owner_access(Some("other-user".to_string()), &auth);
        assert!(result.is_ok());
    }
}
