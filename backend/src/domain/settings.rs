use crate::domain::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::infra::mdblist::MdblistClient;
use crate::infra::pt::PtRegistry;
use crate::infra::qbittorrent::QbittorrentClient;
use crate::infra::tmdb::TmdbClient;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct IntegrationSettings {
    #[serde(default)]
    pub tmdb_api_key: String,
    /// TMDB API language, e.g. `zh-CN`, `en-US`. Default Chinese.
    #[serde(default = "default_tmdb_language")]
    pub tmdb_language: String,
    /// MDBList key used to read IMDb and Rotten Tomatoes scores. Empty skips those scores.
    #[serde(default)]
    pub mdblist_api_key: String,
    /// Hardlink folder name template. Placeholders: `{{title}}`, `{{year}}`, `{{original_title}}`, `{{tmdb_id}}`.
    #[serde(default = "default_hardlink_folder_template")]
    pub hardlink_folder_template: String,
    #[serde(default)]
    pub mteam_api_key: String,
    #[serde(default = "default_mteam_base")]
    pub mteam_api_base: String,
    #[serde(default = "default_qb_url")]
    pub qb_url: String,
    #[serde(default = "default_qb_user")]
    pub qb_username: String,
    #[serde(default)]
    pub qb_password: String,
    #[serde(default = "default_qb_category")]
    pub qb_category: String,
    #[serde(default)]
    pub qb_save_path: String,
    #[serde(default)]
    pub plex_url: String,
    /// Account JWT from PIN/JWT flow (plex.tv); never shown in UI (redacted on GET).
    #[serde(default)]
    pub plex_token: String,
    #[serde(default)]
    pub plex_token_expires_at: String,
    /// PMS access token from `/api/v2/resources` (server-specific; redacted on GET).
    #[serde(default)]
    pub plex_server_token: String,
    /// Stable `X-Plex-Client-Identifier` (server-managed).
    #[serde(default)]
    pub plex_client_id: String,
    #[serde(default)]
    pub plex_device_private_key: String,
    #[serde(default)]
    pub plex_device_public_key: String,
    #[serde(default)]
    pub plex_device_kid: String,
    #[serde(default)]
    pub plex_pin_id: String,
    #[serde(default)]
    pub plex_pin_code: String,
    /// Derived for API responses; not persisted.
    #[serde(default)]
    pub plex_linked: bool,
    #[serde(default)]
    pub jellyfin_url: String,
    #[serde(default)]
    pub jellyfin_api_key: String,
    /// Preferred PT site id (`mteam`, …). Empty = search all configured sites.
    #[serde(default)]
    pub pt_primary_site: String,

    /// PT search interval (seconds). Default 3 hours. Min 60. Aligned from local midnight.
    #[serde(default = "default_search_interval")]
    pub schedule_search_interval_secs: u64,
    /// Deprecated: ignored. Kept for API/DB compatibility.
    #[serde(default)]
    pub schedule_search_at: String,

    /// Download / hardlink progress poll interval (seconds). Default 1 minute. Min 60.
    #[serde(default = "default_download_interval")]
    pub schedule_download_interval_secs: u64,
    /// Deprecated: ignored. Kept for API/DB compatibility.
    #[serde(default)]
    pub schedule_download_at: String,

    /// Plex/Jellyfin library scan + catalog sync interval (seconds). Default 1 day. Min 60.
    #[serde(default = "default_library_scan_interval")]
    pub schedule_library_scan_interval_secs: u64,
    /// Deprecated: ignored. Kept for API/DB compatibility.
    #[serde(default)]
    pub schedule_library_scan_at: String,
}

fn default_tmdb_language() -> String {
    "zh-CN".into()
}
fn default_hardlink_folder_template() -> String {
    "{{title}}-{{year}}".into()
}
fn default_mteam_base() -> String {
    "https://api.m-team.cc".into()
}
fn default_qb_url() -> String {
    "http://127.0.0.1:8081".into()
}
fn default_qb_user() -> String {
    "admin".into()
}
fn default_qb_category() -> String {
    "medianexus".into()
}
fn default_search_interval() -> u64 {
    3 * 60 * 60
}
fn default_download_interval() -> u64 {
    60
}
fn default_library_scan_interval() -> u64 {
    24 * 60 * 60
}

