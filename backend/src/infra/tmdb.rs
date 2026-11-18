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
    /// IMDb score, 0–10.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imdb_rating: Option<f64>,
    /// Rotten Tomatoes critics score, 0–100.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rt_rating: Option<f64>,
    pub popularity: Option<f64>,
    pub vote_average: Option<f64>,
    /// TV seasons from the detail endpoint. Empty for movies and list results.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub seasons: Vec<TvSeason>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TvSeason {
    pub season_number: i64,
    pub name: String,
    pub episode_count: i64,
    pub poster_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TvEpisode {
    pub episode_number: i64,
    pub name: String,
    pub overview: Option<String>,
    pub still_url: Option<String>,
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
    #[serde(default)]
    seasons: Vec<TmdbSeason>,
}

#[derive(Debug, Deserialize)]
struct TmdbSeason {
    season_number: i64,
    name: Option<String>,
    episode_count: Option<i64>,
    poster_path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TmdbSeasonDetail {
    episodes: Vec<TmdbEpisodeRaw>,
}

#[derive(Debug, Deserialize)]
struct TmdbEpisodeRaw {
    episode_number: i64,
    name: Option<String>,
    overview: Option<String>,
    still_path: Option<String>,
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
                imdb_rating: None,
                rt_rating: None,
                popularity: m.popularity,
                vote_average: m.vote_average,
                seasons: Vec::new(),
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
                imdb_rating: None,
                rt_rating: None,
                popularity: t.popularity,
                vote_average: t.vote_average,
                seasons: Vec::new(),
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
                imdb_rating: None,
                rt_rating: None,
                popularity: m.popularity,
                vote_average: m.vote_average,
                seasons: Vec::new(),
            })
            .collect())
    }

    pub async fn top_rated_movies(&self, page: u32) -> AppResult<Vec<MediaItem>> {
        self.ensure_key()?;
        let url = format!("{TMDB_BASE}/movie/top_rated");
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
                imdb_rating: None,
                rt_rating: None,
                popularity: m.popularity,
                vote_average: m.vote_average,
                seasons: Vec::new(),
            })
            .collect())
    }

    pub async fn top_rated_tv(&self, page: u32) -> AppResult<Vec<MediaItem>> {
        self.ensure_key()?;
        let url = format!("{TMDB_BASE}/tv/top_rated");
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
                imdb_rating: None,
                rt_rating: None,
                popularity: t.popularity,
                vote_average: t.vote_average,
                seasons: Vec::new(),
            })
            .collect())
    }

    /// Replace list-card text and artwork with the configured TMDB language.
    /// Ratings already on the item are kept.
    pub async fn localize_cards(&self, items: &mut [MediaItem]) {
        if self.ensure_key().is_err() || items.is_empty() {
            return;
        }
        let jobs = items
            .iter()
            .map(|item| self.card(&item.media_type, item.tmdb_id));
        let results = futures_util::future::join_all(jobs).await;
        for (item, result) in items.iter_mut().zip(results) {
            let Ok(card) = result else {
                continue;
            };
            let imdb_rating = item.imdb_rating;
            let rt_rating = item.rt_rating;
            let imdb_id = item.imdb_id.clone();
            *item = card;
            item.imdb_rating = imdb_rating;
            item.rt_rating = rt_rating;
            if item.imdb_id.is_none() {
                item.imdb_id = imdb_id;
            }
        }
    }

    async fn card(&self, media_type: &str, tmdb_id: i64) -> AppResult<MediaItem> {
        match media_type {
            "movie" => {
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
                Ok(MediaItem {
                    media_type: "movie".into(),
                    tmdb_id: m.id,
                    title: m.title.unwrap_or_else(|| "Untitled".into()),
                    original_title: m.original_title,
                    year: year_from_date(m.release_date.as_deref()),
                    poster_url: poster_url(m.poster_path.as_deref()),
                    overview: m.overview,
                    imdb_id: None,
                    imdb_rating: None,
                    rt_rating: None,
                    popularity: m.popularity,
                    vote_average: m.vote_average,
                    seasons: Vec::new(),
                })
            }
            "tv" => {
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
                Ok(MediaItem {
                    media_type: "tv".into(),
                    tmdb_id: t.id,
                    title: t.name.unwrap_or_else(|| "Untitled".into()),
                    original_title: t.original_name,
                    year: year_from_date(t.first_air_date.as_deref()),
                    poster_url: poster_url(t.poster_path.as_deref()),
                    overview: t.overview,
                    imdb_id: None,
                    imdb_rating: None,
                    rt_rating: None,
                    popularity: t.popularity,
                    vote_average: t.vote_average,
                    seasons: Vec::new(),
                })
            }
            _ => Err(AppError::BadRequest(
                "media_type must be movie or tv".into(),
            )),
        }
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
                imdb_rating: None,
                rt_rating: None,
                popularity: t.popularity,
                vote_average: t.vote_average,
                seasons: Vec::new(),
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
                imdb_rating: None,
                rt_rating: None,
                popularity: m.popularity,
                vote_average: m.vote_average,
                seasons: Vec::new(),
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
                imdb_rating: None,
                rt_rating: None,
                popularity: t.popularity,
                vote_average: t.vote_average,
                seasons: Vec::new(),
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
            imdb_rating: None,
            rt_rating: None,
            popularity: m.popularity,
            vote_average: m.vote_average,
            seasons: Vec::new(),
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
            imdb_rating: None,
            rt_rating: None,
            popularity: t.popularity,
            vote_average: t.vote_average,
            seasons: map_seasons(t.seasons),
        })
    }

    pub async fn tv_season(&self, tmdb_id: i64, season_number: i64) -> AppResult<Vec<TvEpisode>> {
        self.ensure_key()?;
        if season_number < 0 {
            return Err(AppError::BadRequest("season_number must be >= 0".into()));
        }
        let url = format!("{TMDB_BASE}/tv/{tmdb_id}/season/{season_number}");
        let detail: TmdbSeasonDetail = self
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
        Ok(detail
            .episodes
            .into_iter()
            .map(|e| TvEpisode {
                episode_number: e.episode_number,
                name: e
                    .name
                    .filter(|n| !n.trim().is_empty())
                    .unwrap_or_else(|| format!("第 {} 集", e.episode_number)),
                overview: e.overview,
                still_url: poster_url(e.still_path.as_deref()),
            })
            .collect())
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

fn map_seasons(seasons: Vec<TmdbSeason>) -> Vec<TvSeason> {
    let mut out: Vec<TvSeason> = seasons
        .into_iter()
        .map(|s| {
            let n = s.season_number;
            TvSeason {
                season_number: n,
                name: s
                    .name
                    .filter(|name| !name.trim().is_empty())
                    .unwrap_or_else(|| {
                        if n == 0 {
                            "特别篇".into()
                        } else {
                            format!("第 {n} 季")
                        }
                    }),
                episode_count: s.episode_count.unwrap_or(0),
                poster_url: poster_url(s.poster_path.as_deref()),
            }
        })
        .collect();
    out.sort_by_key(|s| s.season_number);
    out
}

fn year_from_date(date: Option<&str>) -> Option<String> {
    date.and_then(|d| d.get(0..4).map(|y| y.to_string()))
        .filter(|y| y.chars().all(|c| c.is_ascii_digit()))
}

fn poster_url(path: Option<&str>) -> Option<String> {
    path.filter(|p| !p.is_empty())
        .map(|p| format!("{TMDB_IMG}{p}"))
}
