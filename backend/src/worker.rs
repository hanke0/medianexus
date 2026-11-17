use crate::domain::settings::IntegrationSettings;
use crate::domain::track::{self, Track};
use crate::infra::{hardlink, media_server};
use crate::state::AppState;
use chrono::Local;
use std::path::PathBuf;
use std::time::Duration;

pub fn spawn(state: AppState) {
    tokio::spawn(search_loop(state.clone()));
    tokio::spawn(download_loop(state.clone()));
    tokio::spawn(library_scan_loop(state));
    tracing::info!(
        "MediaNexus schedulers started (search / download / library-scan; intervals from settings)"
    );
}

async fn search_loop(state: AppState) {
    let mut first = true;
    loop {
        wait_for_job(&state, |s| s.schedule_search_interval_secs, first, "search").await;
        first = false;
        if let Err(e) = run_search_tick(&state).await {
            tracing::error!("search scheduler error: {e}");
        }
    }
}

async fn download_loop(state: AppState) {
    let mut first = true;
    loop {
        wait_for_job(
            &state,
            |s| s.schedule_download_interval_secs,
            first,
            "download",
        )
        .await;
        first = false;
        if let Err(e) = run_download_tick(&state).await {
            tracing::error!("download scheduler error: {e}");
        }
    }
}

async fn library_scan_loop(state: AppState) {
    let mut first = true;
    loop {
        wait_for_job(
            &state,
            |s| s.schedule_library_scan_interval_secs,
            first,
            "library-scan",
        )
        .await;
        first = false;
        if let Err(e) = run_library_scan_tick(&state).await {
            tracing::error!("library-scan scheduler error: {e}");
        }
    }
}

/// Wait until the next local-midnight-aligned slot for `interval_secs` (min 60s).
/// Slots are 00:00, 00:00+interval, 00:00+2*interval, …
async fn wait_for_job(
    state: &AppState,
    interval: impl Fn(&IntegrationSettings) -> u64,
    first: bool,
    name: &str,
) {
    let settings = state.settings().await;
    let interval_secs = interval(&settings).max(60);
    let mut wait = secs_until_next_midnight_slot(interval_secs, Local::now());
    if wait == 0 && !first {
        wait = interval_secs;
    }
    tracing::info!(
        job = name,
        wait_secs = wait,
        interval_secs,
        "waiting for midnight-aligned slot"
    );
    if wait > 0 {
        tokio::time::sleep(Duration::from_secs(wait)).await;
    }
}

/// Seconds until the next slot measured from local midnight.
pub fn secs_until_next_midnight_slot(interval_secs: u64, now: chrono::DateTime<Local>) -> u64 {
    let interval = interval_secs.max(60);
    let midnight = now
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .expect("midnight")
        .and_local_timezone(Local)
        .single()
        .unwrap_or(now);
    let elapsed = (now - midnight).num_seconds().max(0) as u64;
    let rem = elapsed % interval;
    if rem == 0 {
        0
    } else {
        interval - rem
    }
}

/// Process all active tracks one step. Used by manual maintenance.
pub async fn run_once(state: &AppState) -> Result<usize, crate::error::AppError> {
    let tracks = track::fetch_pending(&state.db).await?;
    let n = tracks.len();
    for t in tracks {
        if let Err(e) = process_one(state, &t).await {
            fail_track(state, &t, &e).await;
        }
    }
    Ok(n)
}

/// Run one pipeline step for a single track (manual trigger from UI).
pub async fn run_one_track(state: &AppState, t: &Track) -> Result<(), crate::error::AppError> {
    if let Err(e) = process_one(state, t).await {
        fail_track(state, t, &e).await;
        return Err(crate::error::AppError::Internal(e.to_string()));
    }
    Ok(())
}

pub async fn run_search_tick(state: &AppState) -> Result<usize, crate::error::AppError> {
    let tracks = track::fetch_search_queue(&state.db).await?;
    let n = tracks.len();
    tracing::info!(count = n, "search scheduler tick");
    for t in tracks {
        if let Err(e) = process_one(state, &t).await {
            fail_track(state, &t, &e).await;
        }
    }
    Ok(n)
}

pub async fn run_download_tick(state: &AppState) -> Result<usize, crate::error::AppError> {
    let tracks = track::fetch_download_queue(&state.db).await?;
    let n = tracks.len();
    for t in tracks {
        if let Err(e) = process_one(state, &t).await {
            fail_track(state, &t, &e).await;
        }
    }
    Ok(n)
}

