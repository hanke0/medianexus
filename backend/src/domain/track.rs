use crate::domain::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct Track {
    pub id: String,
    pub user_id: String,
    pub media_type: String,
    pub tmdb_id: i64,
    /// TV season. `None` for movies and legacy whole-show tracks.
    pub season_number: Option<i64>,
    /// Set when this row downloads one episode. `None` is a season pack (or a movie).
    pub episode_number: Option<i64>,
    pub title: String,
    pub original_title: Option<String>,
    pub year: Option<String>,
    pub poster_path: Option<String>,
    pub overview: Option<String>,
    pub imdb_id: Option<String>,
    pub status: String,
    pub progress: f64,
    pub torrent_id: Option<String>,
    pub torrent_site: Option<String>,
    pub torrent_name: Option<String>,
    pub torrent_hash: Option<String>,
    pub torrent_size: Option<String>,
    pub download_path: Option<String>,
    pub hardlink_path: Option<String>,
    pub error_message: Option<String>,
    pub status_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateTrackRequest {
    pub media_type: String,
    pub tmdb_id: i64,
    /// Required for `tv`. Ignored for movies.
    #[serde(default)]
    pub season_number: Option<i64>,
    /// Optional single episode inside `season_number`.
    #[serde(default)]
    pub episode_number: Option<i64>,
}

pub struct EpisodeTorrent<'a> {
    pub episode_number: i64,
    pub site: &'a str,
    pub torrent_id: &'a str,
    pub torrent_name: &'a str,
    pub torrent_size: Option<&'a str>,
}

pub async fn list_for_user(state: &AppState, user: &AuthUser) -> AppResult<Vec<Track>> {
    let tracks = if user.is_admin() {
        sqlx::query_as::<_, Track>("SELECT * FROM tracks ORDER BY created_at DESC")
            .fetch_all(&state.db)
            .await?
    } else {
        sqlx::query_as::<_, Track>(
            "SELECT * FROM tracks WHERE user_id = ? ORDER BY created_at DESC",
        )
        .bind(user.id.to_string())
        .fetch_all(&state.db)
        .await?
    };
    Ok(tracks)
}

pub async fn get_track(state: &AppState, user: &AuthUser, id: &str) -> AppResult<Track> {
    let track = sqlx::query_as::<_, Track>("SELECT * FROM tracks WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("track not found".into()))?;

    if !user.is_admin() && track.user_id != user.id.to_string() {
        return Err(AppError::Forbidden("not your track".into()));
    }
    Ok(track)
}

pub async fn create_track(
    state: &AppState,
    user: &AuthUser,
    req: CreateTrackRequest,
) -> AppResult<Track> {
    if req.media_type != "movie" && req.media_type != "tv" {
        return Err(AppError::BadRequest(
            "media_type must be movie or tv".into(),
        ));
    }
    if req.media_type == "movie" && (req.season_number.is_some() || req.episode_number.is_some()) {
        return Err(AppError::BadRequest("电影不按季或集追踪".into()));
    }
    if req.media_type == "tv" && req.season_number.is_none() {
        return Err(AppError::BadRequest(
            "电视剧需要指定季（season_number）".into(),
        ));
    }
    if let Some(season) = req.season_number {
        if season < 0 {
            return Err(AppError::BadRequest("season_number 不能为负".into()));
        }
    }
    if let Some(episode) = req.episode_number {
        if req.season_number.is_none() {
            return Err(AppError::BadRequest("按集追踪时必须同时指定季".into()));
        }
        if episode < 1 {
            return Err(AppError::BadRequest("episode_number 必须 >= 1".into()));
        }
    }

    let clients = state.clients().await;
    let media = if req.media_type == "movie" {
        clients.tmdb.movie_details(req.tmdb_id).await?
    } else {
        clients.tmdb.tv_details(req.tmdb_id).await?
    };

    if req.media_type == "tv" {
        let season = req.season_number.expect("tv season checked");
        if !media.seasons.is_empty() && !media.seasons.iter().any(|s| s.season_number == season) {
            return Err(AppError::BadRequest(format!("没有第 {season} 季")));
        }
        if let Some(episode) = req.episode_number {
            if let Some(info) = media.seasons.iter().find(|s| s.season_number == season) {
                if info.episode_count > 0 && episode > info.episode_count {
                    return Err(AppError::BadRequest(format!(
                        "第 {season} 季没有第 {episode} 集"
                    )));
                }
            }
        }
    }

    // A season can be missing even when the show is already in the library.
    let mut presence = crate::infra::media_server::LibraryPresence {
        found: false,
        matches: Vec::new(),
    };
    if req.season_number.is_none() {
        let mut local_matches = crate::domain::maintenance::find_in_local_catalog(
            &state.db,
            &req.media_type,
            req.tmdb_id,
            media.imdb_id.as_deref(),
        )
        .await
        .unwrap_or_default();

        let settings = state.settings().await;
        presence = crate::infra::media_server::find_in_libraries(
            &state.http,
            &settings,
            &req.media_type,
            req.tmdb_id,
            media.imdb_id.as_deref(),
        )
        .await
        .unwrap_or_else(|e| {
            tracing::warn!("library presence check failed: {e}");
            crate::infra::media_server::LibraryPresence {
                found: false,
                matches: Vec::new(),
            }
        });
        if !local_matches.is_empty() {
            presence.matches.append(&mut local_matches);
            presence.found = true;
        }
    }

    let (status, progress, status_message) = if presence.found {
        let detail = presence
            .matches
            .iter()
            .map(|m| {
                format!(
                    "{}「{}」via {}",
                    m.source,
                    m.title,
                    m.matched_by.to_uppercase()
                )
            })
            .collect::<Vec<_>>()
            .join("; ");
        (
            "already_exists",
            1.0_f64,
            format!("Already in library: {detail}"),
        )
    } else {
        (
            "pending",
            0.0_f64,
            match req.season_number {
                Some(season) => format!(
                    "已加入队列 · {}",
                    crate::infra::tv_release::target_label(season, req.episode_number)
                ),
                None => "Queued for automation".into(),
            },
        )
    };

    let id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO tracks (
            id, user_id, media_type, tmdb_id, season_number, episode_number,
            title, original_title, year,
            poster_path, overview, imdb_id, status, progress, status_message
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&id)
    .bind(user.id.to_string())
    .bind(&req.media_type)
    .bind(req.tmdb_id)
    .bind(req.season_number)
    .bind(req.episode_number)
    .bind(&media.title)
    .bind(&media.original_title)
    .bind(&media.year)
    .bind(&media.poster_url)
    .bind(&media.overview)
    .bind(&media.imdb_id)
    .bind(status)
    .bind(progress)
    .bind(&status_message)
    .execute(&state.db)
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(db) = &e {
            if db.message().contains("UNIQUE") {
                return AppError::Conflict("已经在追踪这一部、这一季或这一集".into());
            }
        }
        AppError::from(e)
    })?;

    get_track(state, user, &id).await
}

