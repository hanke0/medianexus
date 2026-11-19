use crate::config::Config;
use crate::domain::maintenance::MaintenanceJob;
use crate::domain::settings::{IntegrationClients, IntegrationSettings};
use crate::error::AppResult;
use crate::infra::tmdb::MediaItem;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::Arc;
use chrono::{Local, TimeZone};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{Mutex, RwLock};

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub config: Arc<Config>,
    pub http: reqwest::Client,
    integrations: Arc<RwLock<IntegrationBundle>>,
    /// Latest manual maintenance run per tool id.
    pub maintenance_jobs: Arc<std::sync::Mutex<HashMap<String, MaintenanceJob>>>,
    /// Serializes catalog syncs between manual runs and the scheduler.
    pub catalog_sync_lock: Arc<Mutex<()>>,
}

struct IntegrationBundle {
    settings: IntegrationSettings,
    clients: IntegrationClients,
}

impl AppState {
    pub async fn new(db: SqlitePool, config: Config) -> AppResult<Self> {
        let http = reqwest::Client::builder()
            .cookie_store(true)
            .user_agent("MediaNexus/0.1")
            .build()
            .expect("http client");

        let settings = crate::domain::settings::load(&db).await?;
        let clients = settings.build_clients(http.clone());

        Ok(Self {
            db,
            config: Arc::new(config),
            http,
            integrations: Arc::new(RwLock::new(IntegrationBundle { settings, clients })),
            maintenance_jobs: Arc::default(),
            catalog_sync_lock: Arc::default(),
        })
    }

    /// A browse page that has not yet passed the next scheduled refresh time (local midnight when off).
    pub async fn cached_browse(&self, key: &str) -> Option<Vec<MediaItem>> {
        load_browse(&self.db, key, unix_now()).await
    }

    pub async fn remember_browse(&self, key: String, items: Vec<MediaItem>) {
        let at = self
            .settings()
            .await
            .browse_warm_time()
            .unwrap_or(chrono::NaiveTime::MIN);
        let expires_at = next_local_time(Local::now(), at).timestamp();
        store_browse(&self.db, &key, &items, expires_at, unix_now()).await;
    }

    pub async fn settings(&self) -> IntegrationSettings {
        self.integrations.read().await.settings.clone()
    }

    pub async fn clients(&self) -> IntegrationClients {
        self.integrations.read().await.clients.clone()
    }

    pub async fn reload_integrations(&self) -> AppResult<()> {
        let settings = crate::domain::settings::load(&self.db).await?;
        let clients = settings.build_clients(self.http.clone());
        let mut guard = self.integrations.write().await;
        guard.settings = settings;
        guard.clients = clients;
        Ok(())
    }
}

/// Cache failures, including rows written by an older `MediaItem` shape, are treated as misses.
async fn load_browse(db: &SqlitePool, key: &str, now: i64) -> Option<Vec<MediaItem>> {
    let row: Option<String> =
        sqlx::query_scalar("SELECT items FROM browse_cache WHERE key = ? AND expires_at > ?")
            .bind(key)
            .bind(now)
            .fetch_optional(db)
            .await
            .inspect_err(|err| tracing::warn!(key, error = %err, "read browse cache failed"))
            .ok()?;
    serde_json::from_str(&row?)
        .inspect_err(|err| tracing::warn!(key, error = %err, "ignoring unreadable browse cache entry"))
        .ok()
}

async fn store_browse(db: &SqlitePool, key: &str, items: &[MediaItem], expires_at: i64, now: i64) {
    let json = match serde_json::to_string(items) {
        Ok(json) => json,
        Err(err) => {
            tracing::warn!(key, error = %err, "serialize browse cache failed");
            return;
        }
    };
    let result = async {
        sqlx::query("DELETE FROM browse_cache WHERE expires_at <= ?")
            .bind(now)
            .execute(db)
            .await?;
        sqlx::query(
            "INSERT INTO browse_cache (key, items, expires_at) VALUES (?, ?, ?) \
             ON CONFLICT(key) DO UPDATE SET items = excluded.items, expires_at = excluded.expires_at",
        )
        .bind(key)
        .bind(json)
        .bind(expires_at)
        .execute(db)
        .await
    }
    .await;
    if let Err(err) = result {
        tracing::warn!(key, error = %err, "write browse cache failed");
    }
}

