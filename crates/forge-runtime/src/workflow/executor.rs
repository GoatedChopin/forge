use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use tokio::sync::RwLock;
use uuid::Uuid;

use super::registry::WorkflowRegistry;
use super::state::{WorkflowRecord, WorkflowStepRecord};
use forge_core::CircuitBreakerClient;
use forge_core::function::WorkflowDispatch;
use forge_core::workflow::{CompensationHandler, StepStatus, WorkflowContext, WorkflowStatus};

/// Workflow execution result.
#[derive(Debug)]
pub enum WorkflowResult {
    /// Workflow completed successfully.
    Completed(serde_json::Value),
    /// Workflow is waiting for an external event.
    Waiting { event_type: String },
    /// Workflow failed.
    Failed { error: String },
    /// Workflow was compensated.
    Compensated,
}

/// Compensation state for a running workflow.
struct CompensationState {
    handlers: HashMap<String, CompensationHandler>,
    completed_steps: Vec<String>,
}

/// Executes workflows.
pub struct WorkflowExecutor {
    registry: Arc<WorkflowRegistry>,
    pool: sqlx::PgPool,
    http_client: CircuitBreakerClient,
    /// Compensation state for active workflows (run_id -> state).
    compensation_state: Arc<RwLock<HashMap<Uuid, CompensationState>>>,
}

