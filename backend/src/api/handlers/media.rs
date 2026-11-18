use crate::config::BrowseList;
use crate::error::AppResult;
use crate::infra::media_server::{self, LibraryPresence};
use crate::infra::tmdb::{MediaItem, TvEpisode};
use crate::state::AppState;
use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Deserialize)]
pub struct BrowseQuery {
    #[serde(default = "default_page")]
    pub page: u32,
    /// `tmdb:latest`, `tmdb:popular`, `tmdb:top`, `imdb:moviemeter`, or `list:user/slug`.
    #[serde(default = "default_catalog")]
    pub catalog: String,
}

fn default_catalog() -> String {
    "tmdb:latest".into()
}

#[derive(Serialize, ToSchema)]
pub struct MediaCatalog {
    pub id: String,
    pub label: String,
    /// `movie`, `tv`, or `all` when the list follows the movie/tv tab.
    pub media_type: String,
}

fn default_page() -> u32 {
    1
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
    path = "/media/catalogs",
    tag = "Media",
    responses((status = 200, body = [MediaCatalog])),
    security(("bearer_auth" = []))
)]
pub async fn list_catalogs(State(state): State<AppState>) -> Json<Vec<MediaCatalog>> {
    Json(catalogs(&state.config.browse_lists))
}

fn catalogs(extra: &[BrowseList]) -> Vec<MediaCatalog> {
    let mut out = vec![
        builtin("tmdb:latest", "TMDB 最新"),
        builtin("tmdb:popular", "TMDB 最热"),
        builtin("tmdb:top", "TMDB 高分"),
        builtin("imdb:moviemeter", "IMDb 最热"),
    ];
    for list in extra {
        out.push(MediaCatalog {
            id: list.id(),
            label: list.label(),
            media_type: list.media_type().into(),
        });
    }
    out
}

fn builtin(id: &str, label: &str) -> MediaCatalog {
    MediaCatalog {
        id: id.into(),
        label: label.into(),
        media_type: "all".into(),
    }
}

#[utoipa::path(
    get,
    path = "/media/movies",
    tag = "Media",
    params(
        ("page" = Option<u32>, Query, description = "Page number"),
        ("catalog" = Option<String>, Query, description = "tmdb:latest | tmdb:popular | tmdb:top | imdb:moviemeter | list:user/slug")
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
        ("catalog" = Option<String>, Query, description = "tmdb:latest | tmdb:popular | tmdb:top | imdb:moviemeter | list:user/slug")
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
    let catalog = q.catalog.trim();
    let catalog = if catalog.is_empty() {
        "tmdb:latest"
    } else {
        catalog
    };
    let clients = state.clients().await;
    let key = format!(
        "{media_type}|{catalog}|{page}|{}",
        clients.tmdb.language()
    );
    if let Some(items) = state.cached_browse(&key).await {
        return Ok(items);
    }
    let items = load_catalog(state, &clients, media_type, catalog, page).await?;
    let items = with_ratings(state, items).await;
    state.remember_browse(key, items.clone()).await;
    Ok(items)
}

async fn load_catalog(
    state: &AppState,
    clients: &crate::domain::settings::IntegrationClients,
    media_type: &str,
    catalog: &str,
    page: u32,
) -> AppResult<Vec<MediaItem>> {
    match catalog {
        "tmdb:latest" => tmdb_list(&clients.tmdb, media_type, "latest", page).await,
        "tmdb:popular" => tmdb_list(&clients.tmdb, media_type, "popular", page).await,
        "tmdb:top" => tmdb_list(&clients.tmdb, media_type, "top", page).await,
        "imdb:moviemeter" => {
            let mut items = clients
                .mdblist
                .ranked_list("official", "moviemeter", media_type, page)
                .await?;
            clients.tmdb.localize_cards(&mut items).await;
            Ok(items)
        }
        other => {
            let list = configured_list(state, other)?;
            if list.media_type() != media_type {
                return Err(crate::error::AppError::BadRequest(
                    "这个列表和当前的电影/电视剧分类不一致".into(),
                ));
            }
            let mut items = clients
                .mdblist
                .ranked_list(&list.user, &list.slug, media_type, page)
                .await?;
            clients.tmdb.localize_cards(&mut items).await;
            Ok(items)
        }
    }
}

fn configured_list(state: &AppState, catalog: &str) -> AppResult<BrowseList> {
    let Some(path) = catalog.strip_prefix("list:") else {
        return Err(crate::error::AppError::BadRequest(
            "unknown catalog".into(),
        ));
    };
    state
        .config
        .browse_lists
        .iter()
        .find(|list| format!("{}/{}", list.user, list.slug) == path)
        .cloned()
        .ok_or_else(|| crate::error::AppError::BadRequest("unknown catalog".into()))
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
