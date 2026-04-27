use forge::prelude::*;

#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    sqlx::Type,
    forge::schemars::JsonSchema,
)]
#[schemars(crate = "forge::schemars")]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "user_role", rename_all = "lowercase")]
pub enum UserRole {
    Admin,
    #[default]
    Member,
    Guest,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub role: UserRole,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    #[serde(skip_serializing)]
    pub password_hash: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UserPublic {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub role: UserRole,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl From<User> for UserPublic {
    fn from(u: User) -> Self {
        Self {
            id: u.id,
            email: u.email,
            name: u.name,
            role: u.role,
            created_at: u.created_at,
            updated_at: u.updated_at,
        }
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct AuthResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub user: UserPublic,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct DemoStats {
    pub total_users: i64,
    pub total_trades: i64,
    pub total_webhooks: i64,
    pub computed_at: Timestamp,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, forge::schemars::JsonSchema)]
#[schemars(crate = "forge::schemars")]
pub struct McpUserInfo {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub role: UserRole,
}
