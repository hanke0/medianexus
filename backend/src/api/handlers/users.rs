use crate::api::middleware::auth::AuthContext;
use crate::domain::user::{self, CreateUserParams};
use crate::error::AppResult;
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
pub struct UserDto {
    pub id: String,
    pub username: String,
    pub role: String,
    pub created_at: String,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateUserRequest {
    pub username: String,
    pub password: String,
    #[serde(default = "default_role")]
    pub role: String,
}

fn default_role() -> String {
    "user".into()
}

#[utoipa::path(
    get,
    path = "/users",
    tag = "Users",
    responses((status = 200, body = [UserDto])),
    security(("bearer_auth" = []))
)]
pub async fn list_users(
    State(state): State<AppState>,
    Extension(AuthContext(user)): Extension<AuthContext>,
) -> AppResult<Json<Vec<UserDto>>> {
    let users = user::list_users(&state, &user).await?;
    Ok(Json(
        users
            .into_iter()
            .map(|u| UserDto {
                id: u.id,
                username: u.username,
                role: u.role,
                created_at: u.created_at,
            })
            .collect(),
    ))
}

#[utoipa::path(
    post,
    path = "/users",
    tag = "Users",
    request_body = CreateUserRequest,
    responses((status = 201, body = UserDto)),
    security(("bearer_auth" = []))
)]
pub async fn create_user(
    State(state): State<AppState>,
    Extension(AuthContext(user)): Extension<AuthContext>,
    Json(req): Json<CreateUserRequest>,
) -> AppResult<(axum::http::StatusCode, Json<UserDto>)> {
    let created = user::create_user(
        &state,
        &user,
        CreateUserParams {
            username: req.username,
            password: req.password,
            role: req.role,
        },
    )
    .await?;
    Ok((
        axum::http::StatusCode::CREATED,
        Json(UserDto {
            id: created.id,
            username: created.username,
            role: created.role,
            created_at: created.created_at,
        }),
    ))
}

#[utoipa::path(
    delete,
    path = "/users/{id}",
    tag = "Users",
    params(("id" = String, Path, description = "User id")),
    responses((status = 204)),
    security(("bearer_auth" = []))
)]
pub async fn delete_user(
    State(state): State<AppState>,
    Extension(AuthContext(user)): Extension<AuthContext>,
    Path(id): Path<String>,
) -> AppResult<axum::http::StatusCode> {
    user::delete_user(&state, &user, &id).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}
