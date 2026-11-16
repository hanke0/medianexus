use crate::domain::settings::IntegrationSettings;
use crate::error::{AppError, AppResult};
use crate::infra::plex::PlexClient;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LibraryMatch {
    pub source: String,
    pub title: String,
    pub matched_by: String,
    pub external_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LibraryPresence {
    pub found: bool,
    pub matches: Vec<LibraryMatch>,
}

/// One title from a remote media server catalog (Plex / Jellyfin).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CatalogItem {
    pub source: String,
    pub media_type: String,
    pub external_id: String,
    pub title: String,
    pub year: Option<String>,
    pub tmdb_id: Option<i64>,
    pub imdb_id: Option<String>,
}

pub async fn trigger_scans(
    http: &reqwest::Client,
    settings: &IntegrationSettings,
) -> AppResult<Vec<String>> {
    let mut messages = Vec::new();

    if let Some(plex) = PlexClient::try_from_settings(http.clone(), settings) {
        match trigger_plex(&plex, None).await {
            Ok(msg) => messages.push(msg),
            Err(e) => {
                tracing::warn!("Plex scan failed: {e}");
                messages.push(format!("Plex scan failed: {e}"));
            }
        }
    }

    let jf_url = settings.jellyfin_url.trim();
    let jf_key = settings.jellyfin_api_key.trim();
    if !jf_url.is_empty() && !jf_key.is_empty() {
        match trigger_jellyfin(http, jf_url, jf_key).await {
            Ok(msg) => messages.push(msg),
            Err(e) => {
                tracing::warn!("Jellyfin scan failed: {e}");
                messages.push(format!("Jellyfin scan failed: {e}"));
            }
        }
    }

    if messages.is_empty() {
        messages.push("No media server configured; skipped library scan".into());
    }

    Ok(messages)
}

/// Look up media in configured Plex / Jellyfin libraries by IMDb then TMDB IDs.
pub async fn find_in_libraries(
    http: &reqwest::Client,
    settings: &IntegrationSettings,
    media_type: &str,
    tmdb_id: i64,
    imdb_id: Option<&str>,
) -> AppResult<LibraryPresence> {
    let mut matches = Vec::new();

    if let Some(plex) = PlexClient::try_from_settings(http.clone(), settings) {
        match find_in_plex(
            &plex,
            None,
            media_type,
            tmdb_id,
            imdb_id,
        )
        .await
        {
            Ok(found) => matches.extend(found),
            Err(e) => tracing::warn!("Plex library lookup failed: {e}"),
        }
    }

    let jf_url = settings.jellyfin_url.trim();
    let jf_key = settings.jellyfin_api_key.trim();
    if !jf_url.is_empty() && !jf_key.is_empty() {
        match find_in_jellyfin(http, jf_url, jf_key, media_type, tmdb_id, imdb_id).await {
            Ok(found) => matches.extend(found),
            Err(e) => tracing::warn!("Jellyfin library lookup failed: {e}"),
        }
    }

    Ok(LibraryPresence {
        found: !matches.is_empty(),
        matches,
    })
}

/// Pull full movie/TV catalogs from configured Plex / Jellyfin servers.
pub async fn list_remote_catalog(
    http: &reqwest::Client,
    settings: &IntegrationSettings,
) -> AppResult<(Vec<CatalogItem>, Vec<String>)> {
    let mut items = Vec::new();
    let mut notes = Vec::new();

    if let Some(plex) = PlexClient::try_from_settings(http.clone(), settings) {
        match list_plex_catalog(&plex, None).await {
            Ok(list) => {
                notes.push(format!("Plex: {} items", list.len()));
                items.extend(list);
            }
            Err(e) => notes.push(format!("Plex catalog failed: {e}")),
        }
    } else {
        notes.push("Plex not configured".into());
    }

    let jf_url = settings.jellyfin_url.trim();
    let jf_key = settings.jellyfin_api_key.trim();
    if !jf_url.is_empty() && !jf_key.is_empty() {
        match list_jellyfin_catalog(http, jf_url, jf_key).await {
            Ok(list) => {
                notes.push(format!("Jellyfin: {} items", list.len()));
                items.extend(list);
            }
            Err(e) => notes.push(format!("Jellyfin catalog failed: {e}")),
        }
    } else {
        notes.push("Jellyfin not configured".into());
    }

    Ok((items, notes))
}