impl WorkflowExecutor {
    /// Create a new workflow executor.
    pub fn new(
        registry: Arc<WorkflowRegistry>,
        pool: sqlx::PgPool,
        http_client: CircuitBreakerClient,
    ) -> Self {
        Self {
            registry,
            pool,
            http_client,
            compensation_state: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Start a new workflow on the active version.
    /// Returns immediately with the run_id; workflow executes in the background.
    pub async fn start<I: serde::Serialize>(
        &self,
        workflow_name: &str,
        input: I,
        owner_subject: Option<String>,
    ) -> forge_core::Result<Uuid> {
        let entry = self.registry.get_active(workflow_name).ok_or_else(|| {
            forge_core::ForgeError::NotFound(format!(
                "No active version of workflow '{}'",
                workflow_name
            ))
        })?;

        let input_value = serde_json::to_value(input)?;

        let record = WorkflowRecord::new(
            workflow_name,
            entry.info.version,
            entry.info.signature,
            input_value.clone(),
            owner_subject,
        );
        let run_id = record.id;

        // Clone entry data for background execution
        let entry_info = entry.info.clone();
        let entry_handler = entry.handler.clone();

        // Persist workflow record
        self.save_workflow(&record).await?;

        // Execute workflow in background
        let registry = self.registry.clone();
        let pool = self.pool.clone();
        let http_client = self.http_client.clone();
        let compensation_state = self.compensation_state.clone();

        tokio::spawn(async move {
            let executor = WorkflowExecutor {
                registry,
                pool,
                http_client,
                compensation_state,
            };
            let entry = super::registry::WorkflowEntry {
                info: entry_info,
                handler: entry_handler,
            };
            if let Err(e) = executor.execute_workflow(run_id, &entry, input_value).await {
                tracing::error!(
                    workflow_run_id = %run_id,
                    error = %e,
                    "Workflow execution failed"
                );
            }
        });

        Ok(run_id)
    }

    /// Execute a workflow.
    async fn execute_workflow(
        &self,
        run_id: Uuid,
        entry: &super::registry::WorkflowEntry,
        input: serde_json::Value,
    ) -> forge_core::Result<WorkflowResult> {
        // Update status to running
        self.update_workflow_status(run_id, WorkflowStatus::Running)
            .await?;

        // Create workflow context
        let mut ctx = WorkflowContext::new(
            run_id,
            entry.info.name.to_string(),
            self.pool.clone(),
            self.http_client.clone(),
        );
        ctx.set_http_timeout(entry.info.http_timeout);

        // Execute workflow with timeout
        let handler = entry.handler.clone();
        let result = tokio::time::timeout(entry.info.timeout, handler(&ctx, input)).await;

        // Capture compensation state after execution
        let compensation_state = CompensationState {
            handlers: ctx.compensation_handlers(),
            completed_steps: ctx.completed_steps_reversed().into_iter().rev().collect(),
        };
        self.compensation_state
            .write()
            .await
            .insert(run_id, compensation_state);

        match result {
            Ok(Ok(output)) => {
                // Mark as completed, clean up compensation state
                self.complete_workflow(run_id, output.clone()).await?;
                self.compensation_state.write().await.remove(&run_id);
                Ok(WorkflowResult::Completed(output))
            }
            Ok(Err(e)) => {
                // Check if this is a suspension (not a real failure)
                if matches!(e, forge_core::ForgeError::WorkflowSuspended) {
                    // Workflow suspended itself (sleep or wait_for_event)
                    // Status already set to 'waiting' by ctx.sleep() - don't mark as failed
                    return Ok(WorkflowResult::Waiting {
                        event_type: "timer".to_string(),
                    });
                }
                // Mark as failed - compensation can be triggered via cancel
                self.fail_workflow(run_id, &e.to_string()).await?;
                Ok(WorkflowResult::Failed {
                    error: e.to_string(),
                })
            }
            Err(_) => {
                // Timeout
                self.fail_workflow(run_id, "Workflow timed out").await?;
                Ok(WorkflowResult::Failed {
                    error: "Workflow timed out".to_string(),
                })
            }
        }
    }

    /// Execute a resumed workflow with step states loaded from the database.
    async fn execute_workflow_resumed(
        &self,
        run_id: Uuid,
        entry: &super::registry::WorkflowEntry,
        input: serde_json::Value,
        started_at: chrono::DateTime<chrono::Utc>,
        from_sleep: bool,
    ) -> forge_core::Result<WorkflowResult> {
        // Update status to running
        self.update_workflow_status(run_id, WorkflowStatus::Running)
            .await?;

        // Load step states from database
        let step_records = self.get_workflow_steps(run_id).await?;
        let mut step_states = std::collections::HashMap::new();
        for step in step_records {
            let status = step.status;
            step_states.insert(
                step.step_name.clone(),
                forge_core::workflow::StepState {
                    name: step.step_name,
                    status,
                    result: step.result,
                    error: step.error,
                    started_at: step.started_at,
                    completed_at: step.completed_at,
                },
            );
        }

        // Create resumed workflow context with step states
        let mut ctx = WorkflowContext::resumed(
            run_id,
            entry.info.name.to_string(),
            started_at,
            self.pool.clone(),
            self.http_client.clone(),
        )
        .with_step_states(step_states);
        ctx.set_http_timeout(entry.info.http_timeout);

        // If resuming from a sleep timer, mark the context so sleep() returns immediately
        if from_sleep {
            ctx = ctx.with_resumed_from_sleep();
        }

        // Execute workflow with timeout
        let handler = entry.handler.clone();
        let result = tokio::time::timeout(entry.info.timeout, handler(&ctx, input)).await;

        // Capture compensation state after execution
        let compensation_state = CompensationState {
            handlers: ctx.compensation_handlers(),
            completed_steps: ctx.completed_steps_reversed().into_iter().rev().collect(),
        };
        self.compensation_state
            .write()
            .await
            .insert(run_id, compensation_state);

        match result {
            Ok(Ok(output)) => {
                // Mark as completed, clean up compensation state
                self.complete_workflow(run_id, output.clone()).await?;
                self.compensation_state.write().await.remove(&run_id);
                Ok(WorkflowResult::Completed(output))
            }
            Ok(Err(e)) => {
                // Check if this is a suspension (not a real failure)
                if matches!(e, forge_core::ForgeError::WorkflowSuspended) {
                    // Workflow suspended itself (sleep or wait_for_event)
                    // Status already set to 'waiting' by ctx.sleep() - don't mark as failed
                    return Ok(WorkflowResult::Waiting {
                        event_type: "timer".to_string(),
                    });
                }
                // Mark as failed - compensation can be triggered via cancel
                self.fail_workflow(run_id, &e.to_string()).await?;
                Ok(WorkflowResult::Failed {
                    error: e.to_string(),
                })
            }
            Err(_) => {
                // Timeout
                self.fail_workflow(run_id, "Workflow timed out").await?;
                Ok(WorkflowResult::Failed {
                    error: "Workflow timed out".to_string(),
                })
            }
        }
    }

    /// Resume a workflow from where it left off.
    pub async fn resume(&self, run_id: Uuid) -> forge_core::Result<WorkflowResult> {
        self.resume_internal(run_id, false).await
    }

    /// Resume a workflow after a sleep timer expired.
    pub async fn resume_from_sleep(&self, run_id: Uuid) -> forge_core::Result<WorkflowResult> {
        self.resume_internal(run_id, true).await
    }

    /// Internal resume logic with version+signature validation.
    async fn resume_internal(
        &self,
        run_id: Uuid,
        from_sleep: bool,
    ) -> forge_core::Result<WorkflowResult> {
        let record = self.get_workflow(run_id).await?;

        // Check if workflow is resumable
        match record.status {
            WorkflowStatus::Running | WorkflowStatus::Waiting => {
                // Can resume
            }
            status if status.is_terminal() || status.is_blocked() => {
                return Err(forge_core::ForgeError::Validation(format!(
                    "Cannot resume workflow in {} state",
                    status.as_str()
                )));
            }
            _ => {}
        }

        // Validate version+signature against registry
        match self.registry.validate_resume(
            &record.workflow_name,
            &record.workflow_version,
            &record.workflow_signature,
        ) {
            Ok(entry) => {
                self.execute_workflow_resumed(
                    run_id,
                    entry,
                    record.input,
                    record.started_at,
                    from_sleep,
                )
                .await
            }
            Err(reason) => {
                // Mark run as blocked
                let status = reason.to_status();
                let description = reason.description();
                self.block_workflow(run_id, status, &description).await?;
                tracing::warn!(
                    workflow_run_id = %run_id,
                    workflow_name = %record.workflow_name,
                    workflow_version = %record.workflow_version,
                    reason = %description,
                    "Workflow run blocked"
                );
                Ok(WorkflowResult::Failed { error: description })
            }
        }
    }

    /// Get workflow status.
    pub async fn status(&self, run_id: Uuid) -> forge_core::Result<WorkflowRecord> {
        self.get_workflow(run_id).await
    }

    /// Cancel a workflow and run compensation.
    ///
    /// Compensation follows the saga pattern: steps are undone in reverse order
    /// of their completion. This ensures that dependencies are respected. For
    /// example, if step A created a resource that step B modified, we must
    /// undo B's modification before deleting A's resource.
    ///
    /// Compensation handlers receive the original step result, allowing them
    /// to know exactly what to undo (e.g., refund the specific payment ID).
    pub async fn cancel(&self, run_id: Uuid) -> forge_core::Result<()> {
        self.update_workflow_status(run_id, WorkflowStatus::Compensating)
            .await?;

        // Get compensation state
        let state = self.compensation_state.write().await.remove(&run_id);

        if let Some(state) = state {
            // Get completed steps with results from database
            let steps = self.get_workflow_steps(run_id).await?;

            // Run compensation in reverse order of completion.
            // This is critical for maintaining consistency: if step B depends on
            // step A's output, we must undo B before A. The completed_steps list
            // preserves insertion order, so reversing gives us the correct undo order.
            for step_name in state.completed_steps.iter().rev() {
                if let Some(handler) = state.handlers.get(step_name) {
                    // Find the step result
                    let step_result = steps
                        .iter()
                        .find(|s| &s.step_name == step_name)
                        .and_then(|s| s.result.clone())
                        .unwrap_or(serde_json::Value::Null);

                    // Run compensation handler
                    match handler(step_result).await {
                        Ok(()) => {
                            tracing::info!(
                                workflow_run_id = %run_id,
                                step = %step_name,
                                "Compensation completed"
                            );
                            self.update_step_status(run_id, step_name, StepStatus::Compensated)
                                .await?;
                        }
                        Err(e) => {
                            tracing::error!(
                                workflow_run_id = %run_id,
                                step = %step_name,
                                error = %e,
                                "Compensation failed"
                            );
                            // Continue with other compensations even if one fails
                        }
                    }
                } else {
                    // No handler, just mark as compensated
                    self.update_step_status(run_id, step_name, StepStatus::Compensated)
                        .await?;
                }
            }
        } else {
            // Fail closed: never report compensated when handlers are unavailable.
            let msg =
                "Compensation handlers unavailable (likely restart); refusing to mark compensated";
            tracing::error!(workflow_run_id = %run_id, "{msg}");
            self.fail_workflow(run_id, msg).await?;
            return Err(forge_core::ForgeError::InvalidState(msg.to_string()));
        }

        self.update_workflow_status(run_id, WorkflowStatus::Compensated)
            .await?;

        Ok(())
    }

    /// Get workflow steps from database.
    async fn get_workflow_steps(
        &self,
        workflow_run_id: Uuid,
    ) -> forge_core::Result<Vec<WorkflowStepRecord>> {
        let rows = sqlx::query!(
            r#"
            SELECT id, workflow_run_id, step_name, status, result, error, started_at, completed_at
            FROM forge_workflow_steps
            WHERE workflow_run_id = $1
            ORDER BY started_at ASC
            "#,
            workflow_run_id,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| forge_core::ForgeError::Database(e.to_string()))?;

        rows.into_iter()
            .map(|row| {
                let status = row.status.parse().map_err(|e| {
                    forge_core::ForgeError::Database(format!(
                        "Invalid step status '{}': {}",
                        row.status, e
                    ))
                })?;
                Ok(WorkflowStepRecord {
                    id: row.id,
                    workflow_run_id: row.workflow_run_id,
                    step_name: row.step_name,
                    status,
                    result: row.result,
                    error: row.error,
                    started_at: row.started_at,
                    completed_at: row.completed_at,
                })
            })
            .collect()
    }

    /// Update step status.
    async fn update_step_status(
        &self,
        workflow_run_id: Uuid,
        step_name: &str,
        status: StepStatus,
    ) -> forge_core::Result<()> {
        sqlx::query!(
            r#"
            UPDATE forge_workflow_steps
            SET status = $3
            WHERE workflow_run_id = $1 AND step_name = $2
            "#,
            workflow_run_id,
            step_name,
            status.as_str(),
        )
        .execute(&self.pool)
        .await
        .map_err(|e| forge_core::ForgeError::Database(e.to_string()))?;

        Ok(())
    }

    /// Save workflow record to database.
    async fn save_workflow(&self, record: &WorkflowRecord) -> forge_core::Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO forge_workflow_runs (
                id, workflow_name, workflow_version, workflow_signature,
                owner_subject, input, status, current_step,
                step_results, started_at, trace_id
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            "#,
            record.id,
            &record.workflow_name,
            &record.workflow_version,
            &record.workflow_signature,
            record.owner_subject as _,
            record.input as _,
            record.status.as_str(),
            record.current_step as _,
            record.step_results as _,
            record.started_at,
            record.trace_id.as_deref(),
        )
        .execute(&self.pool)
        .await
        .map_err(|e| forge_core::ForgeError::Database(e.to_string()))?;

        Ok(())
    }

    /// Get workflow record from database.
    async fn get_workflow(&self, run_id: Uuid) -> forge_core::Result<WorkflowRecord> {
        let row = sqlx::query!(
            r#"
            SELECT id, workflow_name, workflow_version, workflow_signature,
                   owner_subject, input, output, status, blocking_reason,
                   resolution_reason, current_step, step_results, started_at,
                   completed_at, error, trace_id
            FROM forge_workflow_runs
            WHERE id = $1
            "#,
            run_id,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| forge_core::ForgeError::Database(e.to_string()))?;

        let row = row.ok_or_else(|| {
            forge_core::ForgeError::NotFound(format!("Workflow run {} not found", run_id))
        })?;

        let status = row.status.parse().map_err(|e| {
            forge_core::ForgeError::Database(format!(
                "Invalid workflow status '{}': {}",
                row.status, e
            ))
        })?;
        Ok(WorkflowRecord {
            id: row.id,
            workflow_name: row.workflow_name,
            workflow_version: row.workflow_version,
            workflow_signature: row.workflow_signature,
            owner_subject: row.owner_subject,
            input: row.input,
            output: row.output,
            status,
            blocking_reason: row.blocking_reason,
            resolution_reason: row.resolution_reason,
            current_step: row.current_step,
            step_results: row.step_results.unwrap_or_default(),
            started_at: row.started_at,
            completed_at: row.completed_at,
            error: row.error,
            trace_id: row.trace_id,
        })
    }

    /// Get valid source states for a given target workflow status.
    /// Enforces a state machine to prevent invalid transitions.
    fn valid_source_states(target: &WorkflowStatus) -> &'static [&'static str] {
        match target {
            WorkflowStatus::Running => &["created", "waiting", "running"],
            WorkflowStatus::Waiting => &["running"],
            WorkflowStatus::Completed => &["running"],
            WorkflowStatus::Compensating => &["running", "waiting", "failed"],
            WorkflowStatus::Compensated => &["compensating"],
            WorkflowStatus::Failed => &["running", "waiting", "compensating"],
            WorkflowStatus::BlockedMissingVersion
            | WorkflowStatus::BlockedSignatureMismatch
            | WorkflowStatus::BlockedMissingHandler => &["waiting", "running", "created"],
            WorkflowStatus::RetiredUnresumable | WorkflowStatus::CancelledByOperator => &[
                "created",
                "running",
                "waiting",
                "failed",
                "blocked_missing_version",
                "blocked_signature_mismatch",
                "blocked_missing_handler",
            ],
            WorkflowStatus::Created => &[], // entry state only
        }
    }

