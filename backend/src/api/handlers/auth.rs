use crate::api::middleware::auth::AuthContext;
use crate::domain::auth;
use crate::error::AppResult;
use crate::state::AppState;
use axum::{extract::State, Extension, Json};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Deserialize, ToSchema)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Serialize, ToSchema)]
pub struct UserInfo {
    pub id: String,
    pub username: String,
    pub role: String,
}

#[derive(Serialize, ToSchema)]
pub struct LoginResponse {
    pub token: String,
    pub user: UserInfo,
}

#[utoipa::path(
    post,
    path = "/auth/login",
    tag = "Auth",
    request_body = LoginRequest,
    responses((status = 200, body = LoginResponse))
)]
pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> AppResult<Json<LoginResponse>> {
    let (token, user) = auth::login(
        &state.db,
        &state.config.jwt_secret,
        &req.username,
        &req.password,
    )
    .await?;
    Ok(Json(LoginResponse {
        token,
        user: UserInfo {
            id: user.id.to_string(),
            username: user.username,
            role: user.role,
        },
    }))
}

#[utoipa::path(
    get,
    path = "/auth/me",
    tag = "Auth",
    responses((status = 200, body = UserInfo)),
    security(("bearer_auth" = []))
)]
pub async fn me(Extension(AuthContext(user)): Extension<AuthContext>) -> Json<UserInfo> {
    Json(UserInfo {
        id: user.id.to_string(),
        username: user.username,
        role: user.role,
    })
}
