use crate::api::middleware::auth::AuthContext;
use crate::domain::track::{self, CreateTrackRequest, Track};
use crate::error::AppResult;
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    Extension, Json,
};

#[utoipa::path(
    get,
    path = "/tracks",
    tag = "Tracks",
    responses((status = 200, body = [Track])),
    security(("bearer_auth" = []))
)]
pub async fn list_tracks(
    State(state): State<AppState>,
    Extension(AuthContext(user)): Extension<AuthContext>,
) -> AppResult<Json<Vec<Track>>> {
    Ok(Json(track::list_for_user(&state, &user).await?))
}

#[utoipa::path(
    post,
    path = "/tracks",
    tag = "Tracks",
    request_body = CreateTrackRequest,
    responses((status = 201, body = Track)),
    security(("bearer_auth" = []))
)]
pub async fn create_track(
    State(state): State<AppState>,
    Extension(AuthContext(user)): Extension<AuthContext>,
    Json(req): Json<CreateTrackRequest>,
) -> AppResult<(axum::http::StatusCode, Json<Track>)> {
    let created = track::create_track(&state, &user, req).await?;
    Ok((axum::http::StatusCode::CREATED, Json(created)))
}

#[utoipa::path(
    get,
    path = "/tracks/{id}",
    tag = "Tracks",
    params(("id" = String, Path, description = "Track id")),
    responses((status = 200, body = Track)),
    security(("bearer_auth" = []))
)]
pub async fn get_track(
    State(state): State<AppState>,
    Extension(AuthContext(user)): Extension<AuthContext>,
    Path(id): Path<String>,
) -> AppResult<Json<Track>> {
    Ok(Json(track::get_track(&state, &user, &id).await?))
}

#[utoipa::path(
    post,
    path = "/tracks/{id}/cancel",
    tag = "Tracks",
    params(("id" = String, Path, description = "Track id")),
    responses((status = 200, body = Track)),
    security(("bearer_auth" = []))
)]
pub async fn cancel_track(
    State(state): State<AppState>,
    Extension(AuthContext(user)): Extension<AuthContext>,
    Path(id): Path<String>,
) -> AppResult<Json<Track>> {
    Ok(Json(track::cancel_track(&state, &user, &id).await?))
}

#[utoipa::path(
    post,
    path = "/tracks/{id}/trigger",
    tag = "Tracks",
    params(("id" = String, Path, description = "Track id")),
    responses((status = 200, body = Track)),
    security(("bearer_auth" = []))
)]
pub async fn trigger_track(
    State(state): State<AppState>,
    Extension(AuthContext(user)): Extension<AuthContext>,
    Path(id): Path<String>,
) -> AppResult<Json<Track>> {
    Ok(Json(track::trigger_track(&state, &user, &id).await?))
}

#[utoipa::path(
    get,
    path = "/tracks/by-media/{media_type}/{tmdb_id}",
    tag = "Tracks",
    params(
        ("media_type" = String, Path, description = "movie | tv"),
        ("tmdb_id" = i64, Path, description = "TMDB id")
    ),
    responses((status = 200, body = [Track])),
    security(("bearer_auth" = []))
)]
pub async fn find_track_by_media(
    State(state): State<AppState>,
    Extension(AuthContext(user)): Extension<AuthContext>,
    Path((media_type, tmdb_id)): Path<(String, i64)>,
) -> AppResult<Json<Vec<Track>>> {
    Ok(Json(
        track::find_for_media(&state, &user, &media_type, tmdb_id).await?,
    ))
}

#[utoipa::path(
    delete,
    path = "/tracks/{id}",
    tag = "Tracks",
    params(("id" = String, Path, description = "Track id")),
    responses((status = 204)),
    security(("bearer_auth" = []))
)]
pub async fn delete_track(
    State(state): State<AppState>,
    Extension(AuthContext(user)): Extension<AuthContext>,
    Path(id): Path<String>,
) -> AppResult<axum::http::StatusCode> {
    track::delete_track(&state, &user, &id).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}