pub async fn run_library_scan_tick(state: &AppState) -> Result<(), crate::error::AppError> {
    tracing::info!("library-scan scheduler tick");
    let (catalog, notes) = crate::domain::maintenance::sync_library_catalog_inner(state).await?;
    let scan_msgs = crate::domain::maintenance::trigger_media_scan_inner(state).await?;
    tracing::info!(
        catalog,
        notes = ?notes,
        scans = ?scan_msgs,
        "library-scan completed"
    );
    Ok(())
}

async fn fail_track(state: &AppState, t: &Track, e: &crate::error::AppError) {
    tracing::error!(track_id = %t.id, title = %t.title, "pipeline error: {e}");
    let _ = track::update_status(
        &state.db,
        &t.id,
        "failed",
        t.progress,
        Some("Failed"),
        Some(&e.to_string()),
    )
    .await;
}

async fn process_one(state: &AppState, t: &Track) -> Result<(), crate::error::AppError> {
    match t.status.as_str() {
        "pending" | "searching" => search_and_submit(state, t).await,
        "torrent_found" => submit_to_qb(state, t).await,
        "downloading" => poll_download(state, t).await,
        "hardlinking" => do_hardlink(state, t).await,
        "scanning" => do_scan(state, t).await,
        _ => Ok(()),
    }
}

async fn search_and_submit(state: &AppState, t: &Track) -> Result<(), crate::error::AppError> {
    let _ = crate::infra::plex::refresh_token_if_needed(&state.http, &state.db).await;
    let _ = state.reload_integrations().await;

    // Re-check library before downloading (synced catalog + live Plex/Jellyfin).
    let mut local = crate::domain::maintenance::find_in_local_catalog(
        &state.db,
        &t.media_type,
        t.tmdb_id,
        t.imdb_id.as_deref(),
    )
    .await?;
    let settings = state.settings().await;
    let mut presence = media_server::find_in_libraries(
        &state.http,
        &settings,
        &t.media_type,
        t.tmdb_id,
        t.imdb_id.as_deref(),
    )
    .await?;
    if !local.is_empty() {
        presence.matches.append(&mut local);
        presence.found = true;
    }
    if presence.found {
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
        track::update_status(
            &state.db,
            &t.id,
            "already_exists",
            1.0,
            Some(&format!("Already in library: {detail}")),
            None,
        )
        .await?;
        return Ok(());
    }

    track::update_status(
        &state.db,
        &t.id,
        "searching",
        0.02,
        Some("Searching PT sites…"),
        None,
    )
    .await?;

    let keyword = build_keyword(t);
    let settings = state.settings().await;
    let preferred = optional_str(&settings.pt_primary_site);
    let clients = state.clients().await;
    let query = crate::infra::pt::PtSearchQuery {
        keyword: keyword.clone(),
        imdb_id: t.imdb_id.clone(),
        kind: crate::infra::pt::PtMediaKind::from_track_media_type(&t.media_type),
        page_size: 30,
    };

    let hits = match clients.pt.search(&query, preferred).await {
        Ok(hits) => hits,
        Err(e) => {
            // Keep pending so the next search schedule can retry.
            track::update_status(
                &state.db,
                &t.id,
                "pending",
                0.0,
                Some(&format!("PT 搜种失败：{e}；将在下个搜种周期重试")),
                Some(&e.to_string()),
            )
            .await?;
            return Ok(());
        }
    };
    let Some(best) = crate::infra::pt::pick_best_torrent(&hits) else {
        // Keep pending so the next search schedule can retry.
        track::update_status(
            &state.db,
            &t.id,
            "pending",
            0.0,
            Some(&format!(
                "未找到「{keyword}」相关种子；将在下个搜种周期重试"
            )),
            None,
        )
        .await?;
        return Ok(());
    };

    track::set_torrent_info(
        &state.db,
        &t.id,
        &best.site,
        &best.id,
        &best.name,
        Some(&best.size),
    )
    .await?;

    let refreshed = track::fetch_search_queue(&state.db)
        .await?
        .into_iter()
        .find(|x| x.id == t.id)
        .ok_or_else(|| crate::error::AppError::NotFound("track disappeared".into()))?;
    submit_to_qb(state, &refreshed).await
}

