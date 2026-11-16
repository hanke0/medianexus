use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

const TMDB_BASE: &str = "https://api.themoviedb.org/3";
const TMDB_IMG: &str = "https://image.tmdb.org/t/p/w500";

#[derive(Clone)]
pub struct TmdbClient {
    http: reqwest::Client,
    api_key: String,
    language: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MediaItem {
    pub media_type: String,
    pub tmdb_id: i64,
    pub title: String,
    pub original_title: Option<String>,
    pub year: Option<String>,
    pub poster_url: Option<String>,
    pub overview: Option<String>,
    pub imdb_id: Option<String>,
    pub popularity: Option<f64>,
    pub vote_average: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct TmdbPaged<T> {
    results: Vec<T>,
}

#[derive(Debug, Deserialize)]
struct TmdbMovie {
    id: i64,
    title: Option<String>,
    original_title: Option<String>,
    release_date: Option<String>,
    poster_path: Option<String>,
    overview: Option<String>,
    popularity: Option<f64>,
    vote_average: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct TmdbTv {
    id: i64,
    name: Option<String>,
    original_name: Option<String>,
    first_air_date: Option<String>,
    poster_path: Option<String>,
    overview: Option<String>,
    popularity: Option<f64>,
    vote_average: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct ExternalIds {
    imdb_id: Option<String>,
}

impl TmdbClient {
    pub fn new(http: reqwest::Client, api_key: String, language: String) -> Self {
        let language = {
            let t = language.trim();
            if t.is_empty() {
                "zh-CN".into()
            } else {
                t.to_string()
            }
        };
        Self {
            http,
            api_key,
            language,
        }
    }

    fn ensure_key(&self) -> AppResult<()> {
        if self.api_key.trim().is_empty() {
            return Err(AppError::BadRequest(
                "TMDB_API_KEY is not configured".into(),
            ));
        }
        Ok(())
    }

    fn lang(&self) -> &str {
        &self.language
    }

    pub async fn latest_movies(&self, page: u32) -> AppResult<Vec<MediaItem>> {
        self.ensure_key()?;
        let url = format!("{TMDB_BASE}/movie/now_playing");
        let page = page.to_string();
        let resp: TmdbPaged<TmdbMovie> = self
            .http
            .get(&url)
            .query(&[
                ("api_key", self.api_key.as_str()),
                ("language", self.lang()),
                ("page", page.as_str()),
                ("region", "CN"),
            ])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        Ok(resp
            .results
            .into_iter()
            .map(|m| MediaItem {
                media_type: "movie".into(),
                tmdb_id: m.id,
                title: m.title.unwrap_or_else(|| "Untitled".into()),
                original_title: m.original_title,
                year: year_from_date(m.release_date.as_deref()),
                poster_url: poster_url(m.poster_path.as_deref()),
                overview: m.overview,
                imdb_id: None,
                popularity: m.popularity,
                vote_average: m.vote_average,
            })
            .collect())
    }

    pub async fn latest_tv(&self, page: u32) -> AppResult<Vec<MediaItem>> {
        self.ensure_key()?;
        let url = format!("{TMDB_BASE}/tv/on_the_air");
        let page = page.to_string();
        let resp: TmdbPaged<TmdbTv> = self
            .http
            .get(&url)
            .query(&[
                ("api_key", self.api_key.as_str()),
                ("language", self.lang()),
                ("page", page.as_str()),
            ])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        Ok(resp
            .results
            .into_iter()
            .map(|t| MediaItem {
                media_type: "tv".into(),
                tmdb_id: t.id,
                title: t.name.unwrap_or_else(|| "Untitled".into()),
                original_title: t.original_name,
                year: year_from_date(t.first_air_date.as_deref()),
                poster_url: poster_url(t.poster_path.as_deref()),
                overview: t.overview,
                imdb_id: None,
                popularity: t.popularity,
                vote_average: t.vote_average,
            })
            .collect())
    }

    pub async fn popular_movies(&self, page: u32) -> AppResult<Vec<MediaItem>> {
        self.ensure_key()?;
        let url = format!("{TMDB_BASE}/movie/popular");
        let page = page.to_string();
        let resp: TmdbPaged<TmdbMovie> = self
            .http
            .get(&url)
            .query(&[
                ("api_key", self.api_key.as_str()),
                ("language", self.lang()),
                ("page", page.as_str()),
            ])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        Ok(resp
            .results
            .into_iter()
            .map(|m| MediaItem {
                media_type: "movie".into(),
                tmdb_id: m.id,
                title: m.title.unwrap_or_else(|| "Untitled".into()),
                original_title: m.original_title,
                year: year_from_date(m.release_date.as_deref()),
                poster_url: poster_url(m.poster_path.as_deref()),
                overview: m.overview,
                imdb_id: None,
                popularity: m.popularity,
                vote_average: m.vote_average,
            })
            .collect())
    }

    pub async fn popular_tv(&self, page: u32) -> AppResult<Vec<MediaItem>> {
        self.ensure_key()?;
        let url = format!("{TMDB_BASE}/tv/popular");
        let page = page.to_string();
        let resp: TmdbPaged<TmdbTv> = self
            .http
            .get(&url)
            .query(&[
                ("api_key", self.api_key.as_str()),
                ("language", self.lang()),
                ("page", page.as_str()),
            ])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        Ok(resp
            .results
            .into_iter()
            .map(|t| MediaItem {
                media_type: "tv".into(),
                tmdb_id: t.id,
                title: t.name.unwrap_or_else(|| "Untitled".into()),
                original_title: t.original_name,
                year: year_from_date(t.first_air_date.as_deref()),
                poster_url: poster_url(t.poster_path.as_deref()),
                overview: t.overview,
                imdb_id: None,
                popularity: t.popularity,
                vote_average: t.vote_average,
            })
            .collect())
    }

    pub async fn search_movies(&self, query: &str, page: u32) -> AppResult<Vec<MediaItem>> {
        self.ensure_key()?;
        let url = format!("{TMDB_BASE}/search/movie");
        let page = page.to_string();
        let resp: TmdbPaged<TmdbMovie> = self
            .http
            .get(&url)
            .query(&[
                ("api_key", self.api_key.as_str()),
                ("language", self.lang()),
                ("query", query),
                ("page", page.as_str()),
                ("include_adult", "false"),
            ])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        Ok(resp
            .results
            .into_iter()
            .map(|m| MediaItem {
                media_type: "movie".into(),
                tmdb_id: m.id,
                title: m.title.unwrap_or_else(|| "Untitled".into()),
                original_title: m.original_title,
                year: year_from_date(m.release_date.as_deref()),
                poster_url: poster_url(m.poster_path.as_deref()),
                overview: m.overview,
                imdb_id: None,
                popularity: m.popularity,
                vote_average: m.vote_average,
            })
            .collect())
    }

    pub async fn search_tv(&self, query: &str, page: u32) -> AppResult<Vec<MediaItem>> {
        self.ensure_key()?;
        let url = format!("{TMDB_BASE}/search/tv");
        let page = page.to_string();
        let resp: TmdbPaged<TmdbTv> = self
            .http
            .get(&url)
            .query(&[
                ("api_key", self.api_key.as_str()),
                ("language", self.lang()),
                ("query", query),
                ("page", page.as_str()),
                ("include_adult", "false"),
            ])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        Ok(resp
            .results
            .into_iter()
            .map(|t| MediaItem {
                media_type: "tv".into(),
                tmdb_id: t.id,
                title: t.name.unwrap_or_else(|| "Untitled".into()),
                original_title: t.original_name,
                year: year_from_date(t.first_air_date.as_deref()),
                poster_url: poster_url(t.poster_path.as_deref()),
                overview: t.overview,
                imdb_id: None,
                popularity: t.popularity,
                vote_average: t.vote_average,
            })
            .collect())
    }

    pub async fn search(
        &self,
        query: &str,
        media_type: &str,
        page: u32,
    ) -> AppResult<Vec<MediaItem>> {
        match media_type {
            "tv" => self.search_tv(query, page).await,
            "movie" => self.search_movies(query, page).await,
            "all" => {
                let (movies, shows) =
                    tokio::try_join!(self.search_movies(query, page), self.search_tv(query, page))?;
                let mut merged = movies;
                merged.extend(shows);
                merged.sort_by(|a, b| {
                    b.popularity
                        .partial_cmp(&a.popularity)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                Ok(merged)
            }
            _ => Err(AppError::BadRequest(
                "media_type must be movie, tv, or all".into(),
            )),
        }
    }

    pub async fn movie_details(&self, tmdb_id: i64) -> AppResult<MediaItem> {
        self.ensure_key()?;
        let url = format!("{TMDB_BASE}/movie/{tmdb_id}");
        let m: TmdbMovie = self
            .http
            .get(&url)
            .query(&[
                ("api_key", self.api_key.as_str()),
                ("language", self.lang()),
            ])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let imdb = self.external_ids("movie", tmdb_id).await.ok().flatten();

        Ok(MediaItem {
            media_type: "movie".into(),
            tmdb_id: m.id,
            title: m.title.unwrap_or_else(|| "Untitled".into()),
            original_title: m.original_title,
            year: year_from_date(m.release_date.as_deref()),
            poster_url: poster_url(m.poster_path.as_deref()),
            overview: m.overview,
            imdb_id: imdb,
            popularity: m.popularity,
            vote_average: m.vote_average,
        })
    }

    pub async fn tv_details(&self, tmdb_id: i64) -> AppResult<MediaItem> {
        self.ensure_key()?;
        let url = format!("{TMDB_BASE}/tv/{tmdb_id}");
        let t: TmdbTv = self
            .http
            .get(&url)
            .query(&[
                ("api_key", self.api_key.as_str()),
                ("language", self.lang()),
            ])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let imdb = self.external_ids("tv", tmdb_id).await.ok().flatten();

        Ok(MediaItem {
            media_type: "tv".into(),
            tmdb_id: t.id,
            title: t.name.unwrap_or_else(|| "Untitled".into()),
            original_title: t.original_name,
            year: year_from_date(t.first_air_date.as_deref()),
            poster_url: poster_url(t.poster_path.as_deref()),
            overview: t.overview,
            imdb_id: imdb,
            popularity: t.popularity,
            vote_average: t.vote_average,
        })
    }

    async fn external_ids(&self, kind: &str, tmdb_id: i64) -> AppResult<Option<String>> {
        let url = format!("{TMDB_BASE}/{kind}/{tmdb_id}/external_ids");
        let ids: ExternalIds = self
            .http
            .get(&url)
            .query(&[("api_key", self.api_key.as_str())])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(ids.imdb_id.filter(|s| !s.is_empty()))
    }
}

fn year_from_date(date: Option<&str>) -> Option<String> {
    date.and_then(|d| d.get(0..4).map(|y| y.to_string()))
        .filter(|y| y.chars().all(|c| c.is_ascii_digit()))
}

fn poster_url(path: Option<&str>) -> Option<String> {
    path.filter(|p| !p.is_empty())
        .map(|p| format!("{TMDB_IMG}{p}"))
}