pub async fn cancel_track(state: &AppState, user: &AuthUser, id: &str) -> AppResult<Track> {
    let track = get_track(state, user, id).await?;
    if track.status == "completed" {
        return Err(AppError::BadRequest("track already completed".into()));
    }
    sqlx::query(
        "UPDATE tracks SET status = 'failed', error_message = 'cancelled by user', status_message = 'Cancelled', updated_at = datetime('now') WHERE id = ?",
    )
    .bind(id)
    .execute(&state.db)
    .await?;
    get_track(state, user, id).await
}

/// Manually trigger one automation step for a track (search / download / …).
pub async fn trigger_track(state: &AppState, user: &AuthUser, id: &str) -> AppResult<Track> {
    let track = get_track(state, user, id).await?;
    match track.status.as_str() {
        "completed" | "already_exists" => {
            return Err(AppError::BadRequest("track already finished".into()));
        }
        // Re-queue searchable / failed items so the pipeline starts from search.
        "failed" | "pending" | "searching" => {
            update_status(&state.db, id, "pending", 0.0, Some("已手动触发"), None).await?;
        }
        _ => {}
    }

    let track = get_track(state, user, id).await?;
    // Soft-fail: pipeline writes status/error on the track; always return latest row.
    let _ = crate::worker::run_one_track(state, &track).await;
    get_track(state, user, id).await
}

/// Find an existing track for this media (any status), if the user owns it / admin.
pub async fn find_for_media(
    state: &AppState,
    user: &AuthUser,
    media_type: &str,
    tmdb_id: i64,
) -> AppResult<Vec<Track>> {
    let tracks = if user.is_admin() {
        sqlx::query_as::<_, Track>(
            "SELECT * FROM tracks WHERE media_type = ? AND tmdb_id = ? ORDER BY season_number, episode_number, created_at",
        )
        .bind(media_type)
        .bind(tmdb_id)
        .fetch_all(&state.db)
        .await?
    } else {
        sqlx::query_as::<_, Track>(
            "SELECT * FROM tracks WHERE user_id = ? AND media_type = ? AND tmdb_id = ? ORDER BY season_number, episode_number, created_at",
        )
        .bind(user.id.to_string())
        .bind(media_type)
        .bind(tmdb_id)
        .fetch_all(&state.db)
        .await?
    };
    Ok(tracks)
}

