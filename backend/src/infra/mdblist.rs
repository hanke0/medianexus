use crate::error::{AppError, AppResult};
use crate::infra::tmdb::MediaItem;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::time::Duration;

const API: &str = "https://api.mdblist.com";
const CHUNK: usize = 10;
const TIMEOUT: Duration = Duration::from_secs(8);
/// Reuse a score for a day so browsing does not burn the daily quota.
const TTL_SECS: i64 = 24 * 60 * 60;

#[derive(Clone)]
pub struct MdblistClient {
    http: reqwest::Client,
    api_key: String,
}

#[derive(Debug, Clone, Default)]
struct Score {
    imdb: Option<f64>,
    rt: Option<f64>,
}

#[derive(Debug, Serialize)]
struct RatingRequest<'a> {
    ids: &'a [i64],
    provider: &'static str,
}

#[derive(Debug, Deserialize)]
struct RatingResponse {
    #[serde(default)]
    ratings: Vec<RatingRow>,
}

#[derive(Debug, Deserialize)]
struct RatingRow {
    id: i64,
    rating: Option<f64>,
}

impl MdblistClient {
    pub fn new(http: reqwest::Client, api_key: String) -> Self {
        Self { http, api_key }
    }

    pub fn configured(&self) -> bool {
        !self.api_key.trim().is_empty()
    }

    async fn lookup(&self, media_type: &str, ids: &[i64]) -> AppResult<HashMap<i64, Score>> {
        let kind = match media_type {
            "movie" => "movie",
            "tv" => "show",
            _ => return Ok(HashMap::new()),
        };
        if ids.is_empty() {
            return Ok(HashMap::new());
        }

        let mut tasks = Vec::new();
        for chunk in ids.chunks(CHUNK) {
            let chunk = chunk.to_vec();
            tasks.push(self.fetch_chunk(kind, chunk));
        }
        let parts = futures_util::future::try_join_all(tasks).await?;
        let mut out = HashMap::new();
        for (imdb, rt) in parts {
            for (id, rating) in imdb {
                out.entry(id).or_insert_with(Score::default).imdb = rating;
            }
            for (id, rating) in rt {
                out.entry(id).or_insert_with(Score::default).rt = rating;
            }
        }
        Ok(out)
    }

    async fn fetch_chunk(
        &self,
        kind: &str,
        ids: Vec<i64>,
    ) -> AppResult<(HashMap<i64, Option<f64>>, HashMap<i64, Option<f64>>)> {
        let (imdb, rt) = tokio::join!(
            self.fetch(kind, "imdb", &ids),
            self.fetch(kind, "tomatoes", &ids),
        );
        Ok((imdb?, rt?))
    }

    async fn fetch(
        &self,
        kind: &str,
        source: &str,
        ids: &[i64],
    ) -> AppResult<HashMap<i64, Option<f64>>> {
        let url = format!("{API}/rating/{kind}/{source}");
        let response = self
            .http
            .post(url)
            .query(&[("apikey", self.api_key.trim())])
            .json(&RatingRequest {
                ids,
                provider: "tmdb",
            })
            .timeout(TIMEOUT)
            .send()
            .await
            .map_err(|err| {
                AppError::Internal(format!(
                    "mdblist {source} request failed: {}",
                    err.without_url()
                ))
            })?;
        let status = response.status();
        if !status.is_success() {
            return Err(AppError::Internal(format!(
                "mdblist {source} returned {status}"
            )));
        }
        let body: RatingResponse = response.json().await.map_err(|err| {
            AppError::Internal(format!(
                "mdblist {source} response was not ratings json: {}",
                err.without_url()
            ))
        })?;
        Ok(body
            .ratings
            .into_iter()
            .map(|row| (row.id, finite_rating(row.rating)))
            .collect())
    }
}