    /// Update workflow status with state transition validation.
    ///
    /// Validates the transition in Rust before issuing the update to keep
    /// SQL queries compile-time checked while still enforcing the state machine.
    async fn update_workflow_status(
        &self,
        run_id: Uuid,
        status: WorkflowStatus,
    ) -> forge_core::Result<()> {
        let valid_from = Self::valid_source_states(&status);

        if valid_from.is_empty() {
            // Entry-only state (Created) – unconditional update.
            sqlx::query!(
                "UPDATE forge_workflow_runs SET status = $1 WHERE id = $2",
                status.as_str(),
                run_id,
            )
            .execute(&self.pool)
            .await
            .map_err(|e| forge_core::ForgeError::Database(e.to_string()))?;
        } else {
            // Fetch current status, then atomically update only if the row
            // still has the expected status, closing the TOCTOU window.
            let current = sqlx::query_scalar!(
                "SELECT status FROM forge_workflow_runs WHERE id = $1",
                run_id,
            )
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| forge_core::ForgeError::Database(e.to_string()))?;

            let current_status = match current {
                Some(ref s) if valid_from.contains(&s.as_str()) => s.clone(),
                Some(_) => {
                    return Err(forge_core::ForgeError::InvalidState(format!(
                        "Cannot transition workflow {} to {:?}: invalid current state",
                        run_id, status
                    )));
                }
                None => {
                    return Err(forge_core::ForgeError::NotFound(format!(
                        "Workflow run {} not found",
                        run_id
                    )));
                }
            };

            // Atomic check-and-set: WHERE includes the expected current status
            // so a concurrent change causes 0 rows affected instead of a silent
            // state machine violation.
            let result = sqlx::query!(
                "UPDATE forge_workflow_runs SET status = $1 WHERE id = $2 AND status = $3",
                status.as_str(),
                run_id,
                current_status,
            )
            .execute(&self.pool)
            .await
            .map_err(|e| forge_core::ForgeError::Database(e.to_string()))?;

            if result.rows_affected() == 0 {
                return Err(forge_core::ForgeError::InvalidState(format!(
                    "Cannot transition workflow {} to {:?}: state changed concurrently",
                    run_id, status
                )));
            }
        }

