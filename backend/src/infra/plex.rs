//! Plex authentication (PIN + device JWT) and PMS request helpers.
//!
//! Spec: https://developer.plex.tv/pms/#section/API-Info/Authenticating-with-Plex

use crate::domain::settings::{self, IntegrationSettings};
use crate::error::{AppError, AppResult};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{Duration, Utc};
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, ACCEPT};
use reqwest::RequestBuilder;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

pub const PRODUCT: &str = "MediaNexus";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const PLATFORM: &str = "MediaNexus";
pub const DEVICE: &str = "MediaNexus";
pub const DEVICE_NAME: &str = "MediaNexus Server";

const PINS_URL: &str = "https://clients.plex.tv/api/v2/pins";
const NONCE_URL: &str = "https://clients.plex.tv/api/v2/auth/nonce";
const TOKEN_URL: &str = "https://clients.plex.tv/api/v2/auth/token";
const RESOURCES_URL: &str = "https://clients.plex.tv/api/v2/resources";
const SCOPE: &str = "username,email,friendly_name";

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct PlexLinkStart {
    pub auth_url: String,
    pub pin_code: String,
    pub pin_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct PlexLinkStatus {
    pub linked: bool,
    pub pending: bool,
    pub pin_code: Option<String>,
    pub auth_url: Option<String>,
    pub expires_at: Option<i64>,
}

/// Stable identity + JWT used as `X-Plex-Token`.
#[derive(Debug, Clone)]
pub struct PlexAuth {
    pub client_id: String,
    pub token: String,
}

impl PlexAuth {
    /// PMS auth: prefer per-server token from `/resources`.
    /// Current PMS builds still 401 account JWTs on library endpoints.
    pub fn from_settings(settings: &IntegrationSettings) -> Option<Self> {
        let client_id = settings.plex_client_id.trim();
        if client_id.is_empty() {
            return None;
        }
        let server = settings.plex_server_token.trim();
        let token = if !server.is_empty() && !server.starts_with("eyJ") {
            server
        } else {
            settings.plex_token.trim()
        };
        if token.is_empty() {
            return None;
        }
        Some(Self {
            client_id: client_id.to_string(),
            token: token.to_string(),
        })
    }

    fn account_auth(settings: &IntegrationSettings) -> Option<Self> {
        let token = settings.plex_token.trim();
        let client_id = settings.plex_client_id.trim();
        if token.is_empty() || client_id.is_empty() {
            return None;
        }
        Some(Self {
            client_id: client_id.to_string(),
            token: token.to_string(),
        })
    }

    pub fn headers(&self) -> AppResult<HeaderMap> {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        insert_header(&mut headers, "X-Plex-Token", &self.token)?;
        insert_header(&mut headers, "X-Plex-Client-Identifier", &self.client_id)?;
        insert_header(&mut headers, "X-Plex-Product", PRODUCT)?;
        insert_header(&mut headers, "X-Plex-Version", VERSION)?;
        insert_header(&mut headers, "X-Plex-Platform", PLATFORM)?;
        insert_header(&mut headers, "X-Plex-Device", DEVICE)?;
        insert_header(&mut headers, "X-Plex-Device-Name", DEVICE_NAME)?;
        Ok(headers)
    }

    pub fn apply(&self, req: RequestBuilder) -> AppResult<RequestBuilder> {
        Ok(req.headers(self.headers()?))
    }

    fn identity_headers(client_id: &str) -> AppResult<HeaderMap> {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        insert_header(&mut headers, "X-Plex-Client-Identifier", client_id)?;
        insert_header(&mut headers, "X-Plex-Product", PRODUCT)?;
        insert_header(&mut headers, "X-Plex-Version", VERSION)?;
        insert_header(&mut headers, "X-Plex-Platform", PLATFORM)?;
        insert_header(&mut headers, "X-Plex-Device", DEVICE)?;
        insert_header(&mut headers, "X-Plex-Device-Name", DEVICE_NAME)?;
        Ok(headers)
    }
}

#[derive(Debug, Clone)]
pub struct PlexClient {
    http: reqwest::Client,
    base: String,
    auth: PlexAuth,
}

impl PlexClient {
    pub fn try_from_settings(
        http: reqwest::Client,
        settings: &IntegrationSettings,
    ) -> Option<Self> {
        let base = settings.plex_url.trim();
        let auth = PlexAuth::from_settings(settings)?;
        if base.is_empty() {
            return None;
        }
        Some(Self {
            http,
            base: base.trim_end_matches('/').to_string(),
            auth,
        })
    }

    pub fn get(&self, path: &str) -> AppResult<RequestBuilder> {
        let url = if path.starts_with("http://") || path.starts_with("https://") {
            path.to_string()
        } else {
            format!("{}{}", self.base, path)
        };
        self.auth.apply(self.http.get(url))
    }

    pub async fn send_json<T: for<'de> Deserialize<'de>>(
        &self,
        req: RequestBuilder,
    ) -> AppResult<T> {
        let resp = req.send().await?;
        let status = resp.status();
        if status.as_u16() == 498 {
            return Err(AppError::BadRequest(
                "Plex JWT expired (HTTP 498); token refresh required".into(),
            ));
        }
        if status.as_u16() == 401 {
            return Err(AppError::BadRequest(
                "Plex authentication failed (HTTP 401)".into(),
            ));
        }
        let resp = resp.error_for_status()?;
        Ok(resp.json().await?)
    }

    pub async fn send_ok(&self, req: RequestBuilder) -> AppResult<()> {
        let resp = req.send().await?;
        let status = resp.status();
        if status.as_u16() == 498 {
            return Err(AppError::BadRequest(
                "Plex JWT expired (HTTP 498); token refresh required".into(),
            ));
        }
        if status.as_u16() == 401 {
            return Err(AppError::BadRequest(
                "Plex authentication failed (HTTP 401)".into(),
            ));
        }
        resp.error_for_status()?;
        Ok(())
    }

    /// Verify PMS URL + auth via `/library/sections`.
    pub async fn test_connection(&self) -> AppResult<()> {
        self.send_ok(self.get("/library/sections")?).await
    }
}

/// Ensure client id + Ed25519 device key exist in settings.
pub async fn ensure_device_identity(db: &SqlitePool) -> AppResult<IntegrationSettings> {
    let mut s = settings::load(db).await?;
    let mut dirty = false;

    if s.plex_client_id.trim().is_empty() {
        s.plex_client_id = uuid::Uuid::new_v4().to_string();
        dirty = true;
    }

    if s.plex_device_private_key.trim().is_empty()
        || s.plex_device_public_key.trim().is_empty()
        || s.plex_device_kid.trim().is_empty()
    {
        let (priv_b64, pub_b64, kid) = generate_device_keypair()?;
        s.plex_device_private_key = priv_b64;
        s.plex_device_public_key = pub_b64;
        s.plex_device_kid = kid;
        dirty = true;
    }

    if dirty {
        settings::save(db, &s).await?;
    }
    Ok(s)
}

/// Start PIN + JWK auth flow. Returns Auth App URL for the user.
pub async fn start_link(
    http: &reqwest::Client,
    db: &SqlitePool,
    forward_url: &str,
) -> AppResult<PlexLinkStart> {
    let s = ensure_device_identity(db).await?;
    let signing = signing_key_from_settings(&s)?;
    let verifying = signing.verifying_key();
    let jwk = public_jwk(&verifying, &s.plex_device_kid);

    let resp = http
        .post(PINS_URL)
        .headers(PlexAuth::identity_headers(&s.plex_client_id)?)
        .json(&serde_json::json!({
            "strong": true,
            "jwk": jwk,
        }))
        .send()
        .await?
        .error_for_status()?;

    let pin: PinCreateResponse = resp.json().await?;
    let pin_id = pin.id.ok_or_else(|| AppError::Internal("Plex PIN missing id".into()))?;
    let pin_code = pin
        .code
        .ok_or_else(|| AppError::Internal("Plex PIN missing code".into()))?;

    let mut s = settings::load(db).await?;
    s.plex_pin_id = pin_id.to_string();
    s.plex_pin_code = pin_code.clone();
    // Clear previous JWT until claim succeeds.
    s.plex_token.clear();
    s.plex_token_expires_at.clear();
    s.plex_server_token.clear();
    settings::save(db, &s).await?;

    let auth_url = build_auth_app_url(&s.plex_client_id, &pin_code, forward_url);
    Ok(PlexLinkStart {
        auth_url,
        pin_code,
        pin_id,
    })
}

/// Poll pending PIN; when claimed, store JWT and clear pin state.
pub async fn poll_link(http: &reqwest::Client, db: &SqlitePool) -> AppResult<PlexLinkStatus> {
    let s = ensure_device_identity(db).await?;
    if !s.plex_token.trim().is_empty() && s.plex_pin_id.trim().is_empty() {
        return Ok(status_from_settings(&s));
    }

    let pin_id = s.plex_pin_id.trim();
    if pin_id.is_empty() {
        return Ok(status_from_settings(&s));
    }

    // Docs Step 3: GET /pins/<id>?deviceJWT=<signedJWT>
    let device_jwt = sign_device_jwt(&s, None)?;
    let url = format!("{PINS_URL}/{pin_id}");
    let resp = match http
        .get(&url)
        .headers(PlexAuth::identity_headers(&s.plex_client_id)?)
        .query(&[("deviceJWT", device_jwt.as_str())])
        .send()
        .await
    {
        Ok(resp) => resp,
        // Transient network failure — keep pending so the client can retry.
        Err(err) => {
            tracing::warn!(error = %err, "Plex PIN poll request failed; keeping pending state");
            return Ok(status_from_settings(&s));
        }
    };

    let status = resp.status().as_u16();
    // Terminal client errors (expired / invalid JWT / unauthorized / gone, …).
    // Keep 429 for retry; clear other 4xx so login is not stuck.
    if (400..500).contains(&status) && status != 429 {
        tracing::warn!(status, "Plex PIN poll rejected; cleared pending link state");
        return clear_pending_pin(db).await;
    }
    if !resp.status().is_success() {
        // 5xx / unexpected — keep pending for the next poll tick.
        tracing::warn!(status, "Plex PIN poll temporary failure; keeping pending state");
        return Ok(status_from_settings(&s));
    }

    let body: PinPollResponse = match resp.json().await {
        Ok(body) => body,
        Err(err) => {
            tracing::warn!(error = %err, "Plex PIN poll JSON decode failed; keeping pending state");
            return Ok(status_from_settings(&s));
        }
    };
    if let Some(token) = body.auth_token.filter(|t| !t.trim().is_empty()) {
        let exp = jwt_exp_claim(&token).unwrap_or_else(|| {
            (Utc::now() + Duration::days(7)).timestamp()
        });
        let mut s = settings::load(db).await?;
        s.plex_token = token;
        s.plex_token_expires_at = exp.to_string();
        s.plex_pin_id.clear();
        s.plex_pin_code.clear();
        settings::save(db, &s).await?;
        tracing::info!(expires_at = exp, "Plex account linked via PIN/JWT");
        if let Err(err) = sync_server_access(http, db).await {
            tracing::warn!(error = %err, "Plex linked but failed to sync PMS server token");
        }
        return Ok(status_from_settings(&settings::load(db).await?));
    }

    Ok(status_from_settings(&settings::load(db).await?))
}

async fn clear_pending_pin(db: &SqlitePool) -> AppResult<PlexLinkStatus> {
    let mut s = settings::load(db).await?;
    s.plex_pin_id.clear();
    s.plex_pin_code.clear();
    settings::save(db, &s).await?;
    Ok(status_from_settings(&s))
}

pub async fn unlink(db: &SqlitePool) -> AppResult<PlexLinkStatus> {
    let mut s = settings::load(db).await?;
    s.plex_token.clear();
    s.plex_token_expires_at.clear();
    s.plex_server_token.clear();
    s.plex_pin_id.clear();
    s.plex_pin_code.clear();
    settings::save(db, &s).await?;
    Ok(status_from_settings(&s))
}

pub fn link_status(settings: &IntegrationSettings) -> PlexLinkStatus {
    status_from_settings(settings)
}

/// Refresh account JWT when near expiry, and keep PMS server token in sync.
pub async fn refresh_token_if_needed(
    http: &reqwest::Client,
    db: &SqlitePool,
) -> AppResult<bool> {
    let s = ensure_device_identity(db).await?;
    if s.plex_device_private_key.trim().is_empty() {
        return Ok(false);
    }
    // Need an established link (prior JWT) to refresh via nonce flow.
    if s.plex_token.trim().is_empty() && s.plex_pin_id.trim().is_empty() {
        return Ok(false);
    }
    // If still pending PIN, don't refresh.
    if !s.plex_pin_id.trim().is_empty() {
        return Ok(false);
    }

    let mut refreshed = false;
    if token_needs_refresh(&s) {
        let nonce = fetch_nonce(http, &s.plex_client_id).await?;
        let device_jwt = sign_device_jwt(&s, Some(&nonce))?;
        let resp = http
            .post(TOKEN_URL)
            .headers(PlexAuth::identity_headers(&s.plex_client_id)?)
            .json(&serde_json::json!({ "jwt": device_jwt }))
            .send()
            .await?
            .error_for_status()?;
        let body: TokenExchangeResponse = resp.json().await?;
        let token = body.auth_token.ok_or_else(|| {
            AppError::Internal("Plex token refresh returned empty auth_token".into())
        })?;
        let exp = jwt_exp_claim(&token)
            .unwrap_or_else(|| (Utc::now() + Duration::days(7)).timestamp());

        let mut s = settings::load(db).await?;
        s.plex_token = token;
        s.plex_token_expires_at = exp.to_string();
        settings::save(db, &s).await?;
        tracing::info!(expires_at = exp, "Plex JWT refreshed");
        refreshed = true;
    }

    let s = settings::load(db).await?;
    let server = s.plex_server_token.trim();
    if refreshed || server.is_empty() || server.starts_with("eyJ") {
        sync_server_access(http, db).await?;
    }
    Ok(refreshed)
}

/// Fetch per-server access token from plex.tv `/resources` for PMS library APIs.
pub async fn sync_server_access(http: &reqwest::Client, db: &SqlitePool) -> AppResult<()> {
    let s = settings::load(db).await?;
    let Some(account) = PlexAuth::account_auth(&s) else {
        return Ok(());
    };

    let resp = http
        .get(RESOURCES_URL)
        .headers(account.headers()?)
        .query(&[("includeHttps", "1"), ("includeRelay", "1")])
        .send()
        .await?
        .error_for_status()?;
    let resources: Vec<PlexResource> = resp.json().await?;
    let Some(server) = pick_plex_server(&resources, &s.plex_url) else {
        tracing::warn!("No owned Plex Media Server found in /resources");
        return Ok(());
    };
    let Some(server_token) = server
        .access_token
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty() && !t.starts_with("eyJ"))
    else {
        tracing::warn!(
            "Plex /resources did not return a usable PMS access token (empty or JWT-shaped)"
        );
        return Ok(());
    };

    let mut s = settings::load(db).await?;
    s.plex_server_token = server_token.to_string();
    if s.plex_url.trim().is_empty() {
        if let Some(uri) = pick_connection_uri(server) {
            s.plex_url = uri;
        }
    }
    settings::save(db, &s).await?;
    tracing::info!("Synced Plex PMS server token from /resources");
    Ok(())
}