async fn submit_to_qb(state: &AppState, t: &Track) -> Result<(), crate::error::AppError> {
    let torrent_id = t
        .torrent_id
        .as_deref()
        .ok_or_else(|| crate::error::AppError::Internal("missing torrent_id".into()))?;
    let torrent_site = t
        .torrent_site
        .as_deref()
        .unwrap_or(crate::infra::pt::mteam::SITE_ID);
    let torrent_name = t
        .torrent_name
        .clone()
        .unwrap_or_else(|| "media.torrent".into());

    track::update_status(
        &state.db,
        &t.id,
        "downloading",
        0.05,
        Some(&format!("Downloading torrent file from {torrent_site}…")),
        None,
    )
    .await?;

    let clients = state.clients().await;
    let bytes = clients.pt.download(torrent_site, torrent_id).await?;
    let hashes = crate::infra::torrent_meta::info_hashes(&bytes)?;
    let filename = format!("{}.torrent", sanitize_filename(&torrent_name));
    let accepted = clients.qb.add_torrent_file(&filename, bytes).await?;

    let mut found = None;
    for attempt in 0..5 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        found = find_by_hashes(&clients.qb, &hashes).await?;
        if found.is_some() {
            break;
        }
    }

    let hash = found
        .as_ref()
        .map(|info| info.hash.clone())
        .unwrap_or_else(|| hashes.preferred().to_string());
    if hash.is_empty() {
        return Err(crate::error::AppError::Internal(
            "torrent file has no info hash".into(),
        ));
    }
    let path = found.as_ref().and_then(|info| {
        if info.content_path.is_empty() {
            None
        } else {
            Some(info.content_path.as_str())
        }
    });
    let progress = found.as_ref().map(|info| info.progress).unwrap_or(0.0);
    track::set_download_info(&state.db, &t.id, &hash, path, progress).await?;

    if found.is_none() && !accepted {
        return Err(crate::error::AppError::Internal(format!(
            "qBittorrent rejected the torrent and hash {hash} is not in the client"
        )));
    }
    if found.is_none() {
        track::update_status(
            &state.db,
            &t.id,
            "downloading",
            0.08,
            Some("Torrent added; waiting for qBittorrent to list it…"),
            None,
        )
        .await?;
    }
    Ok(())
}

async fn find_by_hashes(
    qb: &crate::infra::qbittorrent::QbittorrentClient,
    hashes: &crate::infra::torrent_meta::InfoHashes,
) -> Result<Option<crate::infra::qbittorrent::TorrentInfo>, crate::error::AppError> {
    for hash in hashes.candidates() {
        if let Some(info) = qb.torrent_by_hash(hash).await? {
            return Ok(Some(info));
        }
    }
    Ok(None)
}

async fn poll_download(state: &AppState, t: &Track) -> Result<(), crate::error::AppError> {
    // Older rows never stored an info-hash, so name matching could miss forever.
    let t = if t.torrent_hash.is_none() && t.torrent_id.is_some() {
        submit_to_qb(state, t).await?;
        sqlx::query_as::<_, Track>("SELECT * FROM tracks WHERE id = ?")
            .bind(&t.id)
            .fetch_one(&state.db)
            .await?
    } else {
        t.clone()
    };

    let qb = state.clients().await.qb;
    let mut info = if let Some(hash) = t.torrent_hash.as_deref() {
        qb.torrent_by_hash(hash).await?
    } else {
        None
    };
    if info.is_none() && t.torrent_hash.is_none() {
        if let Some(name) = t.torrent_name.as_deref() {
            info = qb.torrent_by_name_hint(name).await?;
        }
        if info.is_none() {
            info = qb.torrent_by_name_hint(t.title.as_str()).await?;
        }
    }

    let Some(info) = info else {
        track::update_status(
            &state.db,
            &t.id,
            "downloading",
            t.progress.max(0.08),
            Some("Waiting for qBittorrent to pick up torrent…"),
            None,
        )
        .await?;
        return Ok(());
    };

    let path = if info.content_path.is_empty() {
        None
    } else {
        Some(info.content_path.as_str())
    };
    track::set_download_info(&state.db, &t.id, &info.hash, path, info.progress).await?;

    let done = info.progress >= 0.999
        || matches!(
            info.state.as_str(),
            "uploading" | "stalledUP" | "pausedUP" | "queuedUP" | "checkingUP" | "forcedUP"
        );

    if done {
        track::update_status(
            &state.db,
            &t.id,
            "hardlinking",
            1.0,
            Some("Download complete; hardlinking…"),
            None,
        )
        .await?;
        let refreshed = sqlx::query_as::<_, Track>("SELECT * FROM tracks WHERE id = ?")
            .bind(&t.id)
            .fetch_one(&state.db)
            .await?;
        do_hardlink(state, &refreshed).await?;
    }
    Ok(())
}

