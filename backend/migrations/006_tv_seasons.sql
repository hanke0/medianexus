-- Per-season / per-episode TV tracks. SQLite cannot drop UNIQUE, so recreate.
CREATE TABLE tracks_new (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    media_type TEXT NOT NULL CHECK (media_type IN ('movie', 'tv')),
    tmdb_id INTEGER NOT NULL,
    season_number INTEGER,
    episode_number INTEGER,
    title TEXT NOT NULL,
    original_title TEXT,
    year TEXT,
    poster_path TEXT,
    overview TEXT,
    imdb_id TEXT,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN (
            'pending',
            'searching',
            'torrent_found',
            'downloading',
            'hardlinking',
            'scanning',
            'completed',
            'already_exists',
            'failed'
        )),
    progress REAL NOT NULL DEFAULT 0,
    torrent_id TEXT,
    torrent_site TEXT,
    torrent_name TEXT,
    torrent_hash TEXT,
    torrent_size TEXT,
    download_path TEXT,
    hardlink_path TEXT,
    error_message TEXT,
    status_message TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO tracks_new (
    id, user_id, media_type, tmdb_id, title, original_title, year,
    poster_path, overview, imdb_id, status, progress,
    torrent_id, torrent_site, torrent_name, torrent_hash, torrent_size,
    download_path, hardlink_path, error_message, status_message,
    created_at, updated_at
)
SELECT
    id, user_id, media_type, tmdb_id, title, original_title, year,
    poster_path, overview, imdb_id, status, progress,
    torrent_id, torrent_site, torrent_name, torrent_hash, torrent_size,
    download_path, hardlink_path, error_message, status_message,
    created_at, updated_at
FROM tracks;

DROP TABLE tracks;
ALTER TABLE tracks_new RENAME TO tracks;

-- NULL season/episode collapse so a movie still tracks once.
CREATE UNIQUE INDEX idx_tracks_target ON tracks (
    user_id,
    media_type,
    tmdb_id,
    IFNULL(season_number, -1),
    IFNULL(episode_number, -1)
);

CREATE INDEX idx_tracks_user ON tracks(user_id);
CREATE INDEX idx_tracks_status ON tracks(status);
CREATE INDEX idx_tracks_user_active ON tracks(user_id, status);
CREATE INDEX idx_tracks_media ON tracks(media_type, tmdb_id);
