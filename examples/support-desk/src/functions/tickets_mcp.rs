use forge::forge_core::mcp::McpToolContext;

use super::tickets::{add_note, create_ticket, list_tickets, set_priority, set_status};
use crate::schema::{
    AddTicketNoteInput, CreateSupportTicketInput, McpAddTicketNoteInput,
    McpCreateSupportTicketInput, McpSetTicketPriorityInput, McpSetTicketStatusInput, McpTicket,
    SetTicketPriorityInput, SetTicketStatusInput,
};

#[forge::mcp_tool(
    name = "support.list_tickets",
    title = "List Support Tickets",
    description = "List tickets visible in the support inbox",
    public,
    read_only
)]
pub async fn mcp_list_support_tickets(
    ctx: &McpToolContext,
) -> forge::forge_core::Result<Vec<McpTicket>> {
    Ok(list_tickets(ctx.db_conn())
        .await?
        .into_iter()
        .map(McpTicket::from)
        .collect())
}

#[forge::mcp_tool(
    name = "support.create_ticket",
    title = "Create Support Ticket",
    description = "Create a new ticket in the support inbox",
    public,
    idempotent
)]
pub async fn mcp_create_support_ticket(
    ctx: &McpToolContext,
    input: McpCreateSupportTicketInput,
) -> forge::forge_core::Result<McpTicket> {
    let ticket = create_ticket(
        ctx.db_conn(),
        CreateSupportTicketInput {
            customer_name: input.customer_name,
            title: input.title,
            details: input.details,
            priority: Some(input.priority),
        },
    )
    .await?;

    Ok(ticket.into())
}

#[forge::mcp_tool(
    name = "support.set_status",
    title = "Set Ticket Status",
    description = "Move a ticket through lifecycle states",
    public
)]
pub async fn mcp_set_ticket_status(
    ctx: &McpToolContext,
    input: McpSetTicketStatusInput,
) -> forge::forge_core::Result<McpTicket> {
    let ticket = set_status(
        ctx.db_conn(),
        SetTicketStatusInput {
            id: input.id,
            status: input.status,
        },
    )
    .await?;

    Ok(ticket.into())
}

#[forge::mcp_tool(
    name = "support.set_priority",
    title = "Set Ticket Priority",
    description = "Adjust urgency for a support ticket",
    public
)]
pub async fn mcp_set_ticket_priority(
    ctx: &McpToolContext,
    input: McpSetTicketPriorityInput,
) -> forge::forge_core::Result<McpTicket> {
    let ticket = set_priority(
        ctx.db_conn(),
        SetTicketPriorityInput {
            id: input.id,
            priority: input.priority,
        },
    )
    .await?;

    Ok(ticket.into())
}

#[forge::mcp_tool(
    name = "support.add_note",
    title = "Add Ticket Note",
    description = "Add the latest internal note to a ticket",
    public
)]
pub async fn mcp_add_ticket_note(
    ctx: &McpToolContext,
    input: McpAddTicketNoteInput,
) -> forge::forge_core::Result<McpTicket> {
    let ticket = add_note(
        ctx.db_conn(),
        AddTicketNoteInput {
            id: input.id,
            note: input.note,
        },
    )
    .await?;

    Ok(ticket.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge::forge_core::function::{AuthContext, RequestMetadata};
    use forge::testing::IsolatedTestDb;
    use std::path::Path;

    use crate::schema::{TicketPriority, TicketStatus};

    async fn setup_db(test_name: &str) -> IsolatedTestDb {
        IsolatedTestDb::setup(
            test_name,
            &forge::get_internal_sql(),
            Path::new("migrations"),
        )
        .await
        .expect("test database setup")
    }

    fn mcp_ctx(pool: sqlx::PgPool) -> McpToolContext {
        McpToolContext::new(pool, AuthContext::unauthenticated(), RequestMetadata::new())
    }

    #[tokio::test]
    async fn test_mcp_tools_can_run_same_actions_as_ui() {
        let db = setup_db("mcp_tools_parity").await;
        let pool = db.pool().clone();
        let ctx = mcp_ctx(pool.clone());

        let created = mcp_create_support_ticket(
            &ctx,
            McpCreateSupportTicketInput {
                customer_name: "Lena".to_string(),
                title: "Export stuck at 42%".to_string(),
                details: "Customer shared screenshot from dashboard".to_string(),
                priority: TicketPriority::Normal,
            },
        )
        .await
        .expect("mcp ticket created");

        let _ = mcp_set_ticket_status(
            &ctx,
            McpSetTicketStatusInput {
                id: created.id,
                status: TicketStatus::Working,
            },
        )
        .await
        .expect("mcp status set");

        let updated = mcp_add_ticket_note(
            &ctx,
            McpAddTicketNoteInput {
                id: created.id,
                note: "LLM collected repro steps".to_string(),
            },
        )
        .await
        .expect("mcp note added");

        assert_eq!(updated.status, TicketStatus::Working);
        assert_eq!(
            updated.last_note.as_deref(),
            Some("LLM collected repro steps")
        );

        let listed = mcp_list_support_tickets(&ctx).await.expect("mcp list");
        assert_eq!(listed.len(), 1);

        db.cleanup().await.expect("cleanup");
    }
}
