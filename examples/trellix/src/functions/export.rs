use forge::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::schema::Task;

#[derive(Debug, Serialize, Deserialize)]
pub struct ExportProjectInput {
    pub user_id: Uuid,
    pub project_id: Uuid,
    pub format: ExportFormat,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    Csv,
    Json,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExportOutput {
    pub data: String,
    pub task_count: usize,
}

#[forge::job(
    timeout = "5m",
    priority = "low",
    retry(max_attempts = 3, backoff = "exponential"),
    idempotent
)]
pub async fn export_project(ctx: &JobContext, input: ExportProjectInput) -> Result<ExportOutput> {
    sqlx::query_as::<_, (Uuid,)>("SELECT id FROM projects WHERE id = $1 AND owner_id = $2")
        .bind(input.project_id)
        .bind(input.user_id)
        .fetch_optional(ctx.db())
        .await?
        .ok_or_else(|| ForgeError::NotFound("Project not found".into()))?;

    ctx.progress(30, "Fetching tasks")?;
    let tasks: Vec<Task> = sqlx::query_as(
        "SELECT t.*
         FROM tasks t
         JOIN projects p ON p.id = t.project_id
         WHERE t.project_id = $1 AND p.owner_id = $2
         ORDER BY t.position, t.created_at",
    )
    .bind(input.project_id)
    .bind(input.user_id)
    .fetch_all(ctx.db())
    .await?;

    ctx.progress(60, format!("Formatting {} tasks", tasks.len()))?;

    let data = match input.format {
        ExportFormat::Csv => {
            let mut lines = vec!["id,title,status,priority,assignee_id,due_date".to_string()];
            for task in &tasks {
                lines.push(format!(
                    "{},{},{},{},{},{}",
                    task.id,
                    escape_csv(&task.title),
                    task.status,
                    task.priority,
                    task.assignee_id
                        .map(|id| id.to_string())
                        .unwrap_or_default(),
                    task.due_date.map(|d| d.to_string()).unwrap_or_default(),
                ));
            }
            lines.join("\n")
        }
        ExportFormat::Json => {
            serde_json::to_string_pretty(&tasks).map_err(|e| ForgeError::Internal(e.to_string()))?
        }
    };

    ctx.progress(100, "Export complete")?;

    Ok(ExportOutput { task_count: tasks.len(), data })
}

fn escape_csv(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}
