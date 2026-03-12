use forge::forge_core::function::ForgeConn;
use forge::prelude::*;

use crate::schema::{
    AddTicketNoteInput, CreateSupportTicketInput, SetTicketPriorityInput, SetTicketStatusInput,
    SupportTicket, TicketPriority, TicketStatus,
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

pub(crate) async fn list_tickets(conn: &mut ForgeConn<'_>) -> Result<Vec<SupportTicket>> {
    sqlx::query_as!(
        SupportTicket,
        r#"SELECT id, customer_name, title, details,
                  status as "status: TicketStatus",
                  priority as "priority: TicketPriority",
                  last_note, created_at, updated_at
         FROM support_tickets
         ORDER BY
           CASE status
             WHEN 'new' THEN 0
             WHEN 'working' THEN 1
             ELSE 2
           END,
           updated_at DESC"#
    )
    .fetch_all(&mut **conn)
    .await
    .map_err(Into::into)
}

pub(crate) async fn create_ticket(
    conn: &mut ForgeConn<'_>,
    input: CreateSupportTicketInput,
) -> Result<SupportTicket> {
    let customer_name = normalized_non_empty("Customer name", &input.customer_name, 80)?;
    let title = normalized_non_empty("Title", &input.title, 120)?;
    let details = normalized_non_empty("Details", &input.details, 1000)?;
    let priority = input.priority.unwrap_or(TicketPriority::Normal);

    sqlx::query_as!(
        SupportTicket,
        r#"INSERT INTO support_tickets (customer_name, title, details, priority)
         VALUES ($1, $2, $3, $4)
         RETURNING id, customer_name, title, details,
                   status as "status: TicketStatus",
                   priority as "priority: TicketPriority",
                   last_note, created_at, updated_at"#,
        customer_name,
        title,
        details,
        priority as TicketPriority
    )
    .fetch_one(&mut **conn)
    .await
    .map_err(Into::into)
}

pub(crate) async fn set_status(
    conn: &mut ForgeConn<'_>,
    input: SetTicketStatusInput,
) -> Result<SupportTicket> {
    sqlx::query_as!(
        SupportTicket,
        r#"UPDATE support_tickets
         SET status = $1, updated_at = NOW()
         WHERE id = $2
         RETURNING id, customer_name, title, details,
                   status as "status: TicketStatus",
                   priority as "priority: TicketPriority",
                   last_note, created_at, updated_at"#,
        input.status as TicketStatus,
        input.id
    )
    .fetch_optional(&mut **conn)
    .await?
    .ok_or_else(|| ForgeError::NotFound("Ticket not found".into()))
}

pub(crate) async fn set_priority(
    conn: &mut ForgeConn<'_>,
    input: SetTicketPriorityInput,
) -> Result<SupportTicket> {
    sqlx::query_as!(
        SupportTicket,
        r#"UPDATE support_tickets
         SET priority = $1, updated_at = NOW()
         WHERE id = $2
         RETURNING id, customer_name, title, details,
                   status as "status: TicketStatus",
                   priority as "priority: TicketPriority",
                   last_note, created_at, updated_at"#,
        input.priority as TicketPriority,
        input.id
    )
    .fetch_optional(&mut **conn)
    .await?
    .ok_or_else(|| ForgeError::NotFound("Ticket not found".into()))
}

pub(crate) async fn add_note(
    conn: &mut ForgeConn<'_>,
    input: AddTicketNoteInput,
) -> Result<SupportTicket> {
    let note = normalized_non_empty("Note", &input.note, 300)?;

    sqlx::query_as!(
        SupportTicket,
        r#"UPDATE support_tickets
         SET last_note = $1, updated_at = NOW()
         WHERE id = $2
         RETURNING id, customer_name, title, details,
                   status as "status: TicketStatus",
                   priority as "priority: TicketPriority",
                   last_note, created_at, updated_at"#,
        note,
        input.id
    )
    .fetch_optional(&mut **conn)
    .await?
    .ok_or_else(|| ForgeError::NotFound("Ticket not found".into()))
}

#[forge::query(public, tables = ["support_tickets"])]
pub async fn list_support_tickets(ctx: &QueryContext) -> Result<Vec<SupportTicket>> {
    sqlx::query_as!(
        SupportTicket,
        r#"SELECT id, customer_name, title, details,
                  status as "status: TicketStatus",
                  priority as "priority: TicketPriority",
                  last_note, created_at, updated_at
         FROM support_tickets
         ORDER BY
           CASE status
             WHEN 'new' THEN 0
             WHEN 'working' THEN 1
             ELSE 2
           END,
           updated_at DESC"#
    )
    .fetch_all(ctx.db())
    .await
    .map_err(Into::into)
}

#[forge::mutation(public)]
pub async fn create_support_ticket(
    ctx: &MutationContext,
    input: CreateSupportTicketInput,
) -> Result<SupportTicket> {
    let customer_name = normalized_non_empty("Customer name", &input.customer_name, 80)?;
    let title = normalized_non_empty("Title", &input.title, 120)?;
    let details = normalized_non_empty("Details", &input.details, 1000)?;
    let priority = input.priority.unwrap_or(TicketPriority::Normal);

    let mut conn = ctx.conn().await?;

    create_ticket(
        &mut conn,
        CreateSupportTicketInput {
            customer_name,
            title,
            details,
            priority: Some(priority),
        },
    )
    .await
}

#[forge::mutation(public)]
pub async fn set_ticket_status(
    ctx: &MutationContext,
    input: SetTicketStatusInput,
) -> Result<SupportTicket> {
    let mut conn = ctx.conn().await?;
    set_status(&mut conn, input).await
}

#[forge::mutation(public)]
pub async fn set_ticket_priority(
    ctx: &MutationContext,
    input: SetTicketPriorityInput,
) -> Result<SupportTicket> {
    let mut conn = ctx.conn().await?;
    set_priority(&mut conn, input).await
}

#[forge::mutation(public)]
pub async fn add_ticket_note(
    ctx: &MutationContext,
    input: AddTicketNoteInput,
) -> Result<SupportTicket> {
    let mut conn = ctx.conn().await?;
    add_note(&mut conn, input).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge::forge_core::function::ForgeConn;
    use forge::testing::IsolatedTestDb;
    use std::path::Path;

    use crate::schema::TicketStatus;

    fn db_tests_enabled() -> bool {
        cfg!(feature = "testcontainers") || std::env::var_os("TEST_DATABASE_URL").is_some()
    }

    async fn setup_db(test_name: &str) -> IsolatedTestDb {
        IsolatedTestDb::setup(
            test_name,
            &forge::get_internal_sql(),
            Path::new("migrations"),
        )
        .await
        .expect("test database setup")
    }

    #[tokio::test]
    async fn test_create_and_list_tickets() {
        if !db_tests_enabled() {
            eprintln!("skipping database-backed support-desk test");
            return;
        }

        let db = setup_db("create_and_list_tickets").await;
        let pool = db.pool().clone();
        let mut conn = ForgeConn::Pool(pool.acquire().await.expect("connection"));

        let created = create_ticket(
            &mut conn,
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

        let mut conn = ForgeConn::Pool(pool.acquire().await.expect("connection"));
        create_ticket(
            &mut conn,
            CreateSupportTicketInput {
                customer_name: "Noa".to_string(),
                title: "Billing portal question".to_string(),
                details: "Needs VAT invoice details".to_string(),
                priority: None,
            },
        )
        .await
        .expect("second ticket created");

        let mut conn = ForgeConn::Pool(pool.acquire().await.expect("connection"));
        let tickets = list_tickets(&mut conn).await.expect("tickets listed");
        assert_eq!(tickets.len(), 2);

        db.cleanup().await.expect("cleanup");
    }

    #[tokio::test]
    async fn test_update_status_priority_and_note() {
        if !db_tests_enabled() {
            eprintln!("skipping database-backed support-desk test");
            return;
        }

        let db = setup_db("update_status_priority_note").await;
        let pool = db.pool().clone();
        let mut conn = ForgeConn::Pool(pool.acquire().await.expect("connection"));
        let created = create_ticket(
            &mut conn,
            CreateSupportTicketInput {
                customer_name: "Nia".to_string(),
                title: "Webhook retry spike".to_string(),
                details: "Requests are timing out after deploy".to_string(),
                priority: Some(TicketPriority::Normal),
            },
        )
        .await
        .expect("ticket created");

        let mut conn = ForgeConn::Pool(pool.acquire().await.expect("connection"));
        let working = set_status(
            &mut conn,
            SetTicketStatusInput {
                id: created.id,
                status: TicketStatus::Working,
            },
        )
        .await
        .expect("status updated");
        assert_eq!(working.status, TicketStatus::Working);

        let mut conn = ForgeConn::Pool(pool.acquire().await.expect("connection"));
        let escalated = set_priority(
            &mut conn,
            SetTicketPriorityInput {
                id: created.id,
                priority: TicketPriority::High,
            },
        )
        .await
        .expect("priority updated");
        assert_eq!(escalated.priority, TicketPriority::High);

        let mut conn = ForgeConn::Pool(pool.acquire().await.expect("connection"));
        let noted = add_note(
            &mut conn,
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
        if !db_tests_enabled() {
            eprintln!("skipping database-backed support-desk test");
            return;
        }

        let db = setup_db("validation_rejects_empty_fields").await;
        let pool = db.pool().clone();
        let mut conn = ForgeConn::Pool(pool.acquire().await.expect("connection"));

        let result = create_ticket(
            &mut conn,
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