fn pick_plex_server<'a>(
    resources: &'a [PlexResource],
    preferred_url: &str,
) -> Option<&'a PlexResource> {
    let servers: Vec<_> = resources
        .iter()
        .filter(|r| {
            r.provides
                .as_deref()
                .unwrap_or("")
                .split(',')
                .any(|p| p.trim() == "server")
        })
        .collect();
    if servers.is_empty() {
        return None;
    }

    let preferred = preferred_url.trim().trim_end_matches('/');
    if !preferred.is_empty() {
        if let Some(matched) = servers.iter().find(|s| {
            s.connections.iter().any(|c| {
                c.uri
                    .as_deref()
                    .map(|u| u.trim_end_matches('/') == preferred)
                    .unwrap_or(false)
            })
        }) {
            return Some(*matched);
        }
    }

    servers
        .iter()
        .find(|s| s.owned.unwrap_or(false))
        .or_else(|| servers.first())
        .copied()
}

fn pick_connection_uri(server: &PlexResource) -> Option<String> {
    let mut conns: Vec<_> = server.connections.iter().collect();
    conns.sort_by_key(|c| {
        (
            c.relay.unwrap_or(false),
            !c.local.unwrap_or(false),
            c.protocol.as_deref() != Some("http"),
        )
    });
    conns
        .into_iter()
        .find_map(|c| c.uri.as_ref())
        .map(|u| u.trim_end_matches('/').to_string())
}

