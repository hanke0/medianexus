use crate::domain::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use bcrypt::{hash, DEFAULT_COST};
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct UserInfo {
    pub id: String,
    pub username: String,
    pub role: String,
    pub created_at: String,
}

pub struct CreateUserParams {
    pub username: String,
    pub password: String,
    pub role: String,
}

pub async fn list_users(state: &AppState, actor: &AuthUser) -> AppResult<Vec<UserInfo>> {
    require_admin(actor)?;
    let rows: Vec<(String, String, String, String)> =
        sqlx::query_as("SELECT id, username, role, created_at FROM users ORDER BY created_at")
            .fetch_all(&state.db)
            .await?;
    Ok(rows
        .into_iter()
        .map(|(id, username, role, created_at)| UserInfo {
            id,
            username,
            role,
            created_at,
        })
        .collect())
}

pub async fn create_user(
    state: &AppState,
    actor: &AuthUser,
    params: CreateUserParams,
) -> AppResult<UserInfo> {
    require_admin(actor)?;

    let username = params.username.trim().to_string();
    if username.is_empty() || params.password.len() < 4 {
        return Err(AppError::BadRequest(
            "username required and password must be at least 4 characters".into(),
        ));
    }
    if params.role != "admin" && params.role != "user" {
        return Err(AppError::BadRequest("role must be admin or user".into()));
    }

    let exists: Option<(i64,)> = sqlx::query_as("SELECT 1 FROM users WHERE username = ?")
        .bind(&username)
        .fetch_optional(&state.db)
        .await?;
    if exists.is_some() {
        return Err(AppError::Conflict("username already exists".into()));
    }

    let id = Uuid::new_v4().to_string();
    let password_hash = hash(&params.password, DEFAULT_COST)?;
    sqlx::query("INSERT INTO users (id, username, password_hash, role) VALUES (?, ?, ?, ?)")
        .bind(&id)
        .bind(&username)
        .bind(&password_hash)
        .bind(&params.role)
        .execute(&state.db)
        .await?;

    let created_at: (String,) = sqlx::query_as("SELECT created_at FROM users WHERE id = ?")
        .bind(&id)
        .fetch_one(&state.db)
        .await?;

    Ok(UserInfo {
        id,
        username,
        role: params.role,
        created_at: created_at.0,
    })
}

pub async fn delete_user(state: &AppState, actor: &AuthUser, user_id: &str) -> AppResult<()> {
    require_admin(actor)?;
    if actor.id.to_string() == user_id {
        return Err(AppError::BadRequest("cannot delete yourself".into()));
    }
    let result = sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(user_id)
        .execute(&state.db)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("user not found".into()));
    }
    Ok(())
}

fn require_admin(actor: &AuthUser) -> AppResult<()> {
    if actor.is_admin() {
        Ok(())
    } else {
        Err(AppError::Forbidden("admin only".into()))
    }
}
