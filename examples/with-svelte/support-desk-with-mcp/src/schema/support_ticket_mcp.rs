use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{SupportTicket, TicketPriority, TicketStatus};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct McpTicket {
    pub id: Uuid,
    pub customer_name: String,
    pub title: String,
    pub details: String,
    pub status: TicketStatus,
    pub priority: TicketPriority,
    pub last_note: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct McpCreateSupportTicketInput {
    #[schemars(description = "Customer display name", length(min = 2, max = 80))]
    pub customer_name: String,
    #[schemars(
        description = "Short title used in inbox list",
        length(min = 3, max = 120)
    )]
    pub title: String,
    #[schemars(
        description = "Full issue context for agents and LLMs",
        length(min = 3, max = 1000)
    )]
    pub details: String,
    #[schemars(description = "Priority lane", default = "default_ticket_priority")]
    #[serde(default = "default_ticket_priority")]
    pub priority: TicketPriority,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct McpSetTicketStatusInput {
    #[schemars(description = "Ticket id")]
    pub id: Uuid,
    #[schemars(description = "New lifecycle state")]
    pub status: TicketStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct McpSetTicketPriorityInput {
    #[schemars(description = "Ticket id")]
    pub id: Uuid,
    #[schemars(description = "Priority lane")]
    pub priority: TicketPriority,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct McpAddTicketNoteInput {
    #[schemars(description = "Ticket id")]
    pub id: Uuid,
    #[schemars(
        description = "Latest internal note to attach",
        length(min = 3, max = 300)
    )]
    pub note: String,
}

const fn default_ticket_priority() -> TicketPriority {
    TicketPriority::Normal
}

impl From<SupportTicket> for McpTicket {
    fn from(value: SupportTicket) -> Self {
        Self {
            id: value.id,
            customer_name: value.customer_name,
            title: value.title,
            details: value.details,
            status: value.status,
            priority: value.priority,
            last_note: value.last_note,
            created_at: value.created_at.to_rfc3339(),
            updated_at: value.updated_at.to_rfc3339(),
        }
    }
}
