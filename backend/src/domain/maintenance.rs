use crate::domain::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::infra::media_server::{self, CatalogItem};
use crate::state::AppState;
use crate::worker;
use chrono::Local;
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MaintenanceToolDto {
    pub id: String,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MaintenanceRunResult {
    pub tool: String,
    pub ok: bool,
    pub message: String,
    pub details: Vec<String>,
    pub count: Option<i64>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow, ToSchema)]
pub struct LibraryItemRow {
    pub id: String,
    pub source: String,
    pub media_type: String,
    pub external_id: String,
    pub title: String,
    pub year: Option<String>,
    pub tmdb_id: Option<i64>,
    pub imdb_id: Option<String>,
    pub synced_at: String,
}

pub fn list_tools() -> Vec<MaintenanceToolDto> {
    vec![
        MaintenanceToolDto {
            id: "sync_library_catalog".into(),
            name: "同步影视库目录".into(),
            description: "从 Plex / Jellyfin 拉取已有电影与电视剧（含 TMDB/IMDb ID）写入本地 SQLite，供查重与浏览。".into(),
        },
        MaintenanceToolDto {
            id: "run_track_scan".into(),
            name: "推进追踪流水线".into(),
            description: "对已追踪条目立即执行一轮：搜种 / 检查 qB 下载 / 硬链 / 入库扫描。请先在「发现」里添加追踪。".into(),
        },
        MaintenanceToolDto {
            id: "trigger_media_scan".into(),
            name: "触发影视库扫描".into(),
            description: "调用 Plex / Jellyfin 库刷新；后台也会按设置中的间隔/触发时间自动扫描并同步目录。".into(),
        },
    ]
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MaintenanceJob {
    pub tool: String,
    /// running | succeeded | failed
    pub status: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub result: Option<MaintenanceRunResult>,
    pub error: Option<String>,
}

/// Start a tool in the background; poll `list_jobs` for the outcome.
pub fn start_tool(state: &AppState, actor: &AuthUser, tool: &str) -> AppResult<MaintenanceJob> {
    if !actor.is_admin() {
        return Err(AppError::Forbidden("admin only".into()));
    }
    if !list_tools().iter().any(|t| t.id == tool) {
        return Err(AppError::BadRequest(format!("unknown tool: {tool}")));
    }

    let job = MaintenanceJob {
        tool: tool.to_string(),
        status: "running".into(),
        started_at: Local::now().to_rfc3339(),
        finished_at: None,
        result: None,
        error: None,
    };
    {
        let mut jobs = state.maintenance_jobs.lock().unwrap();
        if jobs.get(tool).is_some_and(|j| j.status == "running") {
            return Err(AppError::Conflict("该任务正在执行中".into()));
        }
        jobs.insert(tool.to_string(), job.clone());
    }

    let state = state.clone();
    let tool = tool.to_string();
    tokio::spawn(async move {
        // Inner task so a panic still marks the job failed instead of leaving it running.
        let inner = {
            let state = state.clone();
            let tool = tool.clone();
            tokio::spawn(async move { run_tool(&state, &tool).await })
        };
        let outcome = match inner.await {
            Ok(res) => res.map_err(|e| e.to_string()),
            Err(e) => Err(format!("task aborted: {e}")),
        };
        if let Err(e) = &outcome {
            tracing::error!(tool, "maintenance tool failed: {e}");
        }
        let mut jobs = state.maintenance_jobs.lock().unwrap();
        if let Some(job) = jobs.get_mut(&tool) {
            job.finished_at = Some(Local::now().to_rfc3339());
            match outcome {
                Ok(res) => {
                    job.status = "succeeded".into();
                    job.result = Some(res);
                }
                Err(e) => {
                    job.status = "failed".into();
                    job.error = Some(e);
                }
            }
        }
    });

    Ok(job)
}

pub fn list_jobs(state: &AppState, actor: &AuthUser) -> AppResult<Vec<MaintenanceJob>> {
    if !actor.is_admin() {
        return Err(AppError::Forbidden("admin only".into()));
    }
    Ok(state.maintenance_jobs.lock().unwrap().values().cloned().collect())
}

async fn run_tool(state: &AppState, tool: &str) -> AppResult<MaintenanceRunResult> {
    match tool {
        "sync_library_catalog" => sync_library_catalog(state).await,
        "run_track_scan" => run_track_scan(state).await,
        "trigger_media_scan" => trigger_media_scan(state).await,
        _ => Err(AppError::BadRequest(format!("unknown tool: {tool}"))),
    }
}

async fn sync_library_catalog(state: &AppState) -> AppResult<MaintenanceRunResult> {
    let (written, notes) = sync_library_catalog_inner(state).await?;
    Ok(MaintenanceRunResult {
        tool: "sync_library_catalog".into(),
        ok: true,
        message: format!("Synced {written} library items"),
        details: notes,
        count: Some(written),
    })
}

/// Sync remote Plex/Jellyfin catalog into SQLite. Used by maintenance and scheduler.
pub async fn sync_library_catalog_inner(state: &AppState) -> AppResult<(i64, Vec<String>)> {
    let _guard = state.catalog_sync_lock.lock().await;
    refresh_plex_jwt(state).await;
    let settings = state.settings().await;
    let (items, notes) = media_server::list_remote_catalog(&state.http, &settings).await?;

    // One transaction so readers keep seeing the old catalog until the new one is complete.
    let mut tx = state.db.begin().await?;
    sqlx::query("DELETE FROM library_items")
        .execute(&mut *tx)
        .await?;

    let mut written = 0i64;
    for item in &items {
        replace_library_item(&mut tx, item).await?;
        written += 1;
    }
    tx.commit().await?;
    Ok((written, notes))
}

async fn replace_library_item(db: &mut sqlx::SqliteConnection, item: &CatalogItem) -> AppResult<()> {
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO library_items (
            id, source, media_type, external_id, title, year, tmdb_id, imdb_id, synced_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))
        ON CONFLICT(source, external_id) DO UPDATE SET
            title = excluded.title,
            year = excluded.year,
            tmdb_id = excluded.tmdb_id,
            imdb_id = excluded.imdb_id,
            media_type = excluded.media_type,
            synced_at = datetime('now')
        "#,
    )
    .bind(&id)
    .bind(&item.source)
    .bind(&item.media_type)
    .bind(&item.external_id)
    .bind(&item.title)
    .bind(&item.year)
    .bind(item.tmdb_id)
    .bind(&item.imdb_id)
    .execute(db)
    .await?;
    Ok(())
}

