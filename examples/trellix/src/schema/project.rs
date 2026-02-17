use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[forge::model]
pub struct Project {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub owner_id: Uuid,
    pub archived: bool,
    pub archive_started_at: Option<DateTime<Utc>>,
    pub archive_delete_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}
