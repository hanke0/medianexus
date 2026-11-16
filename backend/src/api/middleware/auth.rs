use crate::domain::auth::{self, AuthUser};
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use axum::{
    extract::{Request, State},
    http::header::AUTHORIZATION,
    middleware::Next,
    response::Response,
};

#[derive(Clone, Debug)]
pub struct AuthContext(pub AuthUser);

pub async fn require_auth(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let header = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::Unauthorized("Missing Authorization header".into()))?;

    let token = header
        .strip_prefix("Bearer ")
        .ok_or_else(|| AppError::Unauthorized("Use Authorization: Bearer <token>".into()))?
        .trim();

    let user = auth::authenticate_bearer_token(&state.db, &state.config.jwt_secret, token).await?;
    req.extensions_mut().insert(AuthContext(user));
    Ok(next.run(req).await)
}

pub fn auth_user(req: &Request) -> AppResult<&AuthUser> {
    req.extensions()
        .get::<AuthContext>()
        .map(|c| &c.0)
        .ok_or_else(|| AppError::Unauthorized("Not authenticated".into()))
}