impl Default for IntegrationSettings {
    fn default() -> Self {
        Self {
            tmdb_api_key: String::new(),
            tmdb_language: default_tmdb_language(),
            mdblist_api_key: String::new(),
            hardlink_folder_template: default_hardlink_folder_template(),
            mteam_api_key: String::new(),
            mteam_api_base: default_mteam_base(),
            qb_url: default_qb_url(),
            qb_username: default_qb_user(),
            qb_password: String::new(),
            qb_category: default_qb_category(),
            qb_save_path: String::new(),
            plex_url: String::new(),
            plex_token: String::new(),
            plex_token_expires_at: String::new(),
            plex_server_token: String::new(),
            plex_client_id: String::new(),
            plex_device_private_key: String::new(),
            plex_device_public_key: String::new(),
            plex_device_kid: String::new(),
            plex_pin_id: String::new(),
            plex_pin_code: String::new(),
            plex_linked: false,
            jellyfin_url: String::new(),
            jellyfin_api_key: String::new(),
            pt_primary_site: String::new(),
            schedule_search_interval_secs: default_search_interval(),
            schedule_search_at: String::new(),
            schedule_download_interval_secs: default_download_interval(),
            schedule_download_at: String::new(),
            schedule_library_scan_interval_secs: default_library_scan_interval(),
            schedule_library_scan_at: String::new(),
        }
    }
}

impl IntegrationSettings {
    pub fn build_clients(&self, http: reqwest::Client) -> IntegrationClients {
        IntegrationClients {
            tmdb: TmdbClient::new(
                http.clone(),
                self.tmdb_api_key.clone(),
                self.tmdb_language.clone(),
            ),
            mdblist: MdblistClient::new(http.clone(), self.mdblist_api_key.clone()),
            pt: PtRegistry::from_settings(http.clone(), self),
            qb: QbittorrentClient::new(
                http,
                self.qb_url.clone(),
                self.qb_username.clone(),
                self.qb_password.clone(),
                self.qb_category.clone(),
                optional(&self.qb_save_path),
            ),
        }
    }
}

#[derive(Clone)]
pub struct IntegrationClients {
    pub tmdb: TmdbClient,
    pub mdblist: MdblistClient,
    pub pt: PtRegistry,
    pub qb: QbittorrentClient,
}

pub async fn load(db: &SqlitePool) -> AppResult<IntegrationSettings> {
    let rows: Vec<(String, String)> = sqlx::query_as("SELECT key, value FROM settings")
        .fetch_all(db)
        .await?;

    let mut s = IntegrationSettings::default();
    for (key, value) in rows {
        match key.as_str() {
            "tmdb_api_key" => s.tmdb_api_key = value,
            "tmdb_language" => {
                if !value.trim().is_empty() {
                    s.tmdb_language = value;
                }
            }
            "mdblist_api_key" => s.mdblist_api_key = value,
            "hardlink_folder_template" => {
                if !value.trim().is_empty() {
                    s.hardlink_folder_template = value;
                }
            }
            "mteam_api_key" => s.mteam_api_key = value,
            "mteam_api_base" => {
                if !value.is_empty() {
                    s.mteam_api_base = value;
                }
            }
            "qb_url" => {
                if !value.is_empty() {
                    s.qb_url = value;
                }
            }
            "qb_username" => {
                if !value.is_empty() {
                    s.qb_username = value;
                }
            }
            "qb_password" => s.qb_password = value,
            "qb_category" => {
                if !value.is_empty() {
                    s.qb_category = value;
                }
            }
            "qb_save_path" => s.qb_save_path = value,
            "plex_url" => s.plex_url = value,
            "plex_token" => s.plex_token = value,
            "plex_token_expires_at" => s.plex_token_expires_at = value,
            "plex_server_token" => s.plex_server_token = value,
            "plex_client_id" => s.plex_client_id = value,
            "plex_device_private_key" => s.plex_device_private_key = value,
            "plex_device_public_key" => s.plex_device_public_key = value,
            "plex_device_kid" => s.plex_device_kid = value,
            "plex_pin_id" => s.plex_pin_id = value,
            "plex_pin_code" => s.plex_pin_code = value,
            "jellyfin_url" => s.jellyfin_url = value,
            "jellyfin_api_key" => s.jellyfin_api_key = value,
            "pt_primary_site" => s.pt_primary_site = value,
            "schedule_search_interval_secs" => {
                s.schedule_search_interval_secs = parse_u64(&value, default_search_interval());
            }
            "schedule_search_at" => s.schedule_search_at = normalize_clock(&value),
            "schedule_download_interval_secs" => {
                s.schedule_download_interval_secs = parse_u64(&value, default_download_interval());
            }
            "schedule_download_at" => s.schedule_download_at = normalize_clock(&value),
            "schedule_library_scan_interval_secs" => {
                s.schedule_library_scan_interval_secs =
                    parse_u64(&value, default_library_scan_interval());
            }
            "schedule_library_scan_at" => s.schedule_library_scan_at = normalize_clock(&value),
            _ => {}
        }
    }
    Ok(s)
}