/// Jellyfin 10.9+/12 reject legacy `X-Emby-Token`; use MediaBrowser Authorization.
fn jellyfin_authorization(api_key: &str) -> String {
    let token = api_key.trim().replace('"', "");
    format!(
        "MediaBrowser Token=\"{token}\", Client=\"MediaNexus\", Device=\"MediaNexus Server\", DeviceId=\"medianexus\", Version=\"{}\"",
        env!("CARGO_PKG_VERSION")
    )
}

/// Verify Jellyfin URL + API key via `/System/Info`.
pub async fn test_jellyfin(
    http: &reqwest::Client,
    base: &str,
    api_key: &str,
) -> AppResult<()> {
    let base = base.trim().trim_end_matches('/');
    let api_key = api_key.trim();
    if base.is_empty() {
        return Err(AppError::BadRequest("Jellyfin URL is not configured".into()));
    }
    if api_key.is_empty() {
        return Err(AppError::BadRequest(
            "Jellyfin API key is not configured".into(),
        ));
    }
    let url = format!("{base}/System/Info");
    let resp = http
        .get(&url)
        .header("Authorization", jellyfin_authorization(api_key))
        .send()
        .await?;
    let status = resp.status();
    if status.as_u16() == 401 {
        return Err(AppError::BadRequest(
            "Jellyfin authentication failed (HTTP 401)".into(),
        ));
    }
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        let snippet: String = body.chars().take(160).collect();
        return Err(AppError::Internal(format!(
            "Jellyfin connection failed ({status}): {snippet}"
        )));
    }
    Ok(())
}

async fn list_jellyfin_catalog(
    http: &reqwest::Client,
    base: &str,
    api_key: &str,
) -> AppResult<Vec<CatalogItem>> {
    let base = base.trim_end_matches('/');
    let mut out = Vec::new();
    for (include, media_type) in [("Movie", "movie"), ("Series", "tv")] {
        let url = format!("{base}/Items");
        let mut start = 0i32;
        let page_size = 200i32;
        loop {
            let resp = http
                .get(&url)
                .header("Authorization", jellyfin_authorization(api_key))
                .query(&[
                    ("Recursive", "true"),
                    ("IncludeItemTypes", include),
                    ("Fields", "ProviderIds,ProductionYear"),
                    ("StartIndex", &start.to_string()),
                    ("Limit", &page_size.to_string()),
                ])
                .send()
                .await?
                .error_for_status()?
                .json::<JellyfinItemsResponse>()
                .await?;

            let batch = resp.items.unwrap_or_default();
            if batch.is_empty() {
                break;
            }
            for item in batch {
                let external_id = item.id.clone().unwrap_or_else(|| {
                    format!("{media_type}-{}", item.name.as_deref().unwrap_or("x"))
                });
                let (tmdb_id, imdb_id) = extract_jellyfin_ids(&item);
                out.push(CatalogItem {
                    source: "jellyfin".into(),
                    media_type: media_type.into(),
                    external_id,
                    title: item.name.unwrap_or_else(|| "Unknown".into()),
                    year: item.production_year.map(|y| y.to_string()),
                    tmdb_id,
                    imdb_id,
                });
            }
            start += page_size;
            if i64::from(start) >= resp.total_record_count.unwrap_or(i64::MAX) {
                break;
            }
        }
    }
    Ok(out)
}

fn extract_jellyfin_ids(item: &JellyfinItem) -> (Option<i64>, Option<String>) {
    let mut tmdb = None;
    let mut imdb = None;
    if let Some(ids) = &item.provider_ids {
        for (k, v) in ids {
            if k.eq_ignore_ascii_case("Tmdb") {
                tmdb = v.parse().ok();
            } else if k.eq_ignore_ascii_case("Imdb") {
                imdb = Some(normalize_imdb(v)).filter(|s| !s.is_empty());
            }
        }
    }
    (tmdb, imdb)
}

