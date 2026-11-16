use super::{PtIndexer, PtMediaKind, PtSearchQuery, PtTorrent};
use crate::error::{AppError, AppResult};
use async_trait::async_trait;
use serde::de::Deserializer;
use serde::Deserialize;

pub const SITE_ID: &str = "mteam";
pub const SITE_NAME: &str = "M-Team";

#[derive(Clone)]
pub struct MTeamIndexer {
    http: reqwest::Client,
    base: String,
    api_key: String,
}

impl MTeamIndexer {
    pub fn new(http: reqwest::Client, base: String, api_key: String) -> Self {
        Self {
            http,
            base: base.trim_end_matches('/').to_string(),
            api_key,
        }
    }

    fn headers(&self) -> reqwest::header::HeaderMap {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("x-api-key", self.api_key.parse().expect("api key header"));
        headers.insert(reqwest::header::ACCEPT, "application/json".parse().unwrap());
        // M-Team API gateway expects a site Origin (required since ~2025-10).
        headers.insert(
            reqwest::header::ORIGIN,
            "https://kp.m-team.cc".parse().unwrap(),
        );
        headers.insert(
            reqwest::header::REFERER,
            "https://kp.m-team.cc/".parse().unwrap(),
        );
        headers
    }

    async fn parse_envelope<T: for<'de> Deserialize<'de>>(
        resp: reqwest::Response,
    ) -> AppResult<ApiEnvelope<T>> {
        let status = resp.status();
        let bytes = resp.bytes().await?;
        match serde_json::from_slice::<ApiEnvelope<T>>(&bytes) {
            Ok(env) => Ok(env),
            Err(err) => {
                let snippet = String::from_utf8_lossy(&bytes);
                let snippet = snippet.chars().take(240).collect::<String>();
                Err(AppError::Internal(format!(
                    "M-Team bad response ({status}): {err}; body={snippet}"
                )))
            }
        }
    }

    fn mode_for(kind: PtMediaKind) -> &'static str {
        match kind {
            PtMediaKind::Movie => "movie",
            PtMediaKind::Tv => "tvshow",
            PtMediaKind::Any => "normal",
        }
    }

    /// Lightweight auth check: search with pageSize=1.
    pub async fn test_connection(&self) -> AppResult<()> {
        if self.api_key.trim().is_empty() {
            return Err(AppError::BadRequest(
                "M-Team API key is not configured".into(),
            ));
        }
        let _ = self.search_once("", None, "normal", 1).await?;
        Ok(())
    }
}

#[async_trait]
impl PtIndexer for MTeamIndexer {
    fn id(&self) -> &'static str {
        SITE_ID
    }

    fn name(&self) -> &'static str {
        SITE_NAME
    }

    fn configured(&self) -> bool {
        !self.api_key.trim().is_empty()
    }

    async fn search(&self, query: &PtSearchQuery) -> AppResult<Vec<PtTorrent>> {
        if !self.configured() {
            return Err(AppError::BadRequest(
                "M-Team API key is not configured".into(),
            ));
        }

        let mut hits = self
            .search_once(
                &query.keyword,
                query.imdb_id.as_deref(),
                Self::mode_for(query.kind),
                query.page_size,
            )
            .await?;

        if hits.is_empty() && query.imdb_id.is_some() {
            hits = self
                .search_once(
                    &query.keyword,
                    None,
                    Self::mode_for(query.kind),
                    query.page_size,
                )
                .await?;
        }
        if hits.is_empty() && query.kind != PtMediaKind::Any {
            hits = self
                .search_once(&query.keyword, None, "normal", query.page_size)
                .await?;
        }
        Ok(hits)
    }

    async fn download_torrent(&self, torrent_id: &str) -> AppResult<Vec<u8>> {
        if !self.configured() {
            return Err(AppError::BadRequest(
                "M-Team API key is not configured".into(),
            ));
        }

        let url = format!("{}/api/torrent/genDlToken", self.base);
        let form = reqwest::multipart::Form::new().text("id", torrent_id.to_string());
        let http_resp = self
            .http
            .post(&url)
            .headers(self.headers())
            .multipart(form)
            .send()
            .await?
            .error_for_status()?;
        let resp: ApiEnvelope<String> = Self::parse_envelope(http_resp).await?;

        if !resp.is_ok() {
            return Err(AppError::Internal(format!(
                "M-Team genDlToken failed: {}",
                resp.message.unwrap_or(resp.code)
            )));
        }

        let dl_url = resp
            .data
            .ok_or_else(|| AppError::Internal("M-Team returned empty download URL".into()))?;

        let bytes = self
            .http
            .get(&dl_url)
            .headers(self.headers())
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;

        Ok(bytes.to_vec())
    }
}

