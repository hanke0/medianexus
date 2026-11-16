use crate::error::AppResult;
use crate::infra::media_server::{self, LibraryPresence};
use crate::infra::tmdb::MediaItem;
use crate::state::AppState;
use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct BrowseQuery {
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_kind")]
    pub kind: String,
}

fn default_page() -> u32 {
    1
}

fn default_kind() -> String {
    "latest".into()
}

#[derive(Deserialize)]
pub struct SearchQuery {
    pub q: String,
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_search_type")]
    pub media_type: String,
}

fn default_search_type() -> String {
    "all".into()
}

#[utoipa::path(
    get,
    path = "/media/search",
    tag = "Media",
    params(
        ("q" = String, Query, description = "Search keyword"),
        ("page" = Option<u32>, Query, description = "Page number"),
        ("media_type" = Option<String>, Query, description = "movie | tv | all")
    ),
    responses((status = 200, body = [MediaItem])),
    security(("bearer_auth" = []))
)]
pub async fn search_media(
    State(state): State<AppState>,
    Query(q): Query<SearchQuery>,
) -> AppResult<Json<Vec<MediaItem>>> {
    let keyword = q.q.trim();
    if keyword.is_empty() {
        return Err(crate::error::AppError::BadRequest(
            "q must not be empty".into(),
        ));
    }
    let page = q.page.max(1);
    let items = state
        .clients()
        .await
        .tmdb
        .search(keyword, &q.media_type, page)
        .await?;
    Ok(Json(items))
}

#[utoipa::path(
    get,
    path = "/media/movies",
    tag = "Media",
    params(
        ("page" = Option<u32>, Query, description = "Page number"),
        ("kind" = Option<String>, Query, description = "latest | popular")
    ),
    responses((status = 200, body = [MediaItem])),
    security(("bearer_auth" = []))
)]
pub async fn list_movies(
    State(state): State<AppState>,
    Query(q): Query<BrowseQuery>,
) -> AppResult<Json<Vec<MediaItem>>> {
    let page = q.page.max(1);
    let tmdb = state.clients().await.tmdb;
    let items = if q.kind == "popular" {
        tmdb.popular_movies(page).await?
    } else {
        tmdb.latest_movies(page).await?
    };
    Ok(Json(items))
}

#[utoipa::path(
    get,
    path = "/media/tv",
    tag = "Media",
    params(
        ("page" = Option<u32>, Query, description = "Page number"),
        ("kind" = Option<String>, Query, description = "latest | popular")
    ),
    responses((status = 200, body = [MediaItem])),
    security(("bearer_auth" = []))
)]
pub async fn list_tv(
    State(state): State<AppState>,
    Query(q): Query<BrowseQuery>,
) -> AppResult<Json<Vec<MediaItem>>> {
    let page = q.page.max(1);
    let tmdb = state.clients().await.tmdb;
    let items = if q.kind == "popular" {
        tmdb.popular_tv(page).await?
    } else {
        tmdb.latest_tv(page).await?
    };
    Ok(Json(items))
}

#[utoipa::path(
    get,
    path = "/media/{media_type}/{tmdb_id}",
    tag = "Media",
    params(
        ("media_type" = String, Path, description = "movie | tv"),
        ("tmdb_id" = i64, Path, description = "TMDB id")
    ),
    responses((status = 200, body = MediaItem)),
    security(("bearer_auth" = []))
)]
pub async fn get_media(
    State(state): State<AppState>,
    Path((media_type, tmdb_id)): Path<(String, i64)>,
) -> AppResult<Json<MediaItem>> {
    let tmdb = state.clients().await.tmdb;
    let item = match media_type.as_str() {
        "movie" => tmdb.movie_details(tmdb_id).await?,
        "tv" => tmdb.tv_details(tmdb_id).await?,
        _ => {
            return Err(crate::error::AppError::BadRequest(
                "media_type must be movie or tv".into(),
            ))
        }
    };
    Ok(Json(item))
}

#[utoipa::path(
    get,
    path = "/media/{media_type}/{tmdb_id}/library",
    tag = "Media",
    params(
        ("media_type" = String, Path, description = "movie | tv"),
        ("tmdb_id" = i64, Path, description = "TMDB id")
    ),
    responses((status = 200, body = LibraryPresence)),
    security(("bearer_auth" = []))
)]
pub async fn library_presence(
    State(state): State<AppState>,
    Path((media_type, tmdb_id)): Path<(String, i64)>,
) -> AppResult<Json<LibraryPresence>> {
    if media_type != "movie" && media_type != "tv" {
        return Err(crate::error::AppError::BadRequest(
            "media_type must be movie or tv".into(),
        ));
    }
    let tmdb = state.clients().await.tmdb;
    let item = if media_type == "movie" {
        tmdb.movie_details(tmdb_id).await?
    } else {
        tmdb.tv_details(tmdb_id).await?
    };
    let mut local = crate::domain::maintenance::find_in_local_catalog(
        &state.db,
        &media_type,
        tmdb_id,
        item.imdb_id.as_deref(),
    )
    .await?;

    let settings = state.settings().await;
    let mut presence = media_server::find_in_libraries(
        &state.http,
        &settings,
        &media_type,
        tmdb_id,
        item.imdb_id.as_deref(),
    )
    .await?;
    if !local.is_empty() {
        presence.matches.append(&mut local);
        presence.found = true;
    }
    Ok(Json(presence))
}