async fn do_hardlink(state: &AppState, t: &Track) -> Result<(), crate::error::AppError> {
    let mut download_path = t.download_path.clone();

    if download_path
        .as_ref()
        .map(|p| !PathBuf::from(p).exists())
        .unwrap_or(true)
    {
        if let Some(hash) = t.torrent_hash.as_deref() {
            let qb = state.clients().await.qb;
            if let Some(info) = qb.torrent_by_hash(hash).await? {
                if !info.content_path.is_empty() {
                    track::set_download_info(
                        &state.db,
                        &t.id,
                        &info.hash,
                        Some(&info.content_path),
                        1.0,
                    )
                    .await?;
                    download_path = Some(info.content_path);
                }
            }
        }
    }

    let src = download_path
        .as_ref()
        .map(PathBuf::from)
        .ok_or_else(|| crate::error::AppError::Internal("missing download_path".into()))?;

    if !src.exists() {
        return Err(crate::error::AppError::Internal(format!(
            "download path does not exist yet: {}",
            src.display()
        )));
    }

    let settings = state.settings().await;
    let folder = hardlink::render_folder_name(
        &settings.hardlink_folder_template,
        &hardlink::FolderNameVars {
            title: &t.title,
            year: t.year.as_deref(),
            original_title: t.original_title.as_deref(),
            tmdb_id: t.tmdb_id,
        },
    );
    // movie → library/movies/… ; default (tv / other) → library/tv/…
    let dest_root = state
        .config
        .library_dir
        .join(hardlink::library_kind_dir(&t.media_type));
    let dest = hardlink::hardlink_media(&src, &dest_root, &folder).await?;
    track::set_hardlink_path(&state.db, &t.id, &dest.display().to_string()).await?;

    let refreshed = sqlx::query_as::<_, Track>("SELECT * FROM tracks WHERE id = ?")
        .bind(&t.id)
        .fetch_one(&state.db)
        .await?;
    do_scan(state, &refreshed).await
}

async fn do_scan(state: &AppState, t: &Track) -> Result<(), crate::error::AppError> {
    let settings = state.settings().await;
    let messages = media_server::trigger_scans(&state.http, &settings).await?;
    let msg = messages.join("; ");
    track::update_status(&state.db, &t.id, "completed", 1.0, Some(&msg), None).await?;
    tracing::info!(track_id = %t.id, title = %t.title, "pipeline completed");
    Ok(())
}

fn build_keyword(t: &Track) -> String {
    if let Some(orig) = &t.original_title {
        if !orig.is_empty() {
            return match &t.year {
                Some(year) => format!("{orig} {year}"),
                None => orig.clone(),
            };
        }
    }
    match &t.year {
        Some(year) => format!("{} {year}", t.title),
        None => t.title.clone(),
    }
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c => c,
        })
        .take(80)
        .collect()
}

fn optional_str(s: &str) -> Option<&str> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn midnight_slot_at_boundary_is_zero() {
        let now = Local
            .with_ymd_and_hms(2026, 11, 17, 0, 0, 0)
            .single()
            .unwrap();
        assert_eq!(secs_until_next_midnight_slot(3600, now), 0);
    }

    #[test]
    fn midnight_slot_waits_remainder() {
        let now = Local
            .with_ymd_and_hms(2026, 11, 17, 0, 10, 0)
            .single()
            .unwrap();
        assert_eq!(secs_until_next_midnight_slot(3600, now), 3000);
    }

    #[test]
    fn midnight_slot_clamps_min_interval() {
        let now = Local
            .with_ymd_and_hms(2026, 11, 17, 0, 0, 30)
            .single()
            .unwrap();
        // interval 1 is clamped to 60 → next slot at 00:01:00
        assert_eq!(secs_until_next_midnight_slot(1, now), 30);
    }
}