async fn list_plex_catalog(
    plex: &PlexClient,
    library_id: Option<&str>,
) -> AppResult<Vec<CatalogItem>> {
    let sections = plex_sections(plex, library_id).await?;
    let mut out = Vec::new();

    for section in sections {
        let (type_num, media_type) = match section.section_type.as_deref() {
            Some("movie") => ("1", "movie"),
            Some("show") => ("2", "tv"),
            _ => continue,
        };
        let path = format!("/library/sections/{}/all", section.key);
        let mut start = 0i32;
        let size = 100i32;
        loop {
            let start_s = start.to_string();
            let size_s = size.to_string();
            let req = plex.get(&path)?.query(&[
                ("type", type_num),
                ("includeGuids", "1"),
                ("X-Plex-Container-Start", start_s.as_str()),
                ("X-Plex-Container-Size", size_s.as_str()),
            ]);
            let resp: PlexContainer = plex.send_json(req).await?;

            let container = resp.media_container.unwrap_or_default();
            let meta = container.metadata.unwrap_or_default();
            if meta.is_empty() {
                break;
            }
            for item in meta {
                let external_id = item
                    .rating_key
                    .clone()
                    .or_else(|| item.key.clone())
                    .unwrap_or_else(|| item.title.clone().unwrap_or_default());
                let (tmdb_id, imdb_id) = extract_plex_ids(&item);
                out.push(CatalogItem {
                    source: "plex".into(),
                    media_type: media_type.into(),
                    external_id,
                    title: item.title.unwrap_or_else(|| "Unknown".into()),
                    year: item.year.map(|y| y.to_string()),
                    tmdb_id,
                    imdb_id,
                });
            }
            start += size;
            let total = container.total_size.unwrap_or(start);
            if start >= total {
                break;
            }
        }
    }
    Ok(out)
}

fn extract_plex_ids(item: &PlexMetadata) -> (Option<i64>, Option<String>) {
    let mut tmdb = None;
    let mut imdb = None;
    for g in item.guid_list() {
        let lower = g.to_ascii_lowercase();
        if let Some(id) = strip_plex_provider(&lower, "imdb") {
            imdb = Some(normalize_imdb(id));
        } else if let Some(id) = strip_plex_provider(&lower, "tmdb")
            .or_else(|| strip_plex_provider(&lower, "themoviedb"))
        {
            tmdb = id.parse().ok();
        }
    }
    (tmdb, imdb.filter(|s| !s.is_empty()))
}

fn strip_plex_provider<'a>(guid: &'a str, provider: &str) -> Option<&'a str> {
    let prefixes = [
        format!("{provider}://"),
        format!("com.plexapp.agents.{provider}://"),
    ];
    for p in prefixes {
        if let Some(rest) = guid.strip_prefix(&p) {
            return Some(rest.split('?').next().unwrap_or(rest));
        }
    }
    None
}

async fn trigger_plex(plex: &PlexClient, library_id: Option<&str>) -> AppResult<String> {
    let path = if let Some(id) = library_id {
        format!("/library/sections/{id}/refresh")
    } else {
        "/library/sections/all/refresh".into()
    };
    plex.send_ok(plex.get(&path)?).await?;
    Ok("Plex library scan triggered".into())
}

async fn trigger_jellyfin(http: &reqwest::Client, base: &str, api_key: &str) -> AppResult<String> {
    let url = format!("{}/Library/Refresh", base.trim_end_matches('/'));
    http.post(&url)
        .header("Authorization", jellyfin_authorization(api_key))
        .send()
        .await?
        .error_for_status()?;
    Ok("Jellyfin library scan triggered".into())
}

