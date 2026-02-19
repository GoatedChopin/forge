use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[forge::forge_enum]
#[derive(forge::forge_core::schemars::JsonSchema)]
pub enum TicketStatus {
    New,
    Working,
    Resolved,
}

#[forge::forge_enum]
#[derive(forge::forge_core::schemars::JsonSchema)]
pub enum TicketPriority {
    Low,
    Normal,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[forge::model]
pub struct SupportTicket {
    pub id: Uuid,
    pub customer_name: String,
    pub title: String,
    pub details: String,
    pub status: TicketStatus,
    pub priority: TicketPriority,
    pub last_note: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