/// Fill IMDb (0–10) and Rotten Tomatoes (0–100) on list/detail items.
/// A missing key or a remote failure leaves the scores empty and still returns the list.
pub async fn attach(db: &SqlitePool, client: &MdblistClient, items: &mut [MediaItem]) {
    if !client.configured() || items.is_empty() {
        return;
    }
    let mut types: Vec<String> = items.iter().map(|item| item.media_type.clone()).collect();
    types.sort();
    types.dedup();
    for media_type in types {
        if let Err(err) = attach_type(db, client, items, &media_type).await {
            tracing::warn!(%err, %media_type, "external ratings lookup failed");
        }
    }
}

async fn attach_type(
    db: &SqlitePool,
    client: &MdblistClient,
    items: &mut [MediaItem],
    media_type: &str,
) -> AppResult<()> {
    let ids: Vec<i64> = items
        .iter()
        .filter(|item| item.media_type == media_type)
        .map(|item| item.tmdb_id)
        .collect();
    let now = chrono::Utc::now().timestamp();
    let mut known: HashMap<i64, Score> = HashMap::new();
    let mut missing = Vec::new();
    for id in ids {
        if let Some(score) = read_cache(db, media_type, id, now).await? {
            known.insert(id, score);
        } else if !missing.contains(&id) {
            missing.push(id);
        }
    }
    if !missing.is_empty() {
        match client.lookup(media_type, &missing).await {
            Ok(fetched) => {
                for id in &missing {
                    let score = fetched.get(id).cloned().unwrap_or_default();
                    write_cache(db, media_type, *id, &score, now).await?;
                    known.insert(*id, score);
                }
            }
            Err(err) => tracing::warn!(%err, "mdblist ratings request failed"),
        }
    }
    for item in items
        .iter_mut()
        .filter(|item| item.media_type == media_type)
    {
        if let Some(score) = known.get(&item.tmdb_id) {
            item.imdb_rating = score.imdb;
            item.rt_rating = score.rt;
        }
    }
    Ok(())
}

async fn read_cache(
    db: &SqlitePool,
    media_type: &str,
    tmdb_id: i64,
    now: i64,
) -> AppResult<Option<Score>> {
    let row: Option<(Option<f64>, Option<f64>, i64)> = sqlx::query_as(
        "SELECT imdb_rating, rt_rating, fetched_at FROM external_ratings WHERE media_type = ? AND tmdb_id = ?",
    )
    .bind(media_type)
    .bind(tmdb_id)
    .fetch_optional(db)
    .await?;
    let Some((imdb, rt, fetched_at)) = row else {
        return Ok(None);
    };
    if now.saturating_sub(fetched_at) > TTL_SECS {
        return Ok(None);
    }
    Ok(Some(Score { imdb, rt }))
}

async fn write_cache(
    db: &SqlitePool,
    media_type: &str,
    tmdb_id: i64,
    score: &Score,
    now: i64,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO external_ratings (media_type, tmdb_id, imdb_rating, rt_rating, fetched_at)
         VALUES (?, ?, ?, ?, ?)
         ON CONFLICT(media_type, tmdb_id) DO UPDATE SET
           imdb_rating = excluded.imdb_rating,
           rt_rating = excluded.rt_rating,
           fetched_at = excluded.fetched_at",
    )
    .bind(media_type)
    .bind(tmdb_id)
    .bind(score.imdb)
    .bind(score.rt)
    .bind(now)
    .execute(db)
    .await?;
    Ok(())
}

fn finite_rating(value: Option<f64>) -> Option<f64> {
    value.filter(|n| n.is_finite())
}

#[cfg(test)]
mod tests {
    use super::RatingResponse;

    #[test]
    fn parses_rating_payload() {
        let body: RatingResponse = serde_json::from_str(
            r#"{"provider_rating":"imdb","ratings":[{"id":923,"rating":8.1},{"id":1,"rating":null}]}"#,
        )
        .unwrap();
        assert_eq!(body.ratings.len(), 2);
        assert_eq!(body.ratings[0].id, 923);
        assert_eq!(body.ratings[0].rating, Some(8.1));
        assert_eq!(body.ratings[1].rating, None);
    }
}
