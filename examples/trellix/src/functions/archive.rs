use std::time::Duration;

use chrono::{DateTime, Utc};
use forge::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::schema::Task;

#[derive(Debug, Serialize, Deserialize)]
pub struct ScheduleDeletionInput {
    pub user_id: Uuid,
    pub project_id: Uuid,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ScheduleDeletionOutput {
    pub tasks_deleted: usize,
}

#[forge::workflow(version = 1, timeout = "8d")]
pub async fn schedule_project_archive(
    ctx: &WorkflowContext,
    input: ScheduleDeletionInput,
) -> Result<ScheduleDeletionOutput> {
    let project_id = input.project_id;
    let project_owner = input.user_id;

    let db = ctx.db().clone();
    ctx.step("schedule_archive", move || {
        let db = db.clone();
        async move {
            let existing: Option<(bool, Option<DateTime<Utc>>)> = sqlx::query_as(
                "SELECT archived, archive_delete_at
                 FROM projects
                 WHERE id = $1 AND owner_id = $2",
            )
            .bind(project_id)
            .bind(project_owner)
            .fetch_optional(&db)
            .await?;

            let Some((archived, archive_delete_at)) = existing else {
                return Err(ForgeError::NotFound("Project not found".into()));
            };

            if archived && archive_delete_at.is_some() {
                return Err(ForgeError::Validation(
                    "Project deletion is already scheduled".into(),
                ));
            }

            let (delete_at,): (DateTime<Utc>,) = sqlx::query_as(
                "UPDATE projects
                 SET archived = true,
                     archive_started_at = NOW(),
                     archive_delete_at = NOW() + INTERVAL '7 days'
                 WHERE id = $1 AND owner_id = $2
                 RETURNING archive_delete_at",
            )
            .bind(project_id)
            .bind(project_owner)
            .fetch_one(&db)
            .await?;

            Ok(serde_json::json!({
                "archive_delete_at": delete_at.to_rfc3339(),
            }))
        }
    })
    .run()
    .await?;

    let db = ctx.db().clone();
    ctx.step("export_data", move || {
        let db = db.clone();
        async move {
            let tasks: Vec<Task> = sqlx::query_as(
                "SELECT t.*
                 FROM tasks t
                 JOIN projects p ON p.id = t.project_id
                 WHERE t.project_id = $1 AND p.owner_id = $2
                 ORDER BY t.position",
            )
            .bind(project_id)
            .bind(project_owner)
            .fetch_all(&db)
            .await?;

            let json = serde_json::to_string_pretty(&tasks)
                .map_err(|e| ForgeError::Internal(e.to_string()))?;

            Ok(serde_json::json!({
                "task_count": tasks.len(),
                "data": json,
            }))
        }
    })
    .run()
    .await?;

    ctx.sleep(Duration::from_secs(7 * 24 * 60 * 60)).await?;

    let db = ctx.db().clone();
    let delete_result = ctx
        .step("delete_tasks", move || {
            let db = db.clone();
            async move {
                let result = sqlx::query(
                    "DELETE FROM tasks t
                     USING projects p
                     WHERE t.project_id = $1
                       AND p.id = t.project_id
                       AND p.owner_id = $2
                       AND p.archived = true
                       AND p.archive_delete_at IS NOT NULL
                       AND p.archive_delete_at <= NOW()",
                )
                .bind(project_id)
                .bind(project_owner)
                .execute(&db)
                .await?;
                Ok(serde_json::json!({ "deleted": result.rows_affected() }))
            }
        })
        .run()
        .await?;

    let db = ctx.db().clone();
    ctx.step("clear_schedule", move || {
        let db = db.clone();
        async move {
            sqlx::query(
                "UPDATE projects
                 SET archive_started_at = NULL,
                     archive_delete_at = NULL
                 WHERE id = $1
                   AND owner_id = $2
                   AND archived = true
                   AND archive_delete_at IS NOT NULL
                   AND archive_delete_at <= NOW()",
            )
            .bind(project_id)
            .bind(project_owner)
            .execute(&db)
            .await?;
            Ok(serde_json::json!({ "cleared": true }))
        }
    })
    .run()
    .await?;

    let tasks_deleted = delete_result
        .and_then(|v| v.get("deleted")?.as_u64())
        .unwrap_or(0) as usize;

    Ok(ScheduleDeletionOutput { tasks_deleted })
}