fn status_from_settings(s: &IntegrationSettings) -> PlexLinkStatus {
    let linked = !s.plex_token.trim().is_empty();
    let pending = !s.plex_pin_id.trim().is_empty();
    let expires_at = s
        .plex_token_expires_at
        .trim()
        .parse::<i64>()
        .ok()
        .filter(|&t| t > 0);
    let auth_url = if pending {
        Some(build_auth_app_url(
            &s.plex_client_id,
            &s.plex_pin_code,
            "", // unused when only reconstructing for display; prefer stored flow
        ))
    } else {
        None
    };
    PlexLinkStatus {
        linked,
        pending,
        pin_code: if pending {
            Some(s.plex_pin_code.clone())
        } else {
            None
        },
        auth_url: if pending && !s.plex_pin_code.is_empty() {
            auth_url
        } else {
            None
        },
        expires_at,
    }
}

fn token_needs_refresh(s: &IntegrationSettings) -> bool {
    let exp = s.plex_token_expires_at.trim().parse::<i64>().unwrap_or(0);
    if exp <= 0 {
        return true;
    }
    let refresh_before = Utc::now() + Duration::days(1);
    exp <= refresh_before.timestamp()
}

async fn fetch_nonce(http: &reqwest::Client, client_id: &str) -> AppResult<String> {
    let resp = http
        .get(NONCE_URL)
        .headers(PlexAuth::identity_headers(client_id)?)
        .send()
        .await?
        .error_for_status()?;
    let body: NonceResponse = resp.json().await?;
    body.nonce
        .filter(|n| !n.is_empty())
        .ok_or_else(|| AppError::Internal("Plex nonce empty".into()))
}

