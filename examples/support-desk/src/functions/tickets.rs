use forge::forge_core::DbConn;
use forge::prelude::*;

use crate::schema::{
    AddTicketNoteInput, CreateSupportTicketInput, SetTicketPriorityInput, SetTicketStatusInput,
    SupportTicket, TicketPriority,
};

fn normalized_non_empty(label: &str, raw: &str, max: usize) -> Result<String> {
    let value = raw.trim();
    if value.is_empty() {
        return Err(ForgeError::Validation(format!("{} cannot be empty", label)));
    }
    if value.len() > max {
        return Err(ForgeError::Validation(format!(
            "{} must be {} characters or fewer",
            label, max
        )));
    }
    Ok(value.to_string())
}

pub(crate) async fn list_tickets(db: DbConn<'_>) -> Result<Vec<SupportTicket>> {
    db.fetch_all(sqlx::query_as(
        "SELECT *
         FROM support_tickets
         ORDER BY
           CASE status
             WHEN 'new' THEN 0
             WHEN 'working' THEN 1
             ELSE 2
           END,
           updated_at DESC",
    ))
    .await
    .map_err(Into::into)
}

pub(crate) async fn create_ticket(
    db: DbConn<'_>,
    input: CreateSupportTicketInput,
) -> Result<SupportTicket> {
    let customer_name = normalized_non_empty("Customer name", &input.customer_name, 80)?;
    let title = normalized_non_empty("Title", &input.title, 120)?;
    let details = normalized_non_empty("Details", &input.details, 1000)?;
    let priority = input.priority.unwrap_or(TicketPriority::Normal);

    db.fetch_one(
        sqlx::query_as(
            "INSERT INTO support_tickets (customer_name, title, details, priority)
         VALUES ($1, $2, $3, $4)
         RETURNING *",
        )
        .bind(customer_name)
        .bind(title)
        .bind(details)
        .bind(priority),
    )
    .await
    .map_err(Into::into)
}

pub(crate) async fn set_status(
    db: DbConn<'_>,
    input: SetTicketStatusInput,
) -> Result<SupportTicket> {
    db.fetch_optional(
        sqlx::query_as(
            "UPDATE support_tickets
         SET status = $1, updated_at = NOW()
         WHERE id = $2
         RETURNING *",
        )
        .bind(input.status)
        .bind(input.id),
    )
    .await?
    .ok_or_else(|| ForgeError::NotFound("Ticket not found".into()))
}

pub(crate) async fn set_priority(
    db: DbConn<'_>,
    input: SetTicketPriorityInput,
) -> Result<SupportTicket> {
    db.fetch_optional(
        sqlx::query_as(
            "UPDATE support_tickets
         SET priority = $1, updated_at = NOW()
         WHERE id = $2
         RETURNING *",
        )
        .bind(input.priority)
        .bind(input.id),
    )
    .await?
    .ok_or_else(|| ForgeError::NotFound("Ticket not found".into()))
}

pub(crate) async fn add_note(db: DbConn<'_>, input: AddTicketNoteInput) -> Result<SupportTicket> {
    let note = normalized_non_empty("Note", &input.note, 300)?;

    db.fetch_optional(
        sqlx::query_as(
            "UPDATE support_tickets
         SET last_note = $1, updated_at = NOW()
         WHERE id = $2
         RETURNING *",
        )
        .bind(note)
        .bind(input.id),
    )
    .await?
    .ok_or_else(|| ForgeError::NotFound("Ticket not found".into()))
}

#[forge::query(public, tables = ["support_tickets"])]
pub async fn list_support_tickets(ctx: &QueryContext) -> Result<Vec<SupportTicket>> {
    list_tickets(DbConn::Pool(ctx.db())).await
}

#[forge::mutation(public)]
pub async fn create_support_ticket(
    ctx: &MutationContext,
    input: CreateSupportTicketInput,
) -> Result<SupportTicket> {
    create_ticket(ctx.db(), input).await
}

#[forge::mutation(public)]
pub async fn set_ticket_status(
    ctx: &MutationContext,
    input: SetTicketStatusInput,
) -> Result<SupportTicket> {
    set_status(ctx.db(), input).await
}