async fn find_in_jellyfin(
    http: &reqwest::Client,
    base: &str,
    api_key: &str,
    media_type: &str,
    tmdb_id: i64,
    imdb_id: Option<&str>,
) -> AppResult<Vec<LibraryMatch>> {
    let base = base.trim_end_matches('/');
    let include = if media_type == "tv" {
        "Series"
    } else {
        "Movie"
    };
    let imdb = imdb_id.map(normalize_imdb).filter(|s| !s.is_empty());

    // Try provider filter first (works on Emby / some Jellyfin builds).
    let mut tries = Vec::new();
    if let Some(imdb) = &imdb {
        tries.push(("imdb", imdb.clone(), format!("Imdb.{imdb}")));
    }
    tries.push(("tmdb", tmdb_id.to_string(), format!("Tmdb.{tmdb_id}")));

    for (matched_by, external_id, provider_eq) in &tries {
        if let Some(hit) = jellyfin_try_provider_filter(
            http,
            base,
            api_key,
            include,
            matched_by,
            external_id,
            provider_eq,
            tmdb_id,
            imdb.as_deref(),
        )
        .await?
        {
            return Ok(vec![hit]);
        }
    }

    // Fallback: page and match ProviderIds client-side.
    jellyfin_scan_match(http, base, api_key, include, tmdb_id, imdb.as_deref()).await
}

#[allow(clippy::too_many_arguments)]
async fn jellyfin_try_provider_filter(
    http: &reqwest::Client,
    base: &str,
    api_key: &str,
    include: &str,
    matched_by: &str,
    external_id: &str,
    provider_eq: &str,
    tmdb_id: i64,
    imdb_id: Option<&str>,
) -> AppResult<Option<LibraryMatch>> {
    let url = format!("{base}/Items");
    let resp = http
        .get(&url)
        .header("Authorization", jellyfin_authorization(api_key))
        .query(&[
            ("Recursive", "true"),
            ("IncludeItemTypes", include),
            ("AnyProviderIdEquals", provider_eq),
            ("Fields", "ProviderIds"),
            ("Limit", "50"),
        ])
        .send()
        .await?
        .error_for_status()?
        .json::<JellyfinItemsResponse>()
        .await?;

    // Filter ignored → huge TotalRecordCount; fall through to scan.
    if resp.total_record_count.unwrap_or(0) > 50 {
        return Ok(None);
    }

    for item in resp.items.unwrap_or_default() {
        if let Some(m) = match_jellyfin_item(&item, tmdb_id, imdb_id) {
            if m.matched_by == matched_by || m.external_id.as_deref() == Some(external_id) {
                return Ok(Some(m));
            }
            return Ok(Some(m));
        }
    }
    Ok(None)
}

async fn jellyfin_scan_match(
    http: &reqwest::Client,
    base: &str,
    api_key: &str,
    include: &str,
    tmdb_id: i64,
    imdb_id: Option<&str>,
) -> AppResult<Vec<LibraryMatch>> {
    let url = format!("{base}/Items");
    let mut start = 0i32;
    let page_size = 200i32;
    for _ in 0..25 {
        let resp = http
            .get(&url)
            .header("Authorization", jellyfin_authorization(api_key))
            .query(&[
                ("Recursive", "true"),
                ("IncludeItemTypes", include),
                ("Fields", "ProviderIds"),
                ("StartIndex", &start.to_string()),
                ("Limit", &page_size.to_string()),
            ])
            .send()
            .await?
            .error_for_status()?
            .json::<JellyfinItemsResponse>()
            .await?;

        let items = resp.items.unwrap_or_default();
        if items.is_empty() {
            break;
        }
        for item in &items {
            if let Some(m) = match_jellyfin_item(item, tmdb_id, imdb_id) {
                return Ok(vec![m]);
            }
        }
        start += page_size;
        if i64::from(start) >= resp.total_record_count.unwrap_or(i64::MAX) {
            break;
        }
    }
    Ok(Vec::new())
}

fn match_jellyfin_item(
    item: &JellyfinItem,
    tmdb_id: i64,
    imdb_id: Option<&str>,
) -> Option<LibraryMatch> {
    if let Some(imdb) = imdb_id {
        if provider_ids_has(item, "Imdb", imdb) {
            return Some(LibraryMatch {
                source: "jellyfin".into(),
                title: item.name.clone().unwrap_or_else(|| "Unknown".into()),
                matched_by: "imdb".into(),
                external_id: Some(imdb.to_string()),
            });
        }
    }
    if provider_ids_has(item, "Tmdb", &tmdb_id.to_string()) {
        return Some(LibraryMatch {
            source: "jellyfin".into(),
            title: item.name.clone().unwrap_or_else(|| "Unknown".into()),
            matched_by: "tmdb".into(),
            external_id: Some(tmdb_id.to_string()),
        });
    }
    None
}

