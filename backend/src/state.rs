use crate::config::Config;
use crate::domain::settings::{IntegrationClients, IntegrationSettings};
use crate::error::AppResult;
use crate::infra::tmdb::MediaItem;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
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

    /// A recent browse page, if it is still inside `ttl`.
    pub async fn cached_browse(&self, key: &str, ttl: Duration) -> Option<Vec<MediaItem>> {
        let now = unix_now();
        let mut cache = self.browse_cache.write().await;
        cache.get(key, ttl, now)
    }

    pub async fn remember_browse(&self, key: String, items: Vec<MediaItem>) {
        let mut cache = self.browse_cache.write().await;
        cache.insert(key, items, unix_now());
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
const MAX_BROWSE_TTL: Duration = Duration::from_secs(3 * 60 * 60);

#[derive(Default)]
struct BrowseCache {
    pages: HashMap<String, CachedBrowse>,
}

struct CachedBrowse {
    stored_at: i64,
    items: Vec<MediaItem>,
}

impl BrowseCache {
    fn get(&mut self, key: &str, ttl: Duration, now: i64) -> Option<Vec<MediaItem>> {
        let fresh = self
            .pages
            .get(key)
            .is_some_and(|page| still_fresh(page.stored_at, now, ttl));
        if !fresh {
            self.pages.remove(key);
            return None;
        }
        self.pages.get(key).map(|page| page.items.clone())
    }

    fn insert(&mut self, key: String, items: Vec<MediaItem>, now: i64) {
        if self.pages.len() >= BROWSE_CACHE_CAP && !self.pages.contains_key(&key) {
            self.pages
                .retain(|_, page| still_fresh(page.stored_at, now, MAX_BROWSE_TTL));
        }
        if self.pages.len() >= BROWSE_CACHE_CAP && !self.pages.contains_key(&key) {
            if let Some(oldest) = self
                .pages
                .iter()
                .min_by_key(|(_, page)| page.stored_at)
                .map(|(key, _)| key.clone())
            {
                self.pages.remove(&oldest);
            }
        }
        self.pages.insert(key, CachedBrowse { stored_at: now, items });
    }
}

fn still_fresh(stored_at: i64, now: i64, ttl: Duration) -> bool {
    now.saturating_sub(stored_at) < ttl.as_secs() as i64
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
    fn browse_page_expires_and_evicts_the_oldest() {
        let mut cache = BrowseCache::default();
        cache.insert("a".into(), Vec::new(), 1_000);
        assert!(cache.get("a", Duration::from_secs(30), 1_020).is_some());
        assert!(cache.get("a", Duration::from_secs(30), 1_030).is_none());

        for i in 0..BROWSE_CACHE_CAP {
            cache.insert(format!("k{i}"), Vec::new(), 5_000 + i as i64);
        }
        cache.insert("new".into(), Vec::new(), 9_000);
        assert_eq!(cache.pages.len(), BROWSE_CACHE_CAP);
        assert!(cache.get("new", Duration::from_secs(60), 9_010).is_some());
        assert!(cache.get("k0", Duration::from_secs(10_000), 9_010).is_none());
    }
}
