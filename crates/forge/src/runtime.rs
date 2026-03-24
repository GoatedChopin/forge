//! FORGE - The Rust Full-Stack Framework
//!
//! Single binary runtime that provides:
//! - HTTP Gateway with RPC endpoints
//! - SSE server for real-time subscriptions
//! - Background job workers
//! - Cron scheduler
//! - Workflow engine
//! - Cluster coordination

use std::future::Future;
use std::net::IpAddr;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::Request;
use axum::response::Response;
use tokio::sync::broadcast;

use forge_core::CircuitBreakerClient;
use forge_core::cluster::{LeaderRole, NodeId, NodeInfo, NodeRole, NodeStatus};
use forge_core::config::{ForgeConfig, NodeRole as ConfigNodeRole};
use forge_core::error::{ForgeError, Result};
use forge_core::function::{ForgeMutation, ForgeQuery};
use forge_core::mcp::ForgeMcpTool;
use forge_runtime::migrations::{Migration, MigrationRunner, load_migrations_from_dir};

use forge_runtime::cluster::{
    GracefulShutdown, HeartbeatConfig, HeartbeatLoop, LeaderConfig, LeaderElection, NodeRegistry,
    ShutdownConfig,
};
use forge_runtime::cron::{CronRegistry, CronRunner, CronRunnerConfig};
use forge_runtime::daemon::{DaemonRegistry, DaemonRunner};
use forge_runtime::db::Database;
use forge_runtime::function::FunctionRegistry;
use forge_runtime::gateway::{AuthConfig, GatewayConfig as RuntimeGatewayConfig, GatewayServer};
use forge_runtime::jobs::{JobDispatcher, JobQueue, JobRegistry, Worker, WorkerConfig};
use forge_runtime::mcp::McpToolRegistry;
use forge_runtime::webhook::{WebhookRegistry, WebhookState, webhook_handler};
use forge_runtime::workflow::{
    EventStore, WorkflowExecutor, WorkflowRegistry, WorkflowScheduler, WorkflowSchedulerConfig,
};
use tokio_util::sync::CancellationToken;

/// Type alias for frontend handler function.
pub type FrontendHandler = fn(Request<Body>) -> Pin<Box<dyn Future<Output = Response> + Send>>;

/// Prelude module for common imports.
pub mod prelude {
    // Common types
    pub use chrono::{DateTime, Utc};
    pub use uuid::Uuid;

    // Serde re-exports for user code
    pub use serde::{Deserialize, Serialize};
    pub use serde_json;

    /// Timestamp type alias for convenience.
    pub type Timestamp = DateTime<Utc>;

    // Core types
    pub use forge_core::auth::TokenPair;
    pub use forge_core::cluster::NodeRole;
    pub use forge_core::config::ForgeConfig;
    pub use forge_core::cron::{CronContext, ForgeCron};
    pub use forge_core::daemon::{DaemonContext, ForgeDaemon};
    pub use forge_core::env::EnvAccess;
    pub use forge_core::error::{ForgeError, Result};
    pub use forge_core::function::{
        AuthContext, ForgeMutation, ForgeQuery, MutationContext, QueryContext,
    };
    pub use forge_core::job::{ForgeJob, JobContext, JobPriority};
    pub use forge_core::mcp::{ForgeMcpTool, McpToolContext, McpToolResult};
    pub use forge_core::realtime::Delta;
    pub use forge_core::schema::{FieldDef, ModelMeta, SchemaRegistry, TableDef};
    pub use forge_core::schemars::JsonSchema;
    pub use forge_core::types::Upload;
    pub use forge_core::webhook::{ForgeWebhook, WebhookContext, WebhookResult, WebhookSignature};
    pub use forge_core::workflow::{ForgeWorkflow, WorkflowContext};

    // Same axum version the runtime uses, avoids type mismatches in custom handlers
    pub use axum;

    pub use crate::{Forge, ForgeBuilder};
}