#[forge::mutation(public)]
pub async fn set_ticket_priority(
    ctx: &MutationContext,
    input: SetTicketPriorityInput,
) -> Result<SupportTicket> {
    set_priority(ctx.db(), input).await
}

#[forge::mutation(public)]
pub async fn add_ticket_note(
    ctx: &MutationContext,
    input: AddTicketNoteInput,
) -> Result<SupportTicket> {
    add_note(ctx.db(), input).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge::testing::{IsolatedTestDb, TestDatabase};
    use std::path::Path;

    use crate::schema::TicketStatus;

    async fn setup_db(test_name: &str) -> IsolatedTestDb {
        let base = TestDatabase::from_env()
            .await
            .expect("test database");
        let db = base.isolated(test_name).await.expect("isolated db");
        db.run_sql(&forge::get_internal_sql())
            .await
            .expect("internal sql loaded");
        db.migrate(Path::new("migrations"))
            .await
            .expect("migrations applied");
        db
    }

    #[tokio::test]
    async fn test_create_and_list_tickets() {
        let db = setup_db("create_and_list_tickets").await;
        let pool = db.pool().clone();

        let created = create_ticket(
            DbConn::Pool(&pool),
            CreateSupportTicketInput {
                customer_name: "Ari".to_string(),
                title: "Mobile checkout fails".to_string(),
                details: "Card tokenization fails on Safari".to_string(),
                priority: Some(TicketPriority::High),
            },
        )
        .await
        .expect("ticket created");

        assert_eq!(created.status, TicketStatus::New);

        create_ticket(
            DbConn::Pool(&pool),
            CreateSupportTicketInput {
                customer_name: "Noa".to_string(),
                title: "Billing portal question".to_string(),
                details: "Needs VAT invoice details".to_string(),
                priority: None,
            },
        )
        .await
        .expect("second ticket created");

        let tickets = list_tickets(DbConn::Pool(&pool))
            .await
            .expect("tickets listed");
        assert_eq!(tickets.len(), 2);

        db.cleanup().await.expect("cleanup");
    }

    #[tokio::test]
    async fn test_update_status_priority_and_note() {
        let db = setup_db("update_status_priority_note").await;
        let pool = db.pool().clone();
        let created = create_ticket(
            DbConn::Pool(&pool),
            CreateSupportTicketInput {
                customer_name: "Nia".to_string(),
                title: "Webhook retry spike".to_string(),
                details: "Requests are timing out after deploy".to_string(),
                priority: Some(TicketPriority::Normal),
            },
        )
        .await
        .expect("ticket created");

        let working = set_status(
            DbConn::Pool(&pool),
            SetTicketStatusInput {
                id: created.id,
                status: TicketStatus::Working,
            },
        )
        .await
        .expect("status updated");
        assert_eq!(working.status, TicketStatus::Working);

        let escalated = set_priority(
            DbConn::Pool(&pool),
            SetTicketPriorityInput {
                id: created.id,
                priority: TicketPriority::High,
            },
        )
        .await
        .expect("priority updated");
        assert_eq!(escalated.priority, TicketPriority::High);

        let noted = add_note(
            DbConn::Pool(&pool),
            AddTicketNoteInput {
                id: created.id,
                note: "Escalated to payments engineer".to_string(),
            },
        )
        .await
        .expect("note added");
        assert_eq!(
            noted.last_note.as_deref(),
            Some("Escalated to payments engineer")
        );

        db.cleanup().await.expect("cleanup");
    }

    #[tokio::test]
    async fn test_validation_rejects_empty_fields() {
        let db = setup_db("validation_rejects_empty_fields").await;
        let pool = db.pool().clone();

        let result = create_ticket(
            DbConn::Pool(&pool),
            CreateSupportTicketInput {
                customer_name: "   ".to_string(),
                title: "  ".to_string(),
                details: "  ".to_string(),
                priority: None,
            },
        )
        .await;

        assert!(matches!(result, Err(ForgeError::Validation(_))));

        db.cleanup().await.expect("cleanup");
    }
}
