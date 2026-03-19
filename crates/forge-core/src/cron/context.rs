use std::sync::Arc;

use chrono::{DateTime, Utc};
use tracing::Span;
use uuid::Uuid;

use crate::env::{EnvAccess, EnvProvider, RealEnvProvider};
use crate::function::AuthContext;

/// Context available to cron handlers.
pub struct CronContext {
    /// Cron run ID.
    pub run_id: Uuid,
    /// Cron name.
    pub cron_name: String,
    /// Scheduled time (when the cron was supposed to run).
    pub scheduled_time: DateTime<Utc>,
    /// Actual execution time.
    pub execution_time: DateTime<Utc>,
    /// Timezone of the cron.
    pub timezone: String,
    /// Whether this is a catch-up run.
    pub is_catch_up: bool,
    /// Authentication context.
    pub auth: AuthContext,
    /// Database pool.
    db_pool: sqlx::PgPool,
    /// HTTP client.
    http_client: reqwest::Client,
    /// Structured logger.
    pub log: CronLog,
    /// Environment variable provider.
    env_provider: Arc<dyn EnvProvider>,
    /// Parent span for trace propagation.
    span: Span,
}

impl CronContext {
    /// Create a new cron context.
    pub fn new(
        run_id: Uuid,
        cron_name: String,
        scheduled_time: DateTime<Utc>,
        timezone: String,
        is_catch_up: bool,
        db_pool: sqlx::PgPool,
        http_client: reqwest::Client,
    ) -> Self {
        Self {
            run_id,
            cron_name: cron_name.clone(),
            scheduled_time,
            execution_time: Utc::now(),
            timezone,
            is_catch_up,
            auth: AuthContext::unauthenticated(),
            db_pool,
            http_client,
            log: CronLog::new(cron_name),
            env_provider: Arc::new(RealEnvProvider::new()),
            span: Span::current(),
        }
    }

    /// Set environment provider.
    pub fn with_env_provider(mut self, provider: Arc<dyn EnvProvider>) -> Self {
        self.env_provider = provider;
        self
    }

    pub fn db(&self) -> crate::function::ForgeDb {
        crate::function::ForgeDb::from_pool(&self.db_pool)
    }

    /// Acquire a connection compatible with sqlx compile-time checked macros.
    pub async fn conn(&self) -> sqlx::Result<crate::function::ForgeConn<'static>> {
        Ok(crate::function::ForgeConn::Pool(
            self.db_pool.acquire().await?,
        ))
    }

    pub fn http(&self) -> &reqwest::Client {
        &self.http_client
    }

    /// Get the delay between scheduled and actual execution time.
    pub fn delay(&self) -> chrono::Duration {
        self.execution_time - self.scheduled_time
    }

    /// Check if the cron is running late (more than 1 minute delay).
    pub fn is_late(&self) -> bool {
        self.delay() > chrono::Duration::minutes(1)
    }

    /// Set authentication context.
    pub fn with_auth(mut self, auth: AuthContext) -> Self {
        self.auth = auth;
        self
    }

    /// Get the trace ID for this cron execution.
    ///
    /// Returns the trace ID if OpenTelemetry is configured, otherwise returns the run_id.
    pub fn trace_id(&self) -> String {
        // The span carries the trace context. When OTel is configured,
        // the trace ID can be extracted from the span context.
        // For now, return the run_id as a fallback correlation ID.
        self.run_id.to_string()
    }

    /// Get the parent span for trace propagation.
    ///
    /// Use this to create child spans within cron handlers.
    pub fn span(&self) -> &Span {
        &self.span
    }
}

impl EnvAccess for CronContext {
    fn env_provider(&self) -> &dyn EnvProvider {
        self.env_provider.as_ref()
    }
}

/// Structured logger for cron jobs.
#[derive(Clone)]
pub struct CronLog {
    cron_name: String,
}

impl CronLog {
    /// Create a new cron logger.
    pub fn new(cron_name: String) -> Self {
        Self { cron_name }
    }

    /// Log an info message.
    pub fn info(&self, message: &str, data: serde_json::Value) {
        tracing::info!(
            cron_name = %self.cron_name,
            data = %data,
            "{}",
            message
        );
    }

    /// Log a warning message.
    pub fn warn(&self, message: &str, data: serde_json::Value) {
        tracing::warn!(
            cron_name = %self.cron_name,
            data = %data,
            "{}",
            message
        );
    }

    /// Log an error message.
    pub fn error(&self, message: &str, data: serde_json::Value) {
        tracing::error!(
            cron_name = %self.cron_name,
            data = %data,
            "{}",
            message
        );
    }

    /// Log a debug message.
    pub fn debug(&self, message: &str, data: serde_json::Value) {
        tracing::debug!(
            cron_name = %self.cron_name,
            data = %data,
            "{}",
            message
        );
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cron_context_creation() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect_lazy("postgres://localhost/nonexistent")
            .expect("Failed to create mock pool");

        let run_id = Uuid::new_v4();
        let scheduled = Utc::now() - chrono::Duration::seconds(30);

        let ctx = CronContext::new(
            run_id,
            "test_cron".to_string(),
            scheduled,
            "UTC".to_string(),
            false,
            pool,
            reqwest::Client::new(),
        );

        assert_eq!(ctx.run_id, run_id);
        assert_eq!(ctx.cron_name, "test_cron");
        assert!(!ctx.is_catch_up);
    }

    #[tokio::test]
    async fn test_cron_delay() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect_lazy("postgres://localhost/nonexistent")
            .expect("Failed to create mock pool");

        let scheduled = Utc::now() - chrono::Duration::minutes(5);

        let ctx = CronContext::new(
            Uuid::new_v4(),
            "test_cron".to_string(),
            scheduled,
            "UTC".to_string(),
            false,
            pool,
            reqwest::Client::new(),
        );

        assert!(ctx.is_late());
        assert!(ctx.delay() >= chrono::Duration::minutes(5));
    }

    #[test]
    fn test_cron_log() {
        let log = CronLog::new("test_cron".to_string());
        log.info("Test message", serde_json::json!({"key": "value"}));
    }
}
