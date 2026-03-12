use forge::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::schema::{Project, Task, TaskPriority, TaskStatus};

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

    sqlx::query_as!(
        Project,
        "SELECT * FROM projects WHERE owner_id = $1 ORDER BY created_at DESC",
        user_id
    )
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

    let project = sqlx::query_as!(
        Project,
        "SELECT * FROM projects WHERE id = $1 AND owner_id = $2",
        input.id,
        user_id
    )
    .fetch_optional(ctx.db())
    .await?
    .ok_or_else(|| ForgeError::NotFound("Project not found".into()))?;

    let tasks = sqlx::query_as!(
        Task,
        r#"SELECT id, project_id, title, description,
                  status as "status: TaskStatus",
                  priority as "priority: TaskPriority",
                  assignee_id, due_date, position, created_at, updated_at
           FROM tasks WHERE project_id = $1 ORDER BY position, created_at"#,
        input.id
    )
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

    let name = input.name.trim().to_string();
    let mut conn = ctx.conn().await?;

    sqlx::query_as!(
        Project,
        "INSERT INTO projects (name, description, owner_id) VALUES ($1, $2, $3) RETURNING *",
        name,
        &input.description,
        user_id
    )
    .fetch_one(&mut *conn)
    .await
    .map_err(Into::into)
}

#[forge::mutation]
pub async fn update_project(ctx: &MutationContext, input: UpdateProjectInput) -> Result<Project> {
    let user_id = ctx.require_user_id()?;
    if input.user_id != user_id {
        return Err(ForgeError::Forbidden("User scope mismatch".into()));
    }

    let name = input.name.as_deref();
    let description = input.description.as_deref();
    let mut conn = ctx.conn().await?;

    sqlx::query_as!(
        Project,
        "UPDATE projects
         SET name = COALESCE($1, name), description = COALESCE($2, description)
         WHERE id = $3 AND owner_id = $4
         RETURNING *",
        name,
        description,
        input.id,
        user_id
    )
    .fetch_optional(&mut *conn)
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

    let mut conn = ctx.conn().await?;

    sqlx::query_as!(
        Project,
        "UPDATE projects
         SET archived = false,
             archive_started_at = NULL,
             archive_delete_at = NULL
         WHERE id = $1 AND owner_id = $2 AND archived = true
         RETURNING *",
        input.id,
        user_id
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or_else(|| ForgeError::NotFound("Project not found or not archived".into()))
}