pub async fn save(db: &SqlitePool, s: &IntegrationSettings) -> AppResult<()> {
    let search_interval = s.schedule_search_interval_secs.to_string();
    let download_interval = s.schedule_download_interval_secs.to_string();
    let library_interval = s.schedule_library_scan_interval_secs.to_string();

    let pairs = [
        ("tmdb_api_key", s.tmdb_api_key.as_str()),
        ("tmdb_language", s.tmdb_language.as_str()),
        ("mdblist_api_key", s.mdblist_api_key.as_str()),
        (
            "hardlink_folder_template",
            s.hardlink_folder_template.as_str(),
        ),
        ("mteam_api_key", s.mteam_api_key.as_str()),
        ("mteam_api_base", s.mteam_api_base.as_str()),
        ("qb_url", s.qb_url.as_str()),
        ("qb_username", s.qb_username.as_str()),
        ("qb_password", s.qb_password.as_str()),
        ("qb_category", s.qb_category.as_str()),
        ("qb_save_path", s.qb_save_path.as_str()),
        ("plex_url", s.plex_url.as_str()),
        ("plex_token", s.plex_token.as_str()),
        ("plex_token_expires_at", s.plex_token_expires_at.as_str()),
        ("plex_server_token", s.plex_server_token.as_str()),
        ("plex_client_id", s.plex_client_id.as_str()),
        (
            "plex_device_private_key",
            s.plex_device_private_key.as_str(),
        ),
        ("plex_device_public_key", s.plex_device_public_key.as_str()),
        ("plex_device_kid", s.plex_device_kid.as_str()),
        ("plex_pin_id", s.plex_pin_id.as_str()),
        ("plex_pin_code", s.plex_pin_code.as_str()),
        ("jellyfin_url", s.jellyfin_url.as_str()),
        ("jellyfin_api_key", s.jellyfin_api_key.as_str()),
        ("pt_primary_site", s.pt_primary_site.as_str()),
        ("schedule_search_interval_secs", search_interval.as_str()),
        ("schedule_search_at", s.schedule_search_at.as_str()),
        (
            "schedule_download_interval_secs",
            download_interval.as_str(),
        ),
        ("schedule_download_at", s.schedule_download_at.as_str()),
        (
            "schedule_library_scan_interval_secs",
            library_interval.as_str(),
        ),
        (
            "schedule_library_scan_at",
            s.schedule_library_scan_at.as_str(),
        ),
    ];

    for (key, value) in pairs {
        sqlx::query(
            "INSERT INTO settings (key, value) VALUES (?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind(key)
        .bind(value)
        .execute(db)
        .await?;
    }
    Ok(())
}

/// Seed default integration settings once when the settings table is empty.
pub async fn seed_defaults_if_empty(db: &SqlitePool) -> AppResult<()> {
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM settings")
        .fetch_one(db)
        .await?;
    if count.0 > 0 {
        return Ok(());
    }

    let s = IntegrationSettings {
        plex_client_id: uuid::Uuid::new_v4().to_string(),
        ..IntegrationSettings::default()
    };
    save(db, &s).await?;
    tracing::info!("Seeded default integration settings");
    Ok(())
}

pub async fn get_for_admin(db: &SqlitePool, actor: &AuthUser) -> AppResult<IntegrationSettings> {
    require_admin(actor)?;
    Ok(redact_plex_secrets(load(db).await?))
}

pub async fn update_for_admin(
    db: &SqlitePool,
    actor: &AuthUser,
    mut settings: IntegrationSettings,
) -> AppResult<IntegrationSettings> {
    require_admin(actor)?;
    if settings.mteam_api_base.trim().is_empty() {
        return Err(AppError::BadRequest("mteam_api_base required".into()));
    }
    if settings.qb_category.trim().is_empty() {
        return Err(AppError::BadRequest("qb_category required".into()));
    }
    // Plex auth secrets are managed by the PIN/JWT link flow, not the settings form.
    let current = load(db).await?;
    settings.plex_token = current.plex_token;
    settings.plex_token_expires_at = current.plex_token_expires_at;
    settings.plex_server_token = current.plex_server_token;
    settings.plex_client_id = current.plex_client_id;
    settings.plex_device_private_key = current.plex_device_private_key;
    settings.plex_device_public_key = current.plex_device_public_key;
    settings.plex_device_kid = current.plex_device_kid;
    settings.plex_pin_id = current.plex_pin_id;
    settings.plex_pin_code = current.plex_pin_code;

    let settings = normalize_schedule(settings)?;
    save(db, &settings).await?;
    Ok(redact_plex_secrets(load(db).await?))
}

fn redact_plex_secrets(mut s: IntegrationSettings) -> IntegrationSettings {
    s.plex_linked = !s.plex_token.trim().is_empty();
    s.plex_token.clear();
    s.plex_server_token.clear();
    s.plex_client_id.clear();
    s.plex_device_private_key.clear();
    s.plex_device_public_key.clear();
    s.plex_device_kid.clear();
    s.plex_pin_id.clear();
    s.plex_pin_code.clear();
    s
}

fn normalize_schedule(mut s: IntegrationSettings) -> AppResult<IntegrationSettings> {
    if s.plex_client_id.trim().is_empty() {
        s.plex_client_id = uuid::Uuid::new_v4().to_string();
    } else {
        s.plex_client_id = s.plex_client_id.trim().to_string();
    }
    if s.tmdb_language.trim().is_empty() {
        s.tmdb_language = default_tmdb_language();
    } else {
        s.tmdb_language = s.tmdb_language.trim().to_string();
    }
    if s.hardlink_folder_template.trim().is_empty() {
        s.hardlink_folder_template = default_hardlink_folder_template();
    } else {
        s.hardlink_folder_template = s.hardlink_folder_template.trim().to_string();
    }
    // Schedulers align to local midnight; minimum interval is 1 minute.
    s.schedule_search_interval_secs = s.schedule_search_interval_secs.max(60);
    s.schedule_download_interval_secs = s.schedule_download_interval_secs.max(60);
    s.schedule_library_scan_interval_secs = s.schedule_library_scan_interval_secs.max(60);
    // Legacy clock fields are ignored by the worker; clear on save.
    s.schedule_search_at.clear();
    s.schedule_download_at.clear();
    s.schedule_library_scan_at.clear();
    Ok(s)
}

fn normalize_clock(raw: &str) -> String {
    let t = raw.trim();
    if t.is_empty() {
        return String::new();
    }
    // Accept HTML time input `HH:MM:SS`.
    let parts: Vec<&str> = t.split(':').collect();
    if parts.len() >= 2 {
        if let (Ok(h), Ok(m)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
            if h < 24 && m < 60 {
                return format!("{h:02}:{m:02}");
            }
        }
    }
    t.to_string()
}

fn parse_u64(value: &str, default: u64) -> u64 {
    value
        .trim()
        .parse::<u64>()
        .ok()
        .filter(|&n| n > 0)
        .unwrap_or(default)
}

fn require_admin(actor: &AuthUser) -> AppResult<()> {
    if actor.is_admin() {
        Ok(())
    } else {
        Err(AppError::Forbidden("admin only".into()))
    }
}

fn optional(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}