/// The main FORGE runtime.
pub struct Forge {
    config: ForgeConfig,
    db: Option<Database>,
    node_id: NodeId,
    function_registry: FunctionRegistry,
    mcp_registry: McpToolRegistry,
    job_registry: JobRegistry,
    cron_registry: Arc<CronRegistry>,
    workflow_registry: WorkflowRegistry,
    daemon_registry: Arc<DaemonRegistry>,
    webhook_registry: Arc<WebhookRegistry>,
    shutdown_tx: broadcast::Sender<()>,
    /// Path to user migrations directory (default: ./migrations).
    migrations_dir: PathBuf,
    /// Additional migrations provided programmatically.
    extra_migrations: Vec<Migration>,
    /// Optional frontend handler for embedded SPA.
    frontend_handler: Option<FrontendHandler>,
    /// Custom axum routes merged into the top-level router.
    custom_routes: Option<Router>,
}

impl Forge {
    /// Create a new builder for configuring FORGE.
    pub fn builder() -> ForgeBuilder {
        ForgeBuilder::new()
    }

    /// Get the node ID.
    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    /// Get the configuration.
    pub fn config(&self) -> &ForgeConfig {
        &self.config
    }

    /// Get the function registry.
    pub fn function_registry(&self) -> &FunctionRegistry {
        &self.function_registry
    }

    /// Get the function registry mutably.
    pub fn function_registry_mut(&mut self) -> &mut FunctionRegistry {
        &mut self.function_registry
    }

    /// Get the MCP tool registry mutably.
    pub fn mcp_registry_mut(&mut self) -> &mut McpToolRegistry {
        &mut self.mcp_registry
    }

    /// Register an MCP tool without manually accessing the registry.
    pub fn register_mcp_tool<T: ForgeMcpTool>(&mut self) -> &mut Self {
        self.mcp_registry.register::<T>();
        self
    }

    /// Get the job registry.
    pub fn job_registry(&self) -> &JobRegistry {
        &self.job_registry
    }

    /// Get the job registry mutably.
    pub fn job_registry_mut(&mut self) -> &mut JobRegistry {
        &mut self.job_registry
    }

    /// Get the cron registry.
    pub fn cron_registry(&self) -> Arc<CronRegistry> {
        self.cron_registry.clone()
    }

    /// Get the workflow registry.
    pub fn workflow_registry(&self) -> &WorkflowRegistry {
        &self.workflow_registry
    }

    /// Get the workflow registry mutably.
    pub fn workflow_registry_mut(&mut self) -> &mut WorkflowRegistry {
        &mut self.workflow_registry
    }

    /// Get the daemon registry.
    pub fn daemon_registry(&self) -> Arc<DaemonRegistry> {
        self.daemon_registry.clone()
    }

    /// Get the webhook registry.
    pub fn webhook_registry(&self) -> Arc<WebhookRegistry> {
        self.webhook_registry.clone()
    }