impl MTeamIndexer {
    async fn search_once(
        &self,
        keyword: &str,
        imdb_id: Option<&str>,
        mode: &str,
        page_size: u32,
    ) -> AppResult<Vec<PtTorrent>> {
        let url = format!("{}/api/torrent/search", self.base);

        let mut body = serde_json::json!({
            "pageNumber": 1,
            "pageSize": page_size,
            "mode": mode,
            "visible": 1,
        });

        if let Some(imdb) = imdb_id.filter(|s| !s.is_empty()) {
            let imdb_url = if imdb.starts_with("http") {
                imdb.to_string()
            } else {
                format!("https://www.imdb.com/title/{imdb}/")
            };
            body["imdb"] = serde_json::Value::String(imdb_url);
        } else if !keyword.is_empty() {
            body["keyword"] = serde_json::Value::String(keyword.to_string());
        }

        let http_resp = self
            .http
            .post(&url)
            .headers(self.headers())
            .json(&body)
            .send()
            .await?
            .error_for_status()?;
        let resp: ApiEnvelope<SearchData> = Self::parse_envelope(http_resp).await?;

        if !resp.is_ok() {
            return Err(AppError::Internal(format!(
                "M-Team search failed: {}",
                resp.message.unwrap_or(resp.code)
            )));
        }

        Ok(resp
            .data
            .and_then(|d| d.data)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|item| {
                let id = value_to_string(&item.id)?;
                Some(PtTorrent {
                    site: SITE_ID.into(),
                    id,
                    name: item.name.unwrap_or_default(),
                    size: value_to_string(&item.size.unwrap_or(serde_json::Value::Null))
                        .unwrap_or_default(),
                    seeders: item.status.as_ref().and_then(|s| value_to_string_opt(&s.seeders)),
                    leechers: item.status.as_ref().and_then(|s| value_to_string_opt(&s.leechers)),
                    category: item.category.as_ref().and_then(value_to_string),
                    discount: item
                        .status
                        .as_ref()
                        .and_then(|s| value_to_string_opt(&s.discount)),
                })
            })
            .collect())
    }
}

#[derive(Debug, Deserialize)]
struct ApiEnvelope<T> {
    #[serde(deserialize_with = "deserialize_code")]
    code: String,
    message: Option<String>,
    data: Option<T>,
}

impl<T> ApiEnvelope<T> {
    fn is_ok(&self) -> bool {
        self.code == "0" || self.code == "200"
    }
}

#[derive(Debug, Deserialize)]
struct SearchData {
    data: Option<Vec<SearchItem>>,
}

#[derive(Debug, Deserialize)]
struct SearchItem {
    id: serde_json::Value,
    name: Option<String>,
    size: Option<serde_json::Value>,
    category: Option<serde_json::Value>,
    status: Option<StatusBlock>,
}

#[derive(Debug, Deserialize)]
struct StatusBlock {
    seeders: Option<serde_json::Value>,
    leechers: Option<serde_json::Value>,
    discount: Option<serde_json::Value>,
}

fn deserialize_code<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(match serde_json::Value::deserialize(deserializer)? {
        serde_json::Value::String(s) => s,
        serde_json::Value::Number(n) => n.to_string(),
        other => other.to_string(),
    })
}

fn value_to_string(v: &serde_json::Value) -> Option<String> {
    match v {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn value_to_string_opt(v: &Option<serde_json::Value>) -> Option<String> {
    v.as_ref().and_then(value_to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_numeric_code_and_null_data() {
        let raw = r#"{"code":1,"data":null,"message":"key無效"}"#;
        let env: ApiEnvelope<SearchData> = serde_json::from_str(raw).unwrap();
        assert_eq!(env.code, "1");
        assert!(!env.is_ok());
        assert_eq!(env.message.as_deref(), Some("key無效"));
        assert!(env.data.is_none());
    }

    #[test]
    fn accepts_string_success_code() {
        let raw = r#"{"code":"0","message":"SUCCESS","data":{"data":[]}}"#;
        let env: ApiEnvelope<SearchData> = serde_json::from_str(raw).unwrap();
        assert!(env.is_ok());
    }
}