fn provider_ids_has(item: &JellyfinItem, key: &str, value: &str) -> bool {
    let Some(ids) = &item.provider_ids else {
        return false;
    };
    ids.iter()
        .any(|(k, v)| k.eq_ignore_ascii_case(key) && v.trim().eq_ignore_ascii_case(value.trim()))
}

async fn find_in_plex(
    plex: &PlexClient,
    library_id: Option<&str>,
    media_type: &str,
    tmdb_id: i64,
    imdb_id: Option<&str>,
) -> AppResult<Vec<LibraryMatch>> {
    let imdb = imdb_id.map(normalize_imdb).filter(|s| !s.is_empty());

    if let Some(imdb) = &imdb {
        for guid in [
            format!("imdb://{imdb}"),
            format!("com.plexapp.agents.imdb://{imdb}?lang=en"),
        ] {
            if let Some(hit) = plex_guid_search(plex, &guid, "imdb", imdb).await? {
                return Ok(vec![hit]);
            }
        }
    }
    for guid in [
        format!("tmdb://{tmdb_id}"),
        format!("themoviedb://{tmdb_id}"),
        format!("com.plexapp.agents.themoviedb://{tmdb_id}?lang=en"),
    ] {
        if let Some(hit) =
            plex_guid_search(plex, &guid, "tmdb", &tmdb_id.to_string()).await?
        {
            return Ok(vec![hit]);
        }
    }

    let sections = plex_sections(plex, library_id).await?;
    let want = if media_type == "tv" { "show" } else { "movie" };
    for section in sections {
        if section.section_type.as_deref() != Some(want) {
            continue;
        }
        if let Some(hit) =
            plex_scan_section(plex, &section.key, media_type, tmdb_id, imdb.as_deref()).await?
        {
            return Ok(vec![hit]);
        }
    }
    Ok(Vec::new())
}

async fn plex_guid_search(
    plex: &PlexClient,
    guid: &str,
    matched_by: &str,
    external_id: &str,
) -> AppResult<Option<LibraryMatch>> {
    let resp = plex.get("/library/all")?.query(&[("guid", guid)]).send().await?;
    if !resp.status().is_success() {
        return Ok(None);
    }
    let body: PlexContainer = resp.json().await.unwrap_or_default();
    let meta = body
        .media_container
        .and_then(|c| c.metadata)
        .unwrap_or_default();
    if let Some(item) = meta.first() {
        return Ok(Some(LibraryMatch {
            source: "plex".into(),
            title: item.title.clone().unwrap_or_else(|| "Unknown".into()),
            matched_by: matched_by.into(),
            external_id: Some(external_id.into()),
        }));
    }
    Ok(None)
}

async fn plex_sections(
    plex: &PlexClient,
    only_id: Option<&str>,
) -> AppResult<Vec<PlexSection>> {
    let resp: PlexContainer = plex.send_json(plex.get("/library/sections")?).await?;
    let dirs = resp
        .media_container
        .and_then(|c| c.directory)
        .unwrap_or_default();

    Ok(dirs
        .into_iter()
        .filter(|d| only_id.map(|id| d.key.as_str() == id).unwrap_or(true))
        .collect())
}

async fn plex_scan_section(
    plex: &PlexClient,
    section_key: &str,
    media_type: &str,
    tmdb_id: i64,
    imdb_id: Option<&str>,
) -> AppResult<Option<LibraryMatch>> {
    let type_num = if media_type == "tv" { "2" } else { "1" };
    let path = format!("/library/sections/{section_key}/all");
    let mut start = 0i32;
    let size = 100i32;
    for _ in 0..30 {
        let start_s = start.to_string();
        let size_s = size.to_string();
        let req = plex.get(&path)?.query(&[
            ("type", type_num),
            ("includeGuids", "1"),
            ("X-Plex-Container-Start", start_s.as_str()),
            ("X-Plex-Container-Size", size_s.as_str()),
        ]);
        let resp: PlexContainer = plex.send_json(req).await?;

        let container = resp.media_container.unwrap_or_default();
        let meta = container.metadata.unwrap_or_default();
        if meta.is_empty() {
            break;
        }
        for item in &meta {
            if let Some(m) = match_plex_item(item, tmdb_id, imdb_id) {
                return Ok(Some(m));
            }
        }
        start += size;
        let total = container.total_size.unwrap_or(start);
        if start >= total {
            break;
        }
    }
    Ok(None)
}