async fn run_track_scan(state: &AppState) -> AppResult<MaintenanceRunResult> {
    let n = worker::run_once(state).await?;
    let (message, details) = if n == 0 {
        (
            "当前没有进行中的追踪".into(),
            vec![
                "请先在「发现」中把影片加入追踪。".into(),
                "已完成 / 失败的条目不会再被推进。".into(),
            ],
        )
    } else {
        (
            format!("已推进 {n} 条追踪"),
            vec!["本轮会按状态执行：搜种 / 下载检查 / 硬链 / 入库扫描。".into()],
        )
    };
    Ok(MaintenanceRunResult {
        tool: "run_track_scan".into(),
        ok: true,
        message,
        details,
        count: Some(n as i64),
    })
}

async fn trigger_media_scan(state: &AppState) -> AppResult<MaintenanceRunResult> {
    let messages = trigger_media_scan_inner(state).await?;
    Ok(MaintenanceRunResult {
        tool: "trigger_media_scan".into(),
        ok: true,
        message: "Media library scan triggered".into(),
        details: messages,
        count: None,
    })
}

pub async fn trigger_media_scan_inner(state: &AppState) -> AppResult<Vec<String>> {
    refresh_plex_jwt(state).await;
    let settings = state.settings().await;
    media_server::trigger_scans(&state.http, &settings).await
}

