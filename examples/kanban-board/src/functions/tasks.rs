use forge::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::schema::{Task, TaskPriority, TaskStatus};

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateTaskInput {
    pub user_id: Uuid,
    pub project_id: Uuid,
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub priority: Option<TaskPriority>,
    pub assignee_id: Option<Uuid>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateTaskInput {
    pub user_id: Uuid,
    pub id: Uuid,
    pub title: Option<String>,
    pub description: Option<String>,
    pub status: Option<TaskStatus>,
    pub priority: Option<TaskPriority>,
    pub assignee_id: Option<Uuid>,
    pub due_date: Option<chrono::NaiveDate>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MoveTaskInput {
    pub user_id: Uuid,
    pub id: Uuid,
    pub status: TaskStatus,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListTasksInput {
    pub user_id: Uuid,
    pub project_id: Uuid,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DeleteTaskInput {
    pub user_id: Uuid,
    pub id: Uuid,
}

#[forge::query(tables = ["tasks"])]
pub async fn list_tasks(ctx: &QueryContext, input: ListTasksInput) -> Result<Vec<Task>> {
    let user_id = ctx.require_user_id()?;
    if input.user_id != user_id {
        return Err(ForgeError::Forbidden("User scope mismatch".into()));
    }

    sqlx::query_as(
        "SELECT t.*
         FROM tasks t
         JOIN projects p ON p.id = t.project_id
         WHERE t.project_id = $1 AND p.owner_id = $2
         ORDER BY t.position, t.created_at",
    )
    .bind(input.project_id)
    .bind(user_id)
    .fetch_all(ctx.db())
    .await
    .map_err(Into::into)
}

#[forge::mutation]
pub async fn create_task(ctx: &MutationContext, input: CreateTaskInput) -> Result<Task> {
    let user_id = ctx.require_user_id()?;
    if input.user_id != user_id {
        return Err(ForgeError::Forbidden("User scope mismatch".into()));
    }

    if input.title.trim().is_empty() {
        return Err(ForgeError::Validation("Task title is required".into()));
    }

    let priority = input.priority.unwrap_or(TaskPriority::Medium);

    // CTE validates project ownership and computes next position in one round-trip
    ctx.db()
        .fetch_optional(
            sqlx::query_as(
                "WITH owned AS (
                    SELECT id FROM projects WHERE id = $1 AND owner_id = $2
                ), pos AS (
                    SELECT COALESCE(MAX(position) + 1, 0) AS next_pos FROM tasks WHERE project_id = $1
                )
                INSERT INTO tasks (project_id, title, description, priority, assignee_id, position)
                SELECT $1, $3, $4, $5, $6, pos.next_pos FROM owned, pos
                RETURNING *",
            )
            .bind(input.project_id)
            .bind(user_id)
            .bind(input.title.trim())
            .bind(&input.description)
            .bind(priority)
            .bind(input.assignee_id),
        )
        .await?
        .ok_or_else(|| ForgeError::NotFound("Project not found".into()))
}

#[forge::mutation]
pub async fn update_task(ctx: &MutationContext, input: UpdateTaskInput) -> Result<Task> {
    let user_id = ctx.require_user_id()?;
    if input.user_id != user_id {
        return Err(ForgeError::Forbidden("User scope mismatch".into()));
    }

    ctx.db()
        .fetch_optional(
            sqlx::query_as(
                "UPDATE tasks t SET
                     title = COALESCE($1, t.title),
                     description = COALESCE($2, t.description),
                     status = COALESCE($3, t.status),
                     priority = COALESCE($4, t.priority),
                     assignee_id = COALESCE($5, t.assignee_id),
                     due_date = COALESCE($6, t.due_date),
                     updated_at = NOW()
                 FROM projects p
                 WHERE t.id = $7 AND p.id = t.project_id AND p.owner_id = $8
                 RETURNING t.*",
            )
            .bind(input.title.as_deref())
            .bind(input.description.as_deref())
            .bind(input.status)
            .bind(input.priority)
            .bind(input.assignee_id)
            .bind(input.due_date)
            .bind(input.id)
            .bind(user_id),
        )
        .await?
        .ok_or_else(|| ForgeError::NotFound("Task not found".into()))
}

#[forge::mutation]
pub async fn delete_task(ctx: &MutationContext, input: DeleteTaskInput) -> Result<bool> {
    let user_id = ctx.require_user_id()?;
    if input.user_id != user_id {
        return Err(ForgeError::Forbidden("User scope mismatch".into()));
    }

    let result = ctx
        .db()
        .execute(
            sqlx::query(
                "DELETE FROM tasks t
                 USING projects p
                 WHERE t.id = $1 AND p.id = t.project_id AND p.owner_id = $2",
            )
            .bind(input.id)
            .bind(user_id),
        )
        .await?;

    Ok(result.rows_affected() > 0)
}

#[forge::mutation]
pub async fn move_task(ctx: &MutationContext, input: MoveTaskInput) -> Result<Task> {
    let user_id = ctx.require_user_id()?;
    if input.user_id != user_id {
        return Err(ForgeError::Forbidden("User scope mismatch".into()));
    }

    ctx.db()
        .fetch_one(
            sqlx::query_as(
                "UPDATE tasks t SET status = $1, updated_at = NOW()
                 FROM projects p
                 WHERE t.id = $2 AND p.id = t.project_id AND p.owner_id = $3
                 RETURNING t.*",
            )
            .bind(input.status)
            .bind(input.id)
            .bind(user_id),
        )
        .await
        .map_err(Into::into)
}
