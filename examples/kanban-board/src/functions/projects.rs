use forge::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::schema::{Project, Task};

#[derive(Debug, Serialize, Deserialize)]
pub struct ListProjectsInput {
    pub user_id: Uuid,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetProjectInput {
    pub id: Uuid,
    pub user_id: Uuid,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateProjectInput {
    pub user_id: Uuid,
    pub name: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateProjectInput {
    pub user_id: Uuid,
    pub id: Uuid,
    pub name: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UnarchiveProjectInput {
    pub user_id: Uuid,
    pub id: Uuid,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProjectWithTasks {
    pub project: Project,
    pub tasks: Vec<Task>,
}

#[forge::query(tables = ["projects"])]
pub async fn list_projects(ctx: &QueryContext, input: ListProjectsInput) -> Result<Vec<Project>> {
    let user_id = ctx.require_user_id()?;
    if input.user_id != user_id {
        return Err(ForgeError::Forbidden("User scope mismatch".into()));
    }

    sqlx::query_as("SELECT * FROM projects WHERE owner_id = $1 ORDER BY created_at DESC")
        .bind(user_id)
        .fetch_all(ctx.db())
        .await
        .map_err(Into::into)
}

#[forge::query(tables = ["projects", "tasks"])]
pub async fn get_project(ctx: &QueryContext, input: GetProjectInput) -> Result<ProjectWithTasks> {
    let user_id = ctx.require_user_id()?;
    if input.user_id != user_id {
        return Err(ForgeError::Forbidden("User scope mismatch".into()));
    }

    let project: Project = sqlx::query_as("SELECT * FROM projects WHERE id = $1 AND owner_id = $2")
        .bind(input.id)
        .bind(user_id)
        .fetch_optional(ctx.db())
        .await?
        .ok_or_else(|| ForgeError::NotFound("Project not found".into()))?;

    let tasks: Vec<Task> =
        sqlx::query_as("SELECT * FROM tasks WHERE project_id = $1 ORDER BY position, created_at")
            .bind(input.id)
            .fetch_all(ctx.db())
            .await?;

    Ok(ProjectWithTasks { project, tasks })
}

#[forge::mutation]
pub async fn create_project(ctx: &MutationContext, input: CreateProjectInput) -> Result<Project> {
    let user_id = ctx.require_user_id()?;
    if input.user_id != user_id {
        return Err(ForgeError::Forbidden("User scope mismatch".into()));
    }

    if input.name.trim().is_empty() {
        return Err(ForgeError::Validation("Project name is required".into()));
    }

    ctx.db()
        .fetch_one(
            sqlx::query_as(
                "INSERT INTO projects (name, description, owner_id) VALUES ($1, $2, $3) RETURNING *",
            )
            .bind(input.name.trim())
            .bind(&input.description)
            .bind(user_id),
        )
        .await
        .map_err(Into::into)
}

#[forge::mutation]
pub async fn update_project(ctx: &MutationContext, input: UpdateProjectInput) -> Result<Project> {
    let user_id = ctx.require_user_id()?;
    if input.user_id != user_id {
        return Err(ForgeError::Forbidden("User scope mismatch".into()));
    }

    ctx.db()
        .fetch_optional(
            sqlx::query_as(
                "UPDATE projects
                 SET name = COALESCE($1, name), description = COALESCE($2, description)
                 WHERE id = $3 AND owner_id = $4
                 RETURNING *",
            )
            .bind(input.name.as_deref())
            .bind(input.description.as_deref())
            .bind(input.id)
            .bind(user_id),
        )
        .await?
        .ok_or_else(|| ForgeError::NotFound("Project not found".into()))
}

#[forge::mutation]
pub async fn unarchive_project(
    ctx: &MutationContext,
    input: UnarchiveProjectInput,
) -> Result<Project> {
    let user_id = ctx.require_user_id()?;
    if input.user_id != user_id {
        return Err(ForgeError::Forbidden("User scope mismatch".into()));
    }

    ctx.db()
        .fetch_optional(
            sqlx::query_as(
                "UPDATE projects
                 SET archived = false,
                     archive_started_at = NULL,
                     archive_delete_at = NULL
                 WHERE id = $1 AND owner_id = $2 AND archived = true
                 RETURNING *",
            )
            .bind(input.id)
            .bind(user_id),
        )
        .await?
        .ok_or_else(|| ForgeError::NotFound("Project not found or not archived".into()))
}
