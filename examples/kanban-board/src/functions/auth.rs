use forge::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::schema::User;

#[derive(Debug, Serialize, Deserialize)]
pub struct RegisterInput {
    pub email: String,
    pub name: String,
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginInput {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthResponse {
    pub token: String,
    pub user: UserPublic,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UserPublic {
    pub id: Uuid,
    pub email: String,
    pub name: String,
}

fn sign_jwt(user_id: Uuid, secret: &str) -> Result<String> {
    let claims = serde_json::json!({
        "sub": user_id,
        "iat": chrono::Utc::now().timestamp(),
        "exp": (chrono::Utc::now() + chrono::Duration::days(7)).timestamp(),
    });

    jsonwebtoken::encode(
        &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| ForgeError::Internal(format!("JWT signing failed: {e}")))
}

fn auth_response(ctx: &MutationContext, user: &User) -> Result<AuthResponse> {
    let secret = ctx.env_require("JWT_SECRET")?;
    let token = sign_jwt(user.id, &secret)?;
    Ok(AuthResponse {
        token,
        user: UserPublic {
            id: user.id,
            email: user.email.clone(),
            name: user.name.clone(),
        },
    })
}

#[forge::mutation(public)]
pub async fn register(ctx: &MutationContext, input: RegisterInput) -> Result<AuthResponse> {
    if input.email.trim().is_empty() {
        return Err(ForgeError::Validation("Email is required".into()));
    }
    if input.name.trim().is_empty() {
        return Err(ForgeError::Validation("Name is required".into()));
    }
    if input.password.len() < 8 {
        return Err(ForgeError::Validation(
            "Password must be at least 8 characters".into(),
        ));
    }

    let password_hash =
        bcrypt::hash(&input.password, 10).map_err(|e| ForgeError::Internal(e.to_string()))?;

    let email = input.email.trim().to_string();
    let name = input.name.trim().to_string();
    let mut conn = ctx
        .conn()
        .await?;

    let user = sqlx::query_as!(
        User,
        "INSERT INTO users (email, name, password_hash) VALUES ($1, $2, $3) RETURNING *",
        email,
        name,
        &password_hash
    )
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| {
        if e.to_string().contains("idx_users_email") {
            ForgeError::Validation("Email already registered".into())
        } else {
            ForgeError::from(e)
        }
    })?;

    auth_response(ctx, &user)
}

#[forge::mutation(public)]
pub async fn login(ctx: &MutationContext, input: LoginInput) -> Result<AuthResponse> {
    let mut conn = ctx
        .conn()
        .await?;

    let user = sqlx::query_as!(User, "SELECT * FROM users WHERE email = $1", &input.email)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or_else(|| ForgeError::Validation("Invalid email or password".into()))?;

    let valid = bcrypt::verify(&input.password, &user.password_hash)
        .map_err(|e| ForgeError::Internal(e.to_string()))?;

    if !valid {
        return Err(ForgeError::Validation("Invalid email or password".into()));
    }

    auth_response(ctx, &user)
}