    /// Run the FORGE server.
    pub async fn run(mut self) -> Result<()> {
        // Users shouldn't need tracing_subscriber boilerplate to see logs
        let telemetry_config = forge_runtime::TelemetryConfig::from_observability_config(
            &self.config.observability,
            &self.config.project.name,
            &self.config.project.version,
        );
        match forge_runtime::init_telemetry(
            &telemetry_config,
            &self.config.project.name,
            &self.config.observability.log_level,
        ) {
            Ok(true) => {}
            Ok(false) => {
                // Subscriber already exists, user set one up manually
            }
            Err(e) => {
                eprintln!("forge: failed to initialize telemetry: {e}");
            }
        }

        tracing::debug!("Connecting to database");

        // Connect to database
        let db =
            Database::from_config_with_service(&self.config.database, &self.config.project.name)
                .await?;
        let pool = db.primary().clone();
        let jobs_pool = db.jobs_pool().clone();
        let observability_pool = db.observability_pool().clone();
        if let Some(handle) = db.start_health_monitor() {
            let mut shutdown_rx = self.shutdown_tx.subscribe();
            tokio::spawn(async move {
                tokio::select! {
                    _ = shutdown_rx.recv() => {}
                    _ = handle => {}
                }
            });
        }
        self.db = Some(db);

        tracing::debug!("Database connected");

        // Run migrations with mesh-safe locking
        // This acquires an advisory lock, so only one node runs migrations at a time
        let runner = MigrationRunner::new(pool.clone());

        // Load user migrations from directory + any programmatic ones
        let mut user_migrations = load_migrations_from_dir(&self.migrations_dir)?;
        user_migrations.extend(self.extra_migrations.clone());

        runner.run(user_migrations).await?;
        tracing::debug!("Migrations applied");

        // Get local node info
        let hostname = get_hostname();

        let ip_address: IpAddr = "127.0.0.1".parse().expect("valid IP literal");
        let roles: Vec<NodeRole> = self
            .config
            .node
            .roles
            .iter()
            .map(config_role_to_node_role)
            .collect();

        let node_info = NodeInfo::new_local(
            hostname,
            ip_address,
            self.config.gateway.port,
            self.config.gateway.grpc_port,
            roles.clone(),
            self.config.node.worker_capabilities.clone(),
            env!("CARGO_PKG_VERSION").to_string(),
        );

        let node_id = node_info.id;
        self.node_id = node_id;

        // Create node registry
        let node_registry = Arc::new(NodeRegistry::new(pool.clone(), node_info));

        // Register node in cluster
        if let Err(e) = node_registry.register().await {
            tracing::debug!("Failed to register node (tables may not exist): {}", e);
        }

        // Set node status to active
        if let Err(e) = node_registry.set_status(NodeStatus::Active).await {
            tracing::debug!("Failed to set node status: {}", e);
        }

        // Create leader election for scheduler role
        let leader_election = if roles.contains(&NodeRole::Scheduler) {
            let election = Arc::new(LeaderElection::new(
                pool.clone(),
                node_id,
                LeaderRole::Scheduler,
                LeaderConfig::default(),
            ));

            // Try to become leader
            if let Err(e) = election.try_become_leader().await {
                tracing::debug!("Failed to acquire leadership: {}", e);
            }

            Some(election)
        } else {
            None
        };

        // Create graceful shutdown coordinator
        let shutdown = Arc::new(GracefulShutdown::new(
            node_registry.clone(),
            leader_election.clone(),
            ShutdownConfig::default(),
        ));

        // Create HTTP client with circuit breaker for actions and crons
        let http_client = CircuitBreakerClient::with_defaults(reqwest::Client::new());

        // Start background tasks based on roles
        let mut handles = Vec::new();

        // Start heartbeat loop
        {
            let heartbeat_pool = pool.clone();
            let heartbeat_node_id = node_id;
            let config = HeartbeatConfig::from_cluster_config(&self.config.cluster);
            handles.push(tokio::spawn(async move {
                let heartbeat = HeartbeatLoop::new(heartbeat_pool, heartbeat_node_id, config);
                heartbeat.run().await;
            }));
        }

        // Start leader election loop if scheduler role
        if let Some(ref election) = leader_election {
            let election = election.clone();
            handles.push(tokio::spawn(async move {
                election.run().await;
            }));
        }

        // Start job worker if worker role
        if roles.contains(&NodeRole::Worker) {
            let job_queue = JobQueue::new(jobs_pool.clone());
            let worker_config = WorkerConfig {
                id: Some(node_id.as_uuid()),
                capabilities: self.config.node.worker_capabilities.clone(),
                max_concurrent: self.config.worker.max_concurrent_jobs,
                poll_interval: Duration::from_millis(self.config.worker.poll_interval_ms),
                ..Default::default()
            };

            let mut worker = Worker::new(
                worker_config,
                job_queue,
                self.job_registry.clone(),
                jobs_pool.clone(),
            );

            handles.push(tokio::spawn(async move {
                if let Err(e) = worker.run().await {
                    tracing::error!("Worker error: {}", e);
                }
            }));

            tracing::debug!("Job worker started");
        }

        // Start cron runner if scheduler role and is leader
        if roles.contains(&NodeRole::Scheduler) {
            let cron_registry = self.cron_registry.clone();
            let cron_pool = jobs_pool.clone();
            let cron_http = http_client.clone();
            let cron_leader_election = leader_election.clone();

            let cron_config = CronRunnerConfig {
                poll_interval: Duration::from_secs(1),
                node_id: node_id.as_uuid(),
                is_leader: cron_leader_election.is_none(),
                leader_election: cron_leader_election,
                run_stale_threshold: Duration::from_secs(15 * 60),
            };

            let cron_runner = CronRunner::new(cron_registry, cron_pool, cron_http, cron_config);

            handles.push(tokio::spawn(async move {
                if let Err(e) = cron_runner.run().await {
                    tracing::error!("Cron runner error: {}", e);
                }
            }));

            tracing::debug!("Cron scheduler started");
        }

        // Start workflow scheduler if scheduler role
        let workflow_shutdown_token = CancellationToken::new();
        if roles.contains(&NodeRole::Scheduler) {
            let scheduler_executor = Arc::new(WorkflowExecutor::new(
                Arc::new(self.workflow_registry.clone()),
                jobs_pool.clone(),
                http_client.clone(),
            ));
            let event_store = Arc::new(EventStore::new(jobs_pool.clone()));
            let scheduler = WorkflowScheduler::new(
                jobs_pool.clone(),
                scheduler_executor,
                event_store,
                WorkflowSchedulerConfig::default(),
            );

            let shutdown_token = workflow_shutdown_token.clone();
            handles.push(tokio::spawn(async move {
                scheduler.run(shutdown_token).await;
            }));

            tracing::debug!("Workflow scheduler started");
        }

        // Create job dispatcher and workflow executor for dispatch capabilities
        let job_queue_for_dispatch = JobQueue::new(jobs_pool.clone());
        let job_dispatcher = Arc::new(JobDispatcher::new(
            job_queue_for_dispatch,
            self.job_registry.clone(),
        ));
        let workflow_executor = Arc::new(WorkflowExecutor::new(
            Arc::new(self.workflow_registry.clone()),
            jobs_pool.clone(),
            http_client.clone(),
        ));

        // Start daemon runner if scheduler role (daemons run as singletons)
        if roles.contains(&NodeRole::Scheduler) && !self.daemon_registry.is_empty() {
            let daemon_registry = self.daemon_registry.clone();
            let daemon_pool = jobs_pool.clone();
            let daemon_http = http_client.clone();
            let daemon_shutdown_rx = self.shutdown_tx.subscribe();

            let daemon_runner = DaemonRunner::new(
                daemon_registry,
                daemon_pool,
                daemon_http,
                node_id.as_uuid(),
                daemon_shutdown_rx,
            )
            .with_job_dispatch(job_dispatcher.clone())
            .with_workflow_dispatch(workflow_executor.clone());

            handles.push(tokio::spawn(async move {
                if let Err(e) = daemon_runner.run().await {
                    tracing::error!("Daemon runner error: {}", e);
                }
            }));

            tracing::debug!("Daemon runner started");
        }

        // Reactor handle for shutdown
        let mut reactor_handle = None;

        // Start HTTP gateway if gateway role
        if roles.contains(&NodeRole::Gateway) {
            let gateway_config = RuntimeGatewayConfig {
                port: self.config.gateway.port,
                max_connections: self.config.gateway.max_connections,
                sse_max_sessions: self.config.gateway.sse_max_sessions,
                request_timeout_secs: self.config.gateway.request_timeout_secs,
                cors_enabled: self.config.gateway.cors_enabled
                    || !self.config.gateway.cors_origins.is_empty(),
                cors_origins: self.config.gateway.cors_origins.clone(),
                auth: AuthConfig::from_forge_config(&self.config.auth)
                    .map_err(|e| ForgeError::Config(e.to_string()))?,
                mcp: self.config.mcp.clone(),
                quiet_routes: self.config.gateway.quiet_routes.clone(),
                token_ttl: forge_core::AuthTokenTtl {
                    access_token_secs: self.config.auth.access_token_ttl_secs(),
                    refresh_token_days: self.config.auth.refresh_token_ttl_days(),
                },
            };

            // Build gateway server (pass Database wrapper for read replica routing)
            let gateway = GatewayServer::new(
                gateway_config,
                self.function_registry.clone(),
                self.db
                    .clone()
                    .ok_or_else(|| ForgeError::Internal("Database not initialized".into()))?,
            )
            .with_job_dispatcher(job_dispatcher.clone())
            .with_workflow_dispatcher(workflow_executor.clone())
            .with_mcp_registry(self.mcp_registry.clone());

            // Start the reactor for real-time updates
            let reactor = gateway.reactor();
            if let Err(e) = reactor.start().await {
                tracing::error!("Failed to start reactor: {}", e);
            } else {
                tracing::debug!("Reactor started");
                reactor_handle = Some(reactor);
            }

            // Build API router (all under /_api)
            let api_router = gateway.router();

            // Build final router with API
            let mut router = Router::new().nest("/_api", api_router);

            // Mount webhook routes under /_api (bypasses gateway auth middleware)
            if !self.webhook_registry.is_empty() {
                use axum::routing::post;
                use tower_http::cors::{Any, CorsLayer};

                let webhook_state = Arc::new(
                    WebhookState::new(self.webhook_registry.clone(), pool.clone())
                        .with_job_dispatcher(job_dispatcher.clone()),
                );

                // Webhook routes need their own CORS layer since they're outside the API router.
                // Reuse gateway CORS policy rather than forcing wildcard access.
                let webhook_cors = if self.config.gateway.cors_enabled
                    || !self.config.gateway.cors_origins.is_empty()
                {
                    if self.config.gateway.cors_origins.iter().any(|o| o == "*") {
                        CorsLayer::new()
                            .allow_origin(Any)
                            .allow_methods(Any)
                            .allow_headers(Any)
                    } else {
                        let origins: Vec<_> = self
                            .config
                            .gateway
                            .cors_origins
                            .iter()
                            .filter_map(|o| o.parse().ok())
                            .collect();
                        CorsLayer::new()
                            .allow_origin(origins)
                            .allow_methods(Any)
                            .allow_headers(Any)
                    }
                } else {
                    CorsLayer::new()
                };

                let webhook_router = Router::new()
                    .route("/{*path}", post(webhook_handler).with_state(webhook_state))
                    .layer(axum::extract::DefaultBodyLimit::max(1024 * 1024))
                    .layer(
                        tower::ServiceBuilder::new()
                            .layer(axum::error_handling::HandleErrorLayer::new(
                                |err: tower::BoxError| async move {
                                    if err.is::<tower::timeout::error::Elapsed>() {
                                        return (
                                            axum::http::StatusCode::REQUEST_TIMEOUT,
                                            "Request timed out",
                                        );
                                    }
                                    (
                                        axum::http::StatusCode::SERVICE_UNAVAILABLE,
                                        "Server overloaded",
                                    )
                                },
                            ))
                            .layer(tower::limit::ConcurrencyLimitLayer::new(
                                self.config.gateway.max_connections,
                            ))
                            .layer(tower::timeout::TimeoutLayer::new(Duration::from_secs(
                                self.config.gateway.request_timeout_secs,
                            ))),
                    )
                    .layer(webhook_cors);

                router = router.nest("/_api/webhooks", webhook_router);

                tracing::debug!(
                    webhooks = ?self.webhook_registry.paths().collect::<Vec<_>>(),
                    "Webhook routes registered"
                );
            }

            // MCP OAuth/resource discovery: return JSON 404 so MCP clients
            // (like Claude Code) get a parseable response instead of an empty
            // HTML page from the frontend fallback, and gracefully skip auth.
            if self.config.mcp.enabled {
                use axum::routing::get;
                async fn oauth_not_supported() -> impl axum::response::IntoResponse {
                    (
                        axum::http::StatusCode::NOT_FOUND,
                        axum::Json(serde_json::json!({
                            "error": "oauth_not_supported",
                            "error_description": "This server does not support OAuth. Connect without authentication."
                        })),
                    )
                }
                router = router
                    .route("/.well-known/oauth-authorization-server", get(oauth_not_supported))
                    .route("/.well-known/oauth-protected-resource", get(oauth_not_supported));
            }

            // Merge custom routes before frontend fallback so they take precedence
            if let Some(custom) = self.custom_routes.take() {
                router = router.merge(custom);
                tracing::debug!("Custom routes merged");
            }

            // Add frontend handler as fallback if configured
            if let Some(handler) = self.frontend_handler {
                use axum::routing::get;
                router = router.fallback(get(handler));
                tracing::debug!("Frontend handler enabled");
            }

            let addr = gateway.addr();

            handles.push(tokio::spawn(async move {
                tracing::debug!(addr = %addr, "Gateway server binding");
                let listener = tokio::net::TcpListener::bind(addr)
                    .await
                    .expect("Failed to bind");
                if let Err(e) = axum::serve(listener, router).await {
                    tracing::error!("Gateway server error: {}", e);
                }
            }));
        }

        tracing::info!(
            queries = self.function_registry.queries().count(),
            mutations = self.function_registry.mutations().count(),
            jobs = self.job_registry.len(),
            crons = self.cron_registry.len(),
            workflows = self.workflow_registry.len(),
            daemons = self.daemon_registry.len(),
            webhooks = self.webhook_registry.len(),
            mcp_tools = self.mcp_registry.len(),
            "Functions registered"
        );

        {
            let metrics_pool = observability_pool;
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(15)).await;
                    forge_runtime::observability::record_pool_metrics(&metrics_pool);
                }
            });
        }

        tracing::info!(
            node_id = %node_id,
            roles = ?roles,
            port = self.config.gateway.port,
            "Forge started"
        );

        // Wait for shutdown signal
        let mut shutdown_rx = self.shutdown_tx.subscribe();

        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                tracing::debug!("Received ctrl-c");
            }
            _ = shutdown_rx.recv() => {
                tracing::debug!("Received shutdown notification");
            }
        }

        // Graceful shutdown
        tracing::debug!("Graceful shutdown starting");

        // Stop workflow scheduler
        workflow_shutdown_token.cancel();

        if let Err(e) = shutdown.shutdown().await {
            tracing::warn!(error = %e, "Shutdown error");
        }

        // Stop leader election
        if let Some(ref election) = leader_election {
            election.stop();
        }

        // Stop reactor before closing database
        if let Some(ref reactor) = reactor_handle {
            reactor.stop();
        }

        // Close database connections
        if let Some(ref db) = self.db {
            db.close().await;
        }

        forge_runtime::shutdown_telemetry();
        tracing::info!("Forge stopped");
        Ok(())
    }

    /// Request shutdown.
    pub fn shutdown(&self) {
        let _ = self.shutdown_tx.send(());
    }
}