fn generate_device_keypair() -> AppResult<(String, String, String)> {
    let mut seed = [0u8; 32];
    getrandom::fill(&mut seed)
        .map_err(|e| AppError::Internal(format!("getrandom failed: {e}")))?;
    let signing = SigningKey::from_bytes(&seed);
    let verifying = signing.verifying_key();
    let priv_bytes = signing.to_bytes();
    let pub_bytes = verifying.to_bytes();
    let kid = {
        let mut hasher = Sha256::new();
        hasher.update(priv_bytes);
        hasher.update(pub_bytes);
        hasher.finalize()[..16]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    };
    Ok((
        URL_SAFE_NO_PAD.encode(priv_bytes),
        URL_SAFE_NO_PAD.encode(pub_bytes),
        kid,
    ))
}

fn signing_key_from_settings(s: &IntegrationSettings) -> AppResult<SigningKey> {
    let bytes = URL_SAFE_NO_PAD
        .decode(s.plex_device_private_key.trim())
        .map_err(|e| AppError::Internal(format!("decode plex private key: {e}")))?;
    let arr: [u8; 32] = bytes
        .try_into()
        .map_err(|_| AppError::Internal("plex private key must be 32 bytes".into()))?;
    Ok(SigningKey::from_bytes(&arr))
}