async fn refresh_plex_jwt(state: &AppState) {
    // May refresh JWT and/or sync PMS server token — always reload cached settings afterward.
    match crate::infra::plex::refresh_token_if_needed(&state.http, &state.db).await {
        Ok(_) => {
            if let Err(e) = state.reload_integrations().await {
                tracing::warn!("reload after plex token sync failed: {e}");
            }
        }
        Err(e) => tracing::warn!("plex jwt refresh failed: {e}"),
    }
}

pub async fn list_library_items(
    state: &AppState,
    actor: &AuthUser,
    media_type: Option<&str>,
    source: Option<&str>,
    limit: i64,
    offset: i64,
) -> AppResult<Vec<LibraryItemRow>> {
    if !actor.is_admin() {
        return Err(AppError::Forbidden("admin only".into()));
    }
    let limit = limit.clamp(1, 200);
    let offset = offset.max(0);

    let rows = match (media_type, source) {
        (Some(mt), Some(src)) => {
            sqlx::query_as::<_, LibraryItemRow>(
                r#"SELECT * FROM library_items
                   WHERE media_type = ? AND source = ?
                   ORDER BY title COLLATE NOCASE LIMIT ? OFFSET ?"#,
            )
            .bind(mt)
            .bind(src)
            .bind(limit)
            .bind(offset)
            .fetch_all(&state.db)
            .await?
        }
        (Some(mt), None) => {
            sqlx::query_as::<_, LibraryItemRow>(
                r#"SELECT * FROM library_items
                   WHERE media_type = ?
                   ORDER BY title COLLATE NOCASE LIMIT ? OFFSET ?"#,
            )
            .bind(mt)
            .bind(limit)
            .bind(offset)
            .fetch_all(&state.db)
            .await?
        }
        (None, Some(src)) => {
            sqlx::query_as::<_, LibraryItemRow>(
                r#"SELECT * FROM library_items
                   WHERE source = ?
                   ORDER BY title COLLATE NOCASE LIMIT ? OFFSET ?"#,
            )
            .bind(src)
            .bind(limit)
            .bind(offset)
            .fetch_all(&state.db)
            .await?
        }
        (None, None) => {
            sqlx::query_as::<_, LibraryItemRow>(
                r#"SELECT * FROM library_items
                   ORDER BY title COLLATE NOCASE LIMIT ? OFFSET ?"#,
            )
            .bind(limit)
            .bind(offset)
            .fetch_all(&state.db)
            .await?
        }
    };
    Ok(rows)
}

pub async fn library_item_count(state: &AppState) -> AppResult<i64> {
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM library_items")
        .fetch_one(&state.db)
        .await?;
    Ok(n)
}

/// Fast path for presence checks using the synced SQLite catalog.
pub async fn find_in_local_catalog(
    db: &sqlx::SqlitePool,
    media_type: &str,
    tmdb_id: i64,
    imdb_id: Option<&str>,
) -> AppResult<Vec<media_server::LibraryMatch>> {
    let mut matches = Vec::new();

    if let Some(imdb) = imdb_id.map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let rows: Vec<(String, String, String)> = sqlx::query_as(
            r#"SELECT source, title, imdb_id FROM library_items
               WHERE media_type = ? AND imdb_id = ?"#,
        )
        .bind(media_type)
        .bind(imdb)
        .fetch_all(db)
        .await?;
        for (source, title, id) in rows {
            matches.push(media_server::LibraryMatch {
                source,
                title,
                matched_by: "imdb".into(),
                external_id: Some(id),
            });
        }
    }

    if matches.is_empty() {
        let rows: Vec<(String, String, Option<i64>)> = sqlx::query_as(
            r#"SELECT source, title, tmdb_id FROM library_items
               WHERE media_type = ? AND tmdb_id = ?"#,
        )
        .bind(media_type)
        .bind(tmdb_id)
        .fetch_all(db)
        .await?;
        for (source, title, id) in rows {
            matches.push(media_server::LibraryMatch {
                source,
                title,
                matched_by: "tmdb".into(),
                external_id: id.map(|v| v.to_string()),
            });
        }
    }

    Ok(matches)
}