/// Builder for configuring the FORGE runtime.
pub struct ForgeBuilder {
    config: Option<ForgeConfig>,
    function_registry: FunctionRegistry,
    mcp_registry: McpToolRegistry,
    job_registry: JobRegistry,
    cron_registry: CronRegistry,
    workflow_registry: WorkflowRegistry,
    daemon_registry: DaemonRegistry,
    webhook_registry: WebhookRegistry,
    migrations_dir: PathBuf,
    extra_migrations: Vec<Migration>,
    frontend_handler: Option<FrontendHandler>,
    custom_routes: Option<Router>,
}

impl ForgeBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self {
            config: None,
            function_registry: FunctionRegistry::new(),
            mcp_registry: McpToolRegistry::new(),
            job_registry: JobRegistry::new(),
            cron_registry: CronRegistry::new(),
            workflow_registry: WorkflowRegistry::new(),
            daemon_registry: DaemonRegistry::new(),
            webhook_registry: WebhookRegistry::new(),
            migrations_dir: PathBuf::from("migrations"),
            extra_migrations: Vec::new(),
            frontend_handler: None,
            custom_routes: None,
        }
    }

    /// Set the directory to load migrations from.
    ///
    /// Defaults to `./migrations`. Migration files should be named like:
    /// - `0001_create_users.sql`
    /// - `0002_add_posts.sql`
    pub fn migrations_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.migrations_dir = path.into();
        self
    }

    /// Add a migration programmatically.
    ///
    /// Use this for migrations that need to be generated at runtime,
    /// or for testing. For most cases, use migration files instead.
    pub fn migration(mut self, name: impl Into<String>, sql: impl Into<String>) -> Self {
        self.extra_migrations.push(Migration::new(name, sql));
        self
    }

    /// Set a frontend handler for serving embedded SPA assets.
    ///
    /// Use with the `embedded-frontend` feature to build a single binary
    /// that includes both backend and frontend.
    pub fn frontend_handler(mut self, handler: FrontendHandler) -> Self {
        self.frontend_handler = Some(handler);
        self
    }

    /// Add custom axum routes to the server.
    ///
    /// Routes are merged at the top level, outside `/_api`, giving full
    /// control over headers, extractors, and response types. Avoid paths
    /// starting with `/_api` as they conflict with internal routes.
    ///
    /// ```ignore
    /// use axum::{Router, routing::get};
    ///
    /// let routes = Router::new()
    ///     .route("/custom/health", get(|| async { "ok" }));
    ///
    /// builder.custom_routes(routes);
    /// ```
    pub fn custom_routes(mut self, router: Router) -> Self {
        self.custom_routes = Some(router);
        self
    }

    /// Automatically register all functions discovered via `#[forge::query]`,
    /// `#[forge::mutation]`, `#[forge::job]`, `#[forge::cron]`, `#[forge::workflow]`,
    /// `#[forge::daemon]`, `#[forge::webhook]`, and `#[forge::mcp_tool]` macros.
    ///
    /// This replaces the need to manually call `.register_query::<T>()` etc.
    /// for every function in your application.
    pub fn auto_register(mut self) -> Self {
        crate::auto_register::auto_register_all(
            &mut self.function_registry,
            &mut self.job_registry,
            &mut self.cron_registry,
            &mut self.workflow_registry,
            &mut self.daemon_registry,
            &mut self.webhook_registry,
            &mut self.mcp_registry,
        );
        self
    }

    /// Set the configuration.
    pub fn config(mut self, config: ForgeConfig) -> Self {
        self.config = Some(config);
        self
    }

    /// Get mutable access to the function registry.
    pub fn function_registry_mut(&mut self) -> &mut FunctionRegistry {
        &mut self.function_registry
    }

    /// Get mutable access to the job registry.
    pub fn job_registry_mut(&mut self) -> &mut JobRegistry {
        &mut self.job_registry
    }

    /// Get mutable access to the MCP tool registry.
    pub fn mcp_registry_mut(&mut self) -> &mut McpToolRegistry {
        &mut self.mcp_registry
    }

    /// Register an MCP tool without manually accessing the registry.
    pub fn register_mcp_tool<T: ForgeMcpTool>(mut self) -> Self {
        self.mcp_registry.register::<T>();
        self
    }

    /// Get mutable access to the cron registry.
    pub fn cron_registry_mut(&mut self) -> &mut CronRegistry {
        &mut self.cron_registry
    }

    /// Get mutable access to the workflow registry.
    pub fn workflow_registry_mut(&mut self) -> &mut WorkflowRegistry {
        &mut self.workflow_registry
    }

    /// Get mutable access to the daemon registry.
    pub fn daemon_registry_mut(&mut self) -> &mut DaemonRegistry {
        &mut self.daemon_registry
    }

    /// Get mutable access to the webhook registry.
    pub fn webhook_registry_mut(&mut self) -> &mut WebhookRegistry {
        &mut self.webhook_registry
    }

    /// Register a query function.
    pub fn register_query<Q: ForgeQuery>(mut self) -> Self
    where
        Q::Args: serde::de::DeserializeOwned + Send + 'static,
        Q::Output: serde::Serialize + Send + 'static,
    {
        self.function_registry.register_query::<Q>();
        self
    }

    /// Register a mutation function.
    pub fn register_mutation<M: ForgeMutation>(mut self) -> Self
    where
        M::Args: serde::de::DeserializeOwned + Send + 'static,
        M::Output: serde::Serialize + Send + 'static,
    {
        self.function_registry.register_mutation::<M>();
        self
    }

    /// Register a background job.
    pub fn register_job<J: forge_core::ForgeJob>(mut self) -> Self
    where
        J::Args: serde::de::DeserializeOwned + Send + 'static,
        J::Output: serde::Serialize + Send + 'static,
    {
        self.job_registry.register::<J>();
        self
    }

    /// Register a cron handler.
    pub fn register_cron<C: forge_core::ForgeCron>(mut self) -> Self {
        self.cron_registry.register::<C>();
        self
    }

    /// Register a workflow.
    pub fn register_workflow<W: forge_core::ForgeWorkflow>(mut self) -> Self
    where
        W::Input: serde::de::DeserializeOwned,
        W::Output: serde::Serialize,
    {
        self.workflow_registry.register::<W>();
        self
    }

    /// Register a daemon.
    pub fn register_daemon<D: forge_core::ForgeDaemon>(mut self) -> Self {
        self.daemon_registry.register::<D>();
        self
    }

    /// Register a webhook.
    pub fn register_webhook<W: forge_core::ForgeWebhook>(mut self) -> Self {
        self.webhook_registry.register::<W>();
        self
    }

    /// Build the FORGE runtime.
    pub fn build(self) -> Result<Forge> {
        let config = self
            .config
            .ok_or_else(|| ForgeError::Config("Configuration is required".to_string()))?;

        let (shutdown_tx, _) = broadcast::channel(1);

        Ok(Forge {
            config,
            db: None,
            node_id: NodeId::new(),
            function_registry: self.function_registry,
            mcp_registry: self.mcp_registry,
            job_registry: self.job_registry,
            cron_registry: Arc::new(self.cron_registry),
            workflow_registry: self.workflow_registry,
            daemon_registry: Arc::new(self.daemon_registry),
            webhook_registry: Arc::new(self.webhook_registry),
            shutdown_tx,
            migrations_dir: self.migrations_dir,
            extra_migrations: self.extra_migrations,
            frontend_handler: self.frontend_handler,
            custom_routes: self.custom_routes,
        })
    }
}

