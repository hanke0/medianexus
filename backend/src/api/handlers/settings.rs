use crate::api::middleware::auth::AuthContext;
use crate::domain::auth::AuthUser;
use crate::domain::connection_test::{self, ConnectionTarget, ConnectionTestResult};
use crate::domain::settings::{self, IntegrationSettings};
use crate::error::{AppError, AppResult};
use crate::infra::plex::{self, PlexLinkStart, PlexLinkStatus};
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    Extension, Json,
};
use serde::Deserialize;
use utoipa::ToSchema;

#[utoipa::path(
    get,
    path = "/settings",
    tag = "Settings",
    responses((status = 200, body = IntegrationSettings)),
    security(("bearer_auth" = []))
)]
pub async fn get_settings(
    State(state): State<AppState>,
    Extension(AuthContext(user)): Extension<AuthContext>,
) -> AppResult<Json<IntegrationSettings>> {
    Ok(Json(settings::get_for_admin(&state.db, &user).await?))
}

#[utoipa::path(
    put,
    path = "/settings",
    tag = "Settings",
    request_body = IntegrationSettings,
    responses((status = 200, body = IntegrationSettings)),
    security(("bearer_auth" = []))
)]
pub async fn update_settings(
    State(state): State<AppState>,
    Extension(AuthContext(user)): Extension<AuthContext>,
    Json(body): Json<IntegrationSettings>,
) -> AppResult<Json<IntegrationSettings>> {
    let saved = settings::update_for_admin(&state.db, &user, body).await?;
    state.reload_integrations().await?;
    Ok(Json(saved))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PlexLinkStartRequest {
    /// Where Plex Auth App should return (SPA settings page).
    #[serde(default)]
    pub forward_url: String,
}

#[utoipa::path(
    post,
    path = "/settings/plex/link/start",
    tag = "Settings",
    request_body = PlexLinkStartRequest,
    responses((status = 200, body = PlexLinkStart)),
    security(("bearer_auth" = []))
)]
pub async fn plex_link_start(
    State(state): State<AppState>,
    Extension(AuthContext(user)): Extension<AuthContext>,
    Json(body): Json<PlexLinkStartRequest>,
) -> AppResult<Json<PlexLinkStart>> {
    require_admin(&user)?;
    let started = plex::start_link(&state.http, &state.db, &body.forward_url).await?;
    state.reload_integrations().await?;
    Ok(Json(started))
}

#[utoipa::path(
    get,
    path = "/settings/plex/link/status",
    tag = "Settings",
    responses((status = 200, body = PlexLinkStatus)),
    security(("bearer_auth" = []))
)]
pub async fn plex_link_status(
    State(state): State<AppState>,
    Extension(AuthContext(user)): Extension<AuthContext>,
) -> AppResult<Json<PlexLinkStatus>> {
    require_admin(&user)?;
    let status = plex::poll_link(&state.http, &state.db).await?;
    if status.linked {
        state.reload_integrations().await?;
    }
    Ok(Json(status))
}

#[utoipa::path(
    delete,
    path = "/settings/plex/link",
    tag = "Settings",
    responses((status = 200, body = PlexLinkStatus)),
    security(("bearer_auth" = []))
)]
pub async fn plex_unlink(
    State(state): State<AppState>,
    Extension(AuthContext(user)): Extension<AuthContext>,
) -> AppResult<Json<PlexLinkStatus>> {
    require_admin(&user)?;
    let status = plex::unlink(&state.db).await?;
    state.reload_integrations().await?;
    Ok(Json(status))
}

#[utoipa::path(
    post,
    path = "/settings/test/{target}",
    tag = "Settings",
    params(("target" = String, Path, description = "mteam | qbittorrent | jellyfin | plex")),
    responses((status = 200, body = ConnectionTestResult)),
    security(("bearer_auth" = []))
)]
pub async fn test_connection(
    State(state): State<AppState>,
    Extension(AuthContext(user)): Extension<AuthContext>,
    Path(target): Path<String>,
) -> AppResult<Json<ConnectionTestResult>> {
    let target = parse_target(&target)?;
    Ok(Json(
        connection_test::test_connection(&state, &user, target).await?,
    ))
}

fn parse_target(raw: &str) -> AppResult<ConnectionTarget> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "mteam" => Ok(ConnectionTarget::Mteam),
        "qbittorrent" | "qb" => Ok(ConnectionTarget::Qbittorrent),
        "jellyfin" => Ok(ConnectionTarget::Jellyfin),
        "plex" => Ok(ConnectionTarget::Plex),
        other => Err(AppError::BadRequest(format!(
            "unknown connection target: {other}"
        ))),
    }
}

fn require_admin(user: &AuthUser) -> AppResult<()> {
    if user.is_admin() {
        Ok(())
    } else {
        Err(AppError::Forbidden("admin only".into()))
    }
}
