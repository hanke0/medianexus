use crate::error::{AppError, AppResult};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct QbittorrentClient {
    http: reqwest::Client,
    base: String,
    username: String,
    password: String,
    category: String,
    save_path: Option<String>,
    logged_in: Arc<Mutex<bool>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TorrentInfo {
    pub hash: String,
    pub name: String,
    pub progress: f64,
    pub state: String,
    pub save_path: String,
    pub content_path: String,
    pub category: String,
    pub dlspeed: i64,
    pub eta: i64,
    pub size: i64,
}

impl QbittorrentClient {
    pub fn new(
        http: reqwest::Client,
        base: String,
        username: String,
        password: String,
        category: String,
        save_path: Option<String>,
    ) -> Self {
        Self {
            http,
            base: base.trim_end_matches('/').to_string(),
            username,
            password,
            category,
            save_path,
            logged_in: Arc::new(Mutex::new(false)),
        }
    }

    pub fn category(&self) -> &str {
        &self.category
    }

    /// Verify WebUI URL + credentials by logging in.
    pub async fn test_connection(&self) -> AppResult<()> {
        if self.base.trim().is_empty() {
            return Err(AppError::BadRequest(
                "qBittorrent URL is not configured".into(),
            ));
        }
        self.invalidate().await;
        self.login().await?;
        Ok(())
    }

    async fn ensure_login(&self) -> AppResult<()> {
        {
            let guard = self.logged_in.lock().await;
            if *guard {
                return Ok(());
            }
        }
        self.login().await
    }

    async fn login(&self) -> AppResult<()> {
        let url = format!("{}/api/v2/auth/login", self.base);
        let body = format!(
            "username={}&password={}",
            urlencoding::encode(&self.username),
            urlencoding::encode(&self.password)
        );
        let resp = self
            .http
            .post(&url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("Referer", &self.base)
            .body(body)
            .send()
            .await?;

        let text = resp.text().await?;
        if text.trim() != "Ok." {
            return Err(AppError::Internal(format!(
                "qBittorrent login failed: {text}"
            )));
        }

        let mut guard = self.logged_in.lock().await;
        *guard = true;
        Ok(())
    }

    async fn invalidate(&self) {
        let mut guard = self.logged_in.lock().await;
        *guard = false;
    }

    pub async fn ensure_category(&self) -> AppResult<()> {
        self.ensure_login().await?;
        let url = format!("{}/api/v2/torrents/createCategory", self.base);
        let mut form = vec![("category", self.category.clone())];
        if let Some(path) = &self.save_path {
            form.push(("savePath", path.clone()));
        }
        let resp = self.http.post(&url).form(&form).send().await?;
        // 409 = already exists — fine
        if resp.status().as_u16() == 409 {
            return Ok(());
        }
        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            // Some versions return 200 even when exists; treat other errors as soft
            tracing::warn!("createCategory response: {text}");
        }
        Ok(())
    }

    /// `Ok(true)` when qBittorrent accepted the torrent. `Ok(false)` is the
    /// `Fails.` response, usually a duplicate already in the client.
    pub async fn add_torrent_file(&self, filename: &str, bytes: Vec<u8>) -> AppResult<bool> {
        self.ensure_login().await?;
        self.ensure_category().await?;

        let url = format!("{}/api/v2/torrents/add", self.base);
        let part = reqwest::multipart::Part::bytes(bytes)
            .file_name(filename.to_string())
            .mime_str("application/x-bittorrent")
            .map_err(|e| AppError::Internal(e.to_string()))?;

        let mut form = reqwest::multipart::Form::new()
            .part("torrents", part)
            .text("category", self.category.clone())
            .text("autoTMM", "false".to_string());

        if let Some(path) = &self.save_path {
            form = form.text("savepath", path.clone());
        }

        let resp = self.http.post(&url).multipart(form).send().await?;
        if resp.status().as_u16() == 403 {
            self.invalidate().await;
            return Err(AppError::Internal(
                "qBittorrent session expired, retry later".into(),
            ));
        }
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        let text = text.trim();
        if !status.is_success() {
            return Err(AppError::Internal(format!(
                "qBittorrent add failed ({status}): {text}"
            )));
        }
        if text == "Ok." || text.is_empty() {
            return Ok(true);
        }
        if text == "Fails." {
            tracing::warn!("qBittorrent add returned Fails. (maybe duplicate)");
            return Ok(false);
        }
        Err(AppError::Internal(format!(
            "qBittorrent add failed ({status}): {text}"
        )))
    }

    pub async fn torrent_by_name_hint(&self, name_hint: &str) -> AppResult<Option<TorrentInfo>> {
        self.ensure_login().await?;
        let url = format!("{}/api/v2/torrents/info", self.base);
        let resp = self
            .http
            .get(&url)
            .query(&[("category", self.category.as_str())])
            .send()
            .await?;

        if resp.status().as_u16() == 403 {
            self.invalidate().await;
            return Err(AppError::Internal(
                "qBittorrent session expired, retry later".into(),
            ));
        }

        let list: Vec<TorrentInfo> = resp.error_for_status()?.json().await?;
        let hint_lower = name_hint.to_lowercase();
        let found = list.into_iter().find(|t| {
            t.name.to_lowercase().contains(&hint_lower)
                || hint_lower.contains(&t.name.to_lowercase())
        });
        Ok(found)
    }

    pub async fn torrent_by_hash(&self, hash: &str) -> AppResult<Option<TorrentInfo>> {
        self.ensure_login().await?;
        let url = format!("{}/api/v2/torrents/info", self.base);
        let resp = self
            .http
            .get(&url)
            .query(&[("hashes", hash)])
            .send()
            .await?;

        if resp.status().as_u16() == 403 {
            self.invalidate().await;
            return Err(AppError::Internal(
                "qBittorrent session expired, retry later".into(),
            ));
        }

        let list: Vec<TorrentInfo> = resp.error_for_status()?.json().await?;
        Ok(list.into_iter().next())
    }

    pub async fn list_category(&self) -> AppResult<Vec<TorrentInfo>> {
        self.ensure_login().await?;
        let url = format!("{}/api/v2/torrents/info", self.base);
        let resp = self
            .http
            .get(&url)
            .query(&[("category", self.category.as_str())])
            .send()
            .await?;

        if resp.status().as_u16() == 403 {
            self.invalidate().await;
            return Err(AppError::Internal(
                "qBittorrent session expired, retry later".into(),
            ));
        }

        Ok(resp.error_for_status()?.json().await?)
    }
}
