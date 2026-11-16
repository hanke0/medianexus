use crate::error::{AppError, AppResult};
use crate::state::AppState;
use bcrypt::{hash, verify, DEFAULT_COST};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtClaims {
    pub sub: String,
    pub username: String,
    pub role: String,
    pub exp: i64,
}

#[derive(Debug, Clone)]
pub struct AuthUser {
    pub id: Uuid,
    pub username: String,
    pub role: String,
}

impl AuthUser {
    pub fn is_admin(&self) -> bool {
        self.role == "admin"
    }
}

pub async fn authenticate_basic_credentials(
    db: &SqlitePool,
    username: &str,
    password: &str,
) -> AppResult<AuthUser> {
    let row: Option<(String, String, String)> =
        sqlx::query_as("SELECT id, password_hash, role FROM users WHERE username = ?")
            .bind(username)
            .fetch_optional(db)
            .await?;

    let (id, password_hash, role) =
        row.ok_or_else(|| AppError::Unauthorized("Invalid credentials".into()))?;

    if !verify(password, &password_hash)? {
        return Err(AppError::Unauthorized("Invalid credentials".into()));
    }

    Ok(AuthUser {
        id: Uuid::parse_str(&id).map_err(|e| AppError::Internal(e.to_string()))?,
        username: username.to_string(),
        role,
    })
}

pub async fn authenticate_bearer_token(
    db: &SqlitePool,
    jwt_secret: &str,
    token: &str,
) -> AppResult<AuthUser> {
    let claims = decode_token(jwt_secret, token)?;
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT username, role FROM users WHERE id = ?")
            .bind(&claims.sub)
            .fetch_optional(db)
            .await?;

    let (username, role) =
        row.ok_or_else(|| AppError::Unauthorized("Session invalid: user no longer exists".into()))?;

    Ok(AuthUser {
        id: Uuid::parse_str(&claims.sub).map_err(|e| AppError::Unauthorized(e.to_string()))?,
        username,
        role,
    })
}

pub async fn login(
    db: &SqlitePool,
    jwt_secret: &str,
    username: &str,
    password: &str,
) -> AppResult<(String, AuthUser)> {
    let user = authenticate_basic_credentials(db, username, password).await?;
    let token = issue_token(jwt_secret, &user)?;
    Ok((token, user))
}

pub fn issue_token(jwt_secret: &str, user: &AuthUser) -> AppResult<String> {
    let claims = JwtClaims {
        sub: user.id.to_string(),
        username: user.username.clone(),
        role: user.role.clone(),
        exp: (Utc::now() + Duration::days(7)).timestamp(),
    };
    Ok(encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(jwt_secret.as_bytes()),
    )?)
}

pub fn decode_token(jwt_secret: &str, token: &str) -> AppResult<JwtClaims> {
    let data = decode::<JwtClaims>(
        token,
        &DecodingKey::from_secret(jwt_secret.as_bytes()),
        &Validation::default(),
    )?;
    Ok(data.claims)
}

pub async fn ensure_default_admin(state: &AppState) -> AppResult<()> {
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(&state.db)
        .await?;
    if count.0 > 0 {
        return Ok(());
    }

    let id = Uuid::new_v4().to_string();
    let password_hash = hash(&state.config.admin_password, DEFAULT_COST)?;
    sqlx::query("INSERT INTO users (id, username, password_hash, role) VALUES (?, ?, ?, 'admin')")
        .bind(&id)
        .bind(&state.config.admin_username)
        .bind(&password_hash)
        .execute(&state.db)
        .await?;

    tracing::info!(
        "Created default admin user '{}'",
        state.config.admin_username
    );
    Ok(())
}
