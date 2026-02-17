use forge::prelude::*;

use crate::schema::Task;

#[forge::cron("0 9 * * *", timezone = "UTC")]
pub async fn overdue_checker(ctx: &CronContext) -> Result<()> {
    let overdue_tasks: Vec<Task> = sqlx::query_as(
        "SELECT * FROM tasks
         WHERE due_date < CURRENT_DATE
         AND status != 'done'
         ORDER BY due_date",
    )
    .fetch_all(ctx.db())
    .await?;

    if overdue_tasks.is_empty() {
        ctx.log.info(
            "No overdue tasks found",
            serde_json::json!({ "run_id": ctx.run_id }),
        );
        return Ok(());
    }

    ctx.log.warn(
        "Found overdue tasks",
        serde_json::json!({
            "count": overdue_tasks.len(),
            "task_ids": overdue_tasks.iter().map(|t| t.id).collect::<Vec<_>>(),
            "run_id": ctx.run_id,
        }),
    );

    Ok(())
}
