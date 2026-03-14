use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[forge::model]
pub struct Counter {
    pub id: Uuid,
    pub name: String,
    pub value: i64,
    pub updated_by: Option<Uuid>,
    pub updated_at: DateTime<Utc>,
}