fn match_plex_item(
    item: &PlexMetadata,
    tmdb_id: i64,
    imdb_id: Option<&str>,
) -> Option<LibraryMatch> {
    let guids = item.guid_list();
    if let Some(imdb) = imdb_id {
        if guids.iter().any(|g| plex_guid_is(g, "imdb", imdb)) {
            return Some(LibraryMatch {
                source: "plex".into(),
                title: item.title.clone().unwrap_or_else(|| "Unknown".into()),
                matched_by: "imdb".into(),
                external_id: Some(imdb.to_string()),
            });
        }
    }
    let tmdb = tmdb_id.to_string();
    if guids
        .iter()
        .any(|g| plex_guid_is(g, "tmdb", &tmdb) || plex_guid_is(g, "themoviedb", &tmdb))
    {
        return Some(LibraryMatch {
            source: "plex".into(),
            title: item.title.clone().unwrap_or_else(|| "Unknown".into()),
            matched_by: "tmdb".into(),
            external_id: Some(tmdb),
        });
    }
    None
}

fn plex_guid_is(guid: &str, provider: &str, id: &str) -> bool {
    let g = guid.to_ascii_lowercase();
    let id = id.to_ascii_lowercase();
    let prefixes = [
        format!("{provider}://"),
        format!("com.plexapp.agents.{provider}://"),
    ];
    prefixes.iter().any(|p| {
        g.strip_prefix(p)
            .map(|rest| rest.split('?').next().unwrap_or(rest) == id)
            .unwrap_or(false)
    })
}

fn normalize_imdb(id: &str) -> String {
    let id = id.trim();
    if id.is_empty() {
        return String::new();
    }
    if id.starts_with("tt") {
        id.to_string()
    } else {
        format!("tt{id}")
    }
}

#[derive(Debug, Deserialize)]
struct JellyfinItemsResponse {
    #[serde(rename = "Items")]
    items: Option<Vec<JellyfinItem>>,
    #[serde(rename = "TotalRecordCount")]
    total_record_count: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct JellyfinItem {
    #[serde(rename = "Id")]
    id: Option<String>,
    #[serde(rename = "Name")]
    name: Option<String>,
    #[serde(rename = "ProductionYear")]
    production_year: Option<i32>,
    #[serde(rename = "ProviderIds")]
    provider_ids: Option<std::collections::HashMap<String, String>>,
}

#[derive(Debug, Deserialize, Default)]
struct PlexContainer {
    #[serde(rename = "MediaContainer")]
    media_container: Option<PlexMediaContainer>,
}

#[derive(Debug, Deserialize, Default)]
struct PlexMediaContainer {
    #[serde(rename = "Metadata")]
    metadata: Option<Vec<PlexMetadata>>,
    #[serde(rename = "Directory")]
    directory: Option<Vec<PlexSection>>,
    #[serde(rename = "totalSize")]
    total_size: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct PlexSection {
    #[serde(rename = "key")]
    key: String,
    #[serde(rename = "type")]
    section_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PlexMetadata {
    #[serde(rename = "title")]
    title: Option<String>,
    #[serde(rename = "ratingKey")]
    rating_key: Option<String>,
    #[serde(rename = "key")]
    key: Option<String>,
    #[serde(rename = "year")]
    year: Option<i32>,
    #[serde(rename = "guid")]
    guid: Option<String>,
    #[serde(rename = "Guid")]
    guids: Option<Vec<PlexGuid>>,
}

impl PlexMetadata {
    fn guid_list(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(g) = &self.guid {
            out.push(g.clone());
        }
        if let Some(list) = &self.guids {
            for g in list {
                if let Some(id) = &g.id {
                    out.push(id.clone());
                }
            }
        }
        out
    }
}

#[derive(Debug, Deserialize)]
struct PlexGuid {
    #[serde(rename = "id")]
    id: Option<String>,
}
