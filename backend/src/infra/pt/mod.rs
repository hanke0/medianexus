//! Pluggable private-tracker (PT) indexers.
//!
//! Add a new site by:
//! 1. Implementing [`PtIndexer`] in `infra/pt/<site>.rs`
//! 2. Registering it in [`PtRegistry::from_settings`]

pub mod mteam;

use crate::domain::settings::IntegrationSettings;
use crate::error::{AppError, AppResult};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub use mteam::{MTeamIndexer, SITE_ID as MTEAM_SITE_ID};

/// Logical media category for PT search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PtMediaKind {
    Movie,
    Tv,
    Any,
}

impl PtMediaKind {
    pub fn from_track_media_type(media_type: &str) -> Self {
        match media_type {
            "tv" => Self::Tv,
            "movie" => Self::Movie,
            _ => Self::Any,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PtSearchQuery {
    pub keyword: String,
    pub imdb_id: Option<String>,
    pub kind: PtMediaKind,
    pub page_size: u32,
}

/// Normalized torrent result shared by all PT sites.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PtTorrent {
    /// Stable site id, e.g. `mteam`.
    pub site: String,
    pub id: String,
    pub name: String,
    pub size: String,
    pub seeders: Option<String>,
    pub leechers: Option<String>,
    pub category: Option<String>,
    pub discount: Option<String>,
}

#[async_trait]
pub trait PtIndexer: Send + Sync {
    /// Machine id used in DB / registry (e.g. `mteam`).
    fn id(&self) -> &'static str;
    /// Human-readable name.
    fn name(&self) -> &'static str;
    fn configured(&self) -> bool;

    async fn search(&self, query: &PtSearchQuery) -> AppResult<Vec<PtTorrent>>;
    async fn download_torrent(&self, torrent_id: &str) -> AppResult<Vec<u8>>;
}

/// Facade over one or more PT indexers.
#[derive(Clone)]
pub struct PtRegistry {
    indexers: Vec<Arc<dyn PtIndexer>>,
}

impl PtRegistry {
    pub fn from_settings(http: reqwest::Client, settings: &IntegrationSettings) -> Self {
        let mut indexers: Vec<Arc<dyn PtIndexer>> = Vec::new();

        // Register known sites here when adding new indexers.
        let mteam = MTeamIndexer::new(
            http,
            settings.mteam_api_base.clone(),
            settings.mteam_api_key.clone(),
        );
        indexers.push(Arc::new(mteam));

        Self { indexers }
    }

    pub fn list(&self) -> Vec<(&'static str, &'static str, bool)> {
        self.indexers
            .iter()
            .map(|i| (i.id(), i.name(), i.configured()))
            .collect()
    }

    pub fn get(&self, site: &str) -> AppResult<&dyn PtIndexer> {
        self.indexers
            .iter()
            .find(|i| i.id() == site)
            .map(|i| i.as_ref())
            .ok_or_else(|| AppError::BadRequest(format!("unknown PT site: {site}")))
    }

    /// Prefer configured primary site; otherwise first configured indexer.
    pub fn primary(&self, preferred: Option<&str>) -> AppResult<&dyn PtIndexer> {
        if let Some(id) = preferred.filter(|s| !s.is_empty()) {
            let indexer = self.get(id)?;
            if indexer.configured() {
                return Ok(indexer);
            }
        }
        self.indexers
            .iter()
            .find(|i| i.configured())
            .map(|i| i.as_ref())
            .ok_or_else(|| {
                AppError::BadRequest("no PT site configured (set M-Team API key in settings)".into())
            })
    }

    /// Search across all configured sites (or a single preferred site).
    pub async fn search(
        &self,
        query: &PtSearchQuery,
        preferred_site: Option<&str>,
    ) -> AppResult<Vec<PtTorrent>> {
        if let Some(site) = preferred_site.filter(|s| !s.is_empty()) {
            return self.get(site)?.search(query).await;
        }

        let mut all = Vec::new();
        let mut errors = Vec::new();
        for indexer in &self.indexers {
            if !indexer.configured() {
                continue;
            }
            match indexer.search(query).await {
                Ok(mut hits) => all.append(&mut hits),
                Err(e) => {
                    tracing::warn!(site = indexer.id(), "PT search failed: {e}");
                    errors.push(format!("{}: {e}", indexer.id()));
                }
            }
        }
        if all.is_empty() && !errors.is_empty() {
            return Err(AppError::Internal(format!(
                "all PT searches failed: {}",
                errors.join("; ")
            )));
        }
        Ok(all)
    }

    pub async fn download(&self, site: &str, torrent_id: &str) -> AppResult<Vec<u8>> {
        self.get(site)?.download_torrent(torrent_id).await
    }
}

pub fn pick_best_torrent(hits: &[PtTorrent]) -> Option<&PtTorrent> {
    hits.iter().max_by(|a, b| {
        let score = |t: &PtTorrent| -> (i32, i64, i64) {
            let free = match t.discount.as_deref() {
                Some("FREE") | Some("PERCENT_50") | Some("PERCENT_30") => 1,
                _ => 0,
            };
            let seeds = t
                .seeders
                .as_deref()
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or(0);
            let size = t.size.parse::<i64>().unwrap_or(0);
            (free, seeds, size)
        };
        score(a).cmp(&score(b))
    })
}