fn public_jwk(verifying: &VerifyingKey, kid: &str) -> serde_json::Value {
    serde_json::json!({
        "kty": "OKP",
        "crv": "Ed25519",
        "x": URL_SAFE_NO_PAD.encode(verifying.as_bytes()),
        "kid": kid,
        "use": "sig",
        "alg": "EdDSA",
    })
}

fn sign_device_jwt(s: &IntegrationSettings, nonce: Option<&str>) -> AppResult<String> {
    let signing = signing_key_from_settings(s)?;
    let now = Utc::now();
    let mut payload = serde_json::json!({
        "aud": "plex.tv",
        "iss": s.plex_client_id,
        "iat": now.timestamp(),
        "exp": (now + Duration::minutes(5)).timestamp(),
        "scope": SCOPE,
    });
    if let Some(n) = nonce {
        payload["nonce"] = serde_json::Value::String(n.to_string());
    }

    let header = serde_json::json!({
        "alg": "EdDSA",
        "typ": "JWT",
        "kid": s.plex_device_kid,
    });

    let h = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header)?);
    let p = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload)?);
    let signing_input = format!("{h}.{p}");
    let sig = signing.sign(signing_input.as_bytes());
    let s = URL_SAFE_NO_PAD.encode(sig.to_bytes());
    Ok(format!("{signing_input}.{s}"))
}