/// The first local `at` strictly after `now` (today if still ahead, otherwise tomorrow).
pub fn next_local_time(
    now: chrono::DateTime<Local>,
    at: chrono::NaiveTime,
) -> chrono::DateTime<Local> {
    let today = now.date_naive();
    let day = if today.and_time(at) > now.naive_local() {
        today
    } else {
        let Some(next_day) = today.succ_opt() else {
            return now + chrono::Duration::hours(24);
        };
        next_day
    };
    match Local.from_local_datetime(&day.and_time(at)) {
        chrono::LocalResult::Single(dt) | chrono::LocalResult::Ambiguous(dt, _) => dt,
        chrono::LocalResult::None => now + chrono::Duration::hours(24),
    }
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn memory_db() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("memory db");
        sqlx::migrate!("./migrations").run(&pool).await.expect("migrate");
        pool
    }

    fn item(tmdb_id: i64) -> MediaItem {
        serde_json::from_value(serde_json::json!({
            "media_type": "movie",
            "tmdb_id": tmdb_id,
            "title": "t",
            "original_title": null,
            "year": null,
            "poster_url": null,
            "overview": null,
            "imdb_id": null,
            "popularity": null,
            "vote_average": null
        }))
        .expect("item")
    }

    #[tokio::test]
    async fn browse_page_survives_until_midnight_and_is_replaced() {
        let db = memory_db().await;
        store_browse(&db, "a", &[item(1)], 1_030, 1_000).await;
        assert_eq!(load_browse(&db, "a", 1_020).await.unwrap()[0].tmdb_id, 1);
        assert!(load_browse(&db, "a", 1_030).await.is_none());

        store_browse(&db, "a", &[item(2)], 2_000, 1_500).await;
        assert_eq!(load_browse(&db, "a", 1_600).await.unwrap()[0].tmdb_id, 2);
    }

    #[tokio::test]
    async fn unreadable_entry_is_a_miss_and_expired_rows_are_pruned() {
        let db = memory_db().await;
        sqlx::query("INSERT INTO browse_cache (key, items, expires_at) VALUES ('old', '[{\"legacy\":1}]', 5000)")
            .execute(&db)
            .await
            .unwrap();
        assert!(load_browse(&db, "old", 1_000).await.is_none());

        store_browse(&db, "stale", &[item(1)], 1_100, 1_000).await;
        store_browse(&db, "fresh", &[item(2)], 9_000, 2_000).await;
        let keys: Vec<String> = sqlx::query_scalar("SELECT key FROM browse_cache ORDER BY key")
            .fetch_all(&db)
            .await
            .unwrap();
        assert_eq!(keys, ["fresh", "old"]);
    }

    #[test]
    fn next_local_time_is_today_when_ahead_otherwise_tomorrow() {
        let at = chrono::NaiveTime::from_hms_opt(0, 1, 0).unwrap();
        let late = Local.with_ymd_and_hms(2026, 11, 18, 23, 15, 0).single().unwrap();
        let next = next_local_time(late, at);
        assert_eq!(next.date_naive(), late.date_naive().succ_opt().unwrap());
        assert_eq!(next.time(), at);

        let early = Local.with_ymd_and_hms(2026, 11, 18, 0, 0, 30).single().unwrap();
        assert_eq!(next_local_time(early, at).date_naive(), early.date_naive());

        let exact = Local.with_ymd_and_hms(2026, 11, 18, 0, 1, 0).single().unwrap();
        assert_eq!(
            next_local_time(exact, at).date_naive(),
            exact.date_naive().succ_opt().unwrap()
        );
    }
}
