CREATE TABLE external_ratings (
    media_type TEXT NOT NULL,
    tmdb_id INTEGER NOT NULL,
    imdb_rating REAL,
    rt_rating REAL,
    fetched_at INTEGER NOT NULL,
    PRIMARY KEY (media_type, tmdb_id)
);