fn build_auth_app_url(client_id: &str, pin_code: &str, forward_url: &str) -> String {
    // Fragment params use the same encoding as the Plex Auth App docs.
    let mut url = format!(
        "https://app.plex.tv/auth#?clientID={}&code={}&context%5Bdevice%5D%5Bproduct%5D={}",
        urlencoding::encode(client_id),
        urlencoding::encode(pin_code),
        urlencoding::encode(PRODUCT),
    );
    if !forward_url.trim().is_empty() {
        url.push_str("&forwardUrl=");
        url.push_str(&urlencoding::encode(forward_url.trim()));
    }
    url
}

fn jwt_exp_claim(token: &str) -> Option<i64> {
    let payload = token.split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD.decode(payload).ok()?;
    let v: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    v.get("exp")?.as_i64()
}

fn insert_header(headers: &mut HeaderMap, name: &str, value: &str) -> AppResult<()> {
    let name = HeaderName::try_from(name)
        .map_err(|e| AppError::Internal(format!("invalid Plex header name {name}: {e}")))?;
    let value = HeaderValue::from_str(value).map_err(|e| {
        AppError::Internal(format!("invalid Plex header value for {name}: {e}"))
    })?;
    headers.insert(name, value);
    Ok(())
}

#[derive(Debug, Deserialize)]
struct PinCreateResponse {
    id: Option<i64>,
    code: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PinPollResponse {
    #[serde(default, alias = "authToken")]
    auth_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NonceResponse {
    nonce: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TokenExchangeResponse {
    #[serde(default, alias = "authToken")]
    auth_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PlexResource {
    #[serde(default)]
    provides: Option<String>,
    #[serde(default)]
    owned: Option<bool>,
    #[serde(default, alias = "accessToken")]
    access_token: Option<String>,
    #[serde(default)]
    connections: Vec<PlexConnection>,
}

#[derive(Debug, Deserialize)]
struct PlexConnection {
    #[serde(default)]
    protocol: Option<String>,
    #[serde(default)]
    uri: Option<String>,
    #[serde(default)]
    local: Option<bool>,
    #[serde(default)]
    relay: Option<bool>,
}