impl Default for ForgeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(unix)]
fn get_hostname() -> String {
    nix::unistd::gethostname()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".to_string())
}

#[cfg(not(unix))]
fn get_hostname() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "unknown".to_string())
}

/// Convert config NodeRole to cluster NodeRole.
fn config_role_to_node_role(role: &ConfigNodeRole) -> NodeRole {
    match role {
        ConfigNodeRole::Gateway => NodeRole::Gateway,
        ConfigNodeRole::Function => NodeRole::Function,
        ConfigNodeRole::Worker => NodeRole::Worker,
        ConfigNodeRole::Scheduler => NodeRole::Scheduler,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::pin::Pin;

    use forge_core::mcp::{McpToolAnnotations, McpToolInfo};

    struct TestMcpTool;

    impl ForgeMcpTool for TestMcpTool {
        type Args = serde_json::Value;
        type Output = serde_json::Value;

        fn info() -> McpToolInfo {
            McpToolInfo {
                name: "test.mcp.tool",
                title: None,
                description: None,
                required_role: None,
                is_public: false,
                timeout: None,
                rate_limit_requests: None,
                rate_limit_per_secs: None,
                rate_limit_key: None,
                annotations: McpToolAnnotations::default(),
                icons: &[],
            }
        }

        fn execute(
            _ctx: &forge_core::McpToolContext,
            _args: Self::Args,
        ) -> Pin<Box<dyn Future<Output = forge_core::Result<Self::Output>> + Send + '_>> {
            Box::pin(async { Ok(serde_json::json!({ "ok": true })) })
        }
    }

    #[test]
    fn test_forge_builder_new() {
        let builder = ForgeBuilder::new();
        assert!(builder.config.is_none());
    }

    #[test]
    fn test_forge_builder_requires_config() {
        let builder = ForgeBuilder::new();
        let result = builder.build();
        assert!(result.is_err());
    }

    #[test]
    fn test_forge_builder_with_config() {
        let config = ForgeConfig::default_with_database_url("postgres://localhost/test");
        let result = ForgeBuilder::new().config(config).build();
        assert!(result.is_ok());
    }

    #[test]
    fn test_forge_builder_register_mcp_tool() {
        let builder = ForgeBuilder::new().register_mcp_tool::<TestMcpTool>();
        assert_eq!(builder.mcp_registry.len(), 1);
    }

    #[test]
    fn test_config_role_conversion() {
        assert_eq!(
            config_role_to_node_role(&ConfigNodeRole::Gateway),
            NodeRole::Gateway
        );
        assert_eq!(
            config_role_to_node_role(&ConfigNodeRole::Worker),
            NodeRole::Worker
        );
        assert_eq!(
            config_role_to_node_role(&ConfigNodeRole::Scheduler),
            NodeRole::Scheduler
        );
        assert_eq!(
            config_role_to_node_role(&ConfigNodeRole::Function),
            NodeRole::Function
        );
    }
}
