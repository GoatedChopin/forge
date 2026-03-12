use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{TicketPriority, TicketStatus};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSupportTicketInput {
    pub customer_name: String,
    pub title: String,
    pub details: String,
    pub priority: Option<TicketPriority>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetTicketStatusInput {
    pub id: Uuid,
    pub status: TicketStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetTicketPriorityInput {
    pub id: Uuid,
    pub priority: TicketPriority,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddTicketNoteInput {
    pub id: Uuid,
    pub note: String,
}
