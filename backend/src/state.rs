use crate::config::Config;
use crate::domain::settings::{IntegrationClients, IntegrationSettings};
use crate::error::AppResult;
use crate::infra::tmdb::MediaItem;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::Arc;
use chrono::{Local, TimeZone};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub config: Arc<Config>,
    pub http: reqwest::Client,
    integrations: Arc<RwLock<IntegrationBundle>>,
    browse_cache: Arc<RwLock<BrowseCache>>,
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
            browse_cache: Arc::new(RwLock::new(BrowseCache::default())),
        })
    }

    /// A browse page fetched during the current local day. It expires at the next local midnight.
    pub async fn cached_browse(&self, key: &str) -> Option<Vec<MediaItem>> {
        let now = unix_now();
        let mut cache = self.browse_cache.write().await;
        cache.get(key, now)
    }

    pub async fn remember_browse(&self, key: String, items: Vec<MediaItem>) {
        let now = unix_now();
        let expires_at = next_local_midnight_unix();
        let mut cache = self.browse_cache.write().await;
        cache.insert(key, items, expires_at, now);
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

const BROWSE_CACHE_CAP: usize = 128;

#[derive(Default)]
struct BrowseCache {
    pages: HashMap<String, CachedBrowse>,
}

struct CachedBrowse {
    expires_at: i64,
    items: Vec<MediaItem>,
}

impl BrowseCache {
    fn get(&mut self, key: &str, now: i64) -> Option<Vec<MediaItem>> {
        let fresh = self
            .pages
            .get(key)
            .is_some_and(|page| page.expires_at > now);
        if !fresh {
            self.pages.remove(key);
            return None;
        }
        self.pages.get(key).map(|page| page.items.clone())
    }

    fn insert(&mut self, key: String, items: Vec<MediaItem>, expires_at: i64, now: i64) {
        if self.pages.len() >= BROWSE_CACHE_CAP && !self.pages.contains_key(&key) {
            self.pages.retain(|_, page| page.expires_at > now);
        }
        if self.pages.len() >= BROWSE_CACHE_CAP && !self.pages.contains_key(&key) {
            if let Some(oldest) = self
                .pages
                .iter()
                .min_by_key(|(_, page)| page.expires_at)
                .map(|(key, _)| key.clone())
            {
                self.pages.remove(&oldest);
            }
        }
        self.pages
            .insert(key, CachedBrowse { expires_at, items });
    }
}

fn next_local_midnight_unix() -> i64 {
    next_local_midnight(Local::now()).timestamp()
}

fn next_local_midnight(now: chrono::DateTime<Local>) -> chrono::DateTime<Local> {
    let Some(next_day) = now.date_naive().succ_opt() else {
        return now + chrono::Duration::hours(24);
    };
    let Some(naive) = next_day.and_hms_opt(0, 0, 0) else {
        return now + chrono::Duration::hours(24);
    };
    match Local.from_local_datetime(&naive) {
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

    #[test]
    fn browse_page_expires_at_midnight_and_evicts_the_oldest() {
        let mut cache = BrowseCache::default();
        cache.insert("a".into(), Vec::new(), 1_030, 1_000);
        assert!(cache.get("a", 1_020).is_some());
        assert!(cache.get("a", 1_030).is_none());

        for i in 0..BROWSE_CACHE_CAP {
            cache.insert(format!("k{i}"), Vec::new(), 5_000 + i as i64, 4_000);
        }
        cache.insert("new".into(), Vec::new(), 9_000, 4_000);
        assert_eq!(cache.pages.len(), BROWSE_CACHE_CAP);
        assert!(cache.get("new", 8_000).is_some());
        assert!(cache.get("k0", 8_000).is_none());
    }

    #[test]
    fn next_midnight_is_the_following_local_day() {
        let now = Local
            .with_ymd_and_hms(2026, 11, 18, 23, 15, 0)
            .single()
            .expect("local time");
        let midnight = next_local_midnight(now);
        assert_eq!(midnight.date_naive(), now.date_naive().succ_opt().unwrap());
        assert_eq!(midnight.time(), chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap());
        assert!(midnight.timestamp() > now.timestamp());
    }
}