        Ok(())
    }

    /// Mark workflow as completed (only from 'running' state).
    async fn complete_workflow(
        &self,
        run_id: Uuid,
        output: serde_json::Value,
    ) -> forge_core::Result<()> {
        let result = sqlx::query!(
            "UPDATE forge_workflow_runs SET status = 'completed', output = $1, completed_at = NOW() WHERE id = $2 AND status = 'running'",
            output as _,
            run_id,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| forge_core::ForgeError::Database(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(forge_core::ForgeError::InvalidState(format!(
                "Cannot complete workflow {}: not in 'running' state",
                run_id
            )));
        }

        Ok(())
    }

    /// Mark workflow as failed (only from valid states).
    async fn fail_workflow(&self, run_id: Uuid, error: &str) -> forge_core::Result<()> {
        let result = sqlx::query!(
            "UPDATE forge_workflow_runs SET status = 'failed', error = $1, completed_at = NOW() WHERE id = $2 AND status IN ('running', 'waiting', 'compensating')",
            error,
            run_id,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| forge_core::ForgeError::Database(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(forge_core::ForgeError::InvalidState(format!(
                "Cannot fail workflow {}: not in a valid state for failure",
                run_id
            )));
        }

        Ok(())
    }

    /// Block a workflow run due to version/signature incompatibility.
    async fn block_workflow(
        &self,
        run_id: Uuid,
        status: WorkflowStatus,
        reason: &str,
    ) -> forge_core::Result<()> {
        sqlx::query!(
            "UPDATE forge_workflow_runs SET status = $1, blocking_reason = $2 WHERE id = $3 AND status IN ('waiting', 'running', 'created')",
            status.as_str(),
            reason,
            run_id,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| forge_core::ForgeError::Database(e.to_string()))?;

        Ok(())
    }

    /// Cancel a workflow run by operator decision. Terminal action that preserves audit trail.
    pub async fn cancel_by_operator(&self, run_id: Uuid, reason: &str) -> forge_core::Result<()> {
        let result = sqlx::query!(
            "UPDATE forge_workflow_runs SET status = 'cancelled_by_operator', resolution_reason = $1, completed_at = NOW() WHERE id = $2 AND status NOT IN ('completed', 'compensated', 'cancelled_by_operator', 'retired_unresumable')",
            reason,
            run_id,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| forge_core::ForgeError::Database(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(forge_core::ForgeError::InvalidState(format!(
                "Cannot cancel workflow {}: already in a terminal state",
                run_id
            )));
        }

        Ok(())
    }

    /// Retire a workflow run as unresumable. Terminal action for blocked runs.
    pub async fn retire_unresumable(&self, run_id: Uuid, reason: &str) -> forge_core::Result<()> {
        let result = sqlx::query!(
            "UPDATE forge_workflow_runs SET status = 'retired_unresumable', resolution_reason = $1, completed_at = NOW() WHERE id = $2 AND status NOT IN ('completed', 'compensated', 'cancelled_by_operator', 'retired_unresumable')",
            reason,
            run_id,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| forge_core::ForgeError::Database(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(forge_core::ForgeError::InvalidState(format!(
                "Cannot retire workflow {}: already in a terminal state",
                run_id
            )));
        }

        Ok(())
    }

    /// Save step record.
    pub async fn save_step(&self, step: &WorkflowStepRecord) -> forge_core::Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO forge_workflow_steps (
                id, workflow_run_id, step_name, status, result, error, started_at, completed_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (workflow_run_id, step_name) DO UPDATE SET
                status = EXCLUDED.status,
                result = EXCLUDED.result,
                error = EXCLUDED.error,
                started_at = COALESCE(forge_workflow_steps.started_at, EXCLUDED.started_at),
                completed_at = EXCLUDED.completed_at
            "#,
            step.id,
            step.workflow_run_id,
            &step.step_name,
            step.status.as_str(),
            step.result as _,
            step.error as _,
            step.started_at,
            step.completed_at,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| forge_core::ForgeError::Database(e.to_string()))?;

        Ok(())
    }

    /// Start a workflow by its registered name with JSON input.
    pub async fn start_by_name(
        &self,
        workflow_name: &str,
        input: serde_json::Value,
        owner_subject: Option<String>,
    ) -> forge_core::Result<Uuid> {
        self.start(workflow_name, input, owner_subject).await
    }
}

impl WorkflowDispatch for WorkflowExecutor {
    fn get_info(&self, workflow_name: &str) -> Option<forge_core::workflow::WorkflowInfo> {
        self.registry
            .get_active(workflow_name)
            .map(|e| e.info.clone())
    }

    fn start_by_name(
        &self,
        workflow_name: &str,
        input: serde_json::Value,
        owner_subject: Option<String>,
    ) -> Pin<Box<dyn Future<Output = forge_core::Result<Uuid>> + Send + '_>> {
        let workflow_name = workflow_name.to_string();
        Box::pin(async move {
            self.start_by_name(&workflow_name, input, owner_subject)
                .await
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_workflow_result_types() {
        let completed = WorkflowResult::Completed(serde_json::json!({}));
        let _waiting = WorkflowResult::Waiting {
            event_type: "approval".to_string(),
        };
        let _failed = WorkflowResult::Failed {
            error: "test".to_string(),
        };
        let _compensated = WorkflowResult::Compensated;

        // Just ensure they can be created
        match completed {
            WorkflowResult::Completed(_) => {}
            _ => panic!("Expected Completed"),
        }
    }
}