/// Episode row already pointed at a chosen torrent. `Ok(None)` when that episode is tracked.
pub async fn insert_episode_track(
    db: &sqlx::SqlitePool,
    parent: &Track,
    ep: EpisodeTorrent<'_>,
) -> AppResult<Option<Track>> {
    let Some(season) = parent.season_number else {
        return Err(AppError::Internal(
            "episode follow-up requires a season".into(),
        ));
    };
    let id = Uuid::new_v4().to_string();
    let label = crate::infra::tv_release::target_label(season, Some(ep.episode_number));
    let inserted = sqlx::query(
        r#"
        INSERT INTO tracks (
            id, user_id, media_type, tmdb_id, season_number, episode_number,
            title, original_title, year, poster_path, overview, imdb_id,
            status, progress, status_message,
            torrent_site, torrent_id, torrent_name, torrent_size
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'torrent_found', 0.05, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&id)
    .bind(&parent.user_id)
    .bind(&parent.media_type)
    .bind(parent.tmdb_id)
    .bind(season)
    .bind(ep.episode_number)
    .bind(&parent.title)
    .bind(&parent.original_title)
    .bind(&parent.year)
    .bind(&parent.poster_path)
    .bind(&parent.overview)
    .bind(&parent.imdb_id)
    .bind(format!("找到单集 {label}：{}", ep.torrent_name))
    .bind(ep.site)
    .bind(ep.torrent_id)
    .bind(ep.torrent_name)
    .bind(ep.torrent_size)
    .execute(db)
    .await;

    match inserted {
        Err(sqlx::Error::Database(db_err)) if db_err.message().contains("UNIQUE") => Ok(None),
        Err(e) => Err(e.into()),
        Ok(_) => {
            let track = sqlx::query_as::<_, Track>("SELECT * FROM tracks WHERE id = ?")
                .bind(&id)
                .fetch_one(db)
                .await?;
            Ok(Some(track))
        }
    }
}

pub async fn delete_track(state: &AppState, user: &AuthUser, id: &str) -> AppResult<()> {
    let _ = get_track(state, user, id).await?;
    sqlx::query("DELETE FROM tracks WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await?;
    Ok(())
}

pub async fn update_status(
    db: &sqlx::SqlitePool,
    id: &str,
    status: &str,
    progress: f64,
    message: Option<&str>,
    error: Option<&str>,
) -> AppResult<()> {
    sqlx::query(
        r#"
        UPDATE tracks SET
            status = ?,
            progress = ?,
            status_message = COALESCE(?, status_message),
            error_message = ?,
            updated_at = datetime('now')
        WHERE id = ?
        "#,
    )
    .bind(status)
    .bind(progress)
    .bind(message)
    .bind(error)
    .bind(id)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn set_torrent_info(
    db: &sqlx::SqlitePool,
    id: &str,
    torrent_site: &str,
    torrent_id: &str,
    torrent_name: &str,
    torrent_size: Option<&str>,
) -> AppResult<()> {
    sqlx::query(
        r#"
        UPDATE tracks SET
            torrent_site = ?,
            torrent_id = ?,
            torrent_name = ?,
            torrent_size = ?,
            status = 'torrent_found',
            status_message = ?,
            updated_at = datetime('now')
        WHERE id = ?
        "#,
    )
    .bind(torrent_site)
    .bind(torrent_id)
    .bind(torrent_name)
    .bind(torrent_size)
    .bind(format!("Found torrent on {torrent_site}: {torrent_name}"))
    .bind(id)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn set_download_info(
    db: &sqlx::SqlitePool,
    id: &str,
    hash: &str,
    download_path: Option<&str>,
    progress: f64,
) -> AppResult<()> {
    sqlx::query(
        r#"
        UPDATE tracks SET
            torrent_hash = ?,
            download_path = COALESCE(?, download_path),
            status = 'downloading',
            progress = ?,
            status_message = ?,
            updated_at = datetime('now')
        WHERE id = ?
        "#,
    )
    .bind(hash)
    .bind(download_path)
    .bind(progress)
    .bind(format!("Downloading {:.1}%", progress * 100.0))
    .bind(id)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn set_hardlink_path(db: &sqlx::SqlitePool, id: &str, path: &str) -> AppResult<()> {
    sqlx::query(
        r#"
        UPDATE tracks SET
            hardlink_path = ?,
            status = 'scanning',
            progress = 1.0,
            status_message = 'Hardlinked; triggering media library scan',
            updated_at = datetime('now')
        WHERE id = ?
        "#,
    )
    .bind(path)
    .bind(id)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn fetch_pending(db: &sqlx::SqlitePool) -> AppResult<Vec<Track>> {
    fetch_by_statuses(
        db,
        &[
            "pending",
            "searching",
            "torrent_found",
            "downloading",
            "hardlinking",
            "scanning",
        ],
    )
    .await
}

pub async fn fetch_search_queue(db: &sqlx::SqlitePool) -> AppResult<Vec<Track>> {
    fetch_by_statuses(db, &["pending", "searching", "torrent_found"]).await
}

pub async fn fetch_download_queue(db: &sqlx::SqlitePool) -> AppResult<Vec<Track>> {
    fetch_by_statuses(db, &["downloading", "hardlinking", "scanning"]).await
}

async fn fetch_by_statuses(db: &sqlx::SqlitePool, statuses: &[&str]) -> AppResult<Vec<Track>> {
    let placeholders = statuses.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
    let sql =
        format!("SELECT * FROM tracks WHERE status IN ({placeholders}) ORDER BY created_at ASC");
    let mut q = sqlx::query_as::<_, Track>(&sql);
    for status in statuses {
        q = q.bind(*status);
    }
    Ok(q.fetch_all(db).await?)
}
