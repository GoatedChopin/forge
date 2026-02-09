use forge::prelude::*;
use uuid::Uuid;

use crate::schema::Todo;

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateTodoInput {
    pub title: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateTodoInput {
    pub id: Uuid,
    pub title: Option<String>,
    pub completed: Option<bool>,
}

#[forge::query(public, tables = ["todos"])]
pub async fn list_todos(ctx: &QueryContext) -> Result<Vec<Todo>> {
    sqlx::query_as("SELECT * FROM todos ORDER BY created_at DESC")
        .fetch_all(ctx.db())
        .await
        .map_err(Into::into)
}

#[forge::mutation(public)]
pub async fn create_todo(ctx: &MutationContext, input: CreateTodoInput) -> Result<Todo> {
    if input.title.trim().is_empty() {
        return Err(ForgeError::Validation("Title cannot be empty".into()));
    }

    ctx.db()
        .fetch_one(
            sqlx::query_as("INSERT INTO todos (title) VALUES ($1) RETURNING *")
                .bind(input.title.trim()),
        )
        .await
        .map_err(Into::into)
}

#[forge::mutation(public)]
pub async fn update_todo(ctx: &MutationContext, input: UpdateTodoInput) -> Result<Todo> {
    let existing: Option<Todo> = ctx
        .db()
        .fetch_optional(
            sqlx::query_as("SELECT * FROM todos WHERE id = $1").bind(input.id),
        )
        .await?;

    let existing = existing.ok_or_else(|| ForgeError::NotFound("Todo not found".into()))?;

    let title = input.title.unwrap_or(existing.title);
    let completed = input.completed.unwrap_or(existing.completed);

    ctx.db()
        .fetch_one(
            sqlx::query_as("UPDATE todos SET title = $1, completed = $2 WHERE id = $3 RETURNING *")
                .bind(title)
                .bind(completed)
                .bind(input.id),
        )
        .await
        .map_err(Into::into)
}

#[forge::mutation(public)]
pub async fn delete_todo(ctx: &MutationContext, id: Uuid) -> Result<bool> {
    let result = ctx
        .db()
        .execute(sqlx::query("DELETE FROM todos WHERE id = $1").bind(id))
        .await?;

    Ok(result.rows_affected() > 0)
}
