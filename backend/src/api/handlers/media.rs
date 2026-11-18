use crate::error::AppResult;
use crate::infra::media_server::{self, LibraryPresence};
use crate::infra::tmdb::{MediaItem, TvEpisode};
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
    #[serde(default = "default_source")]
    pub source: String,
}

fn default_page() -> u32 {
    1
}

fn default_kind() -> String {
    "latest".into()
}

fn default_source() -> String {
    "tmdb".into()
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
    Ok(Json(with_ratings(&state, items).await))
}

#[utoipa::path(
    get,
    path = "/media/movies",
    tag = "Media",
    params(
        ("page" = Option<u32>, Query, description = "Page number"),
        ("kind" = Option<String>, Query, description = "latest | popular | top"),
        ("source" = Option<String>, Query, description = "tmdb | imdb | rt")
    ),
    responses((status = 200, body = [MediaItem])),
    security(("bearer_auth" = []))
)]
pub async fn list_movies(
    State(state): State<AppState>,
    Query(q): Query<BrowseQuery>,
) -> AppResult<Json<Vec<MediaItem>>> {
    Ok(Json(browse(&state, "movie", q).await?))
}

#[utoipa::path(
    get,
    path = "/media/tv",
    tag = "Media",
    params(
        ("page" = Option<u32>, Query, description = "Page number"),
        ("kind" = Option<String>, Query, description = "latest | popular | top"),
        ("source" = Option<String>, Query, description = "tmdb | imdb | rt")
    ),
    responses((status = 200, body = [MediaItem])),
    security(("bearer_auth" = []))
)]
pub async fn list_tv(
    State(state): State<AppState>,
    Query(q): Query<BrowseQuery>,
) -> AppResult<Json<Vec<MediaItem>>> {
    Ok(Json(browse(&state, "tv", q).await?))
}

async fn browse(state: &AppState, media_type: &str, q: BrowseQuery) -> AppResult<Vec<MediaItem>> {
    let page = q.page.max(1);
    let source = match q.source.as_str() {
        "" | "tmdb" => "tmdb",
        "imdb" => "imdb",
        "rt" => "rt",
        _ => {
            return Err(crate::error::AppError::BadRequest(
                "source must be tmdb, imdb, or rt".into(),
            ))
        }
    };
    let clients = state.clients().await;
    let key = format!(
        "{media_type}|{source}|{}|{page}|{}",
        q.kind,
        clients.tmdb.language()
    );
    let ttl = browse_ttl(&q.kind);
    if let Some(items) = state.cached_browse(&key, ttl).await {
        return Ok(items);
    }
    let items = match source {
        "tmdb" => tmdb_list(&clients.tmdb, media_type, &q.kind, page).await?,
        _ => {
            let mut items = clients
                .mdblist
                .chart(media_type, source, &q.kind, page)
                .await?;
            clients.tmdb.localize_cards(&mut items).await;
            items
        }
    };
    let items = with_ratings(state, items).await;
    state.remember_browse(key, items.clone()).await;
    Ok(items)
}

fn browse_ttl(kind: &str) -> std::time::Duration {
    std::time::Duration::from_secs(match kind {
        "top" => 3 * 60 * 60,
        "popular" => 30 * 60,
        _ => 15 * 60,
    })
}

async fn tmdb_list(
    tmdb: &crate::infra::tmdb::TmdbClient,
    media_type: &str,
    kind: &str,
    page: u32,
) -> AppResult<Vec<MediaItem>> {
    match (media_type, kind) {
        ("tv", "popular") => tmdb.popular_tv(page).await,
        ("tv", "top") => tmdb.top_rated_tv(page).await,
        ("tv", _) => tmdb.latest_tv(page).await,
        (_, "popular") => tmdb.popular_movies(page).await,
        (_, "top") => tmdb.top_rated_movies(page).await,
        _ => tmdb.latest_movies(page).await,
    }
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
    let mut rated = with_ratings(&state, vec![item]).await;
    Ok(Json(rated.remove(0)))
}

async fn with_ratings(state: &AppState, mut items: Vec<MediaItem>) -> Vec<MediaItem> {
    let client = state.clients().await.mdblist;
    crate::infra::mdblist::attach(&state.db, &client, &mut items).await;
    items
}

#[utoipa::path(
    get,
    path = "/media/tv/{tmdb_id}/seasons/{season_number}",
    tag = "Media",
    params(
        ("tmdb_id" = i64, Path, description = "TMDB tv id"),
        ("season_number" = i64, Path, description = "Season number (0 = specials)")
    ),
    responses((status = 200, body = [TvEpisode])),
    security(("bearer_auth" = []))
)]
pub async fn tv_season(
    State(state): State<AppState>,
    Path((tmdb_id, season_number)): Path<(i64, i64)>,
) -> AppResult<Json<Vec<TvEpisode>>> {
    let episodes = state
        .clients()
        .await
        .tmdb
        .tv_season(tmdb_id, season_number)
        .await?;
    Ok(Json(episodes))
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
