CREATE TABLE users (
    id TEXT PRIMARY KEY NOT NULL,
    username TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('admin', 'user')),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE tracks (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    media_type TEXT NOT NULL CHECK (media_type IN ('movie', 'tv')),
    tmdb_id INTEGER NOT NULL,
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
            'failed'
        )),
    progress REAL NOT NULL DEFAULT 0,
    torrent_id TEXT,
    torrent_name TEXT,
    torrent_hash TEXT,
    torrent_size TEXT,
    download_path TEXT,
    hardlink_path TEXT,
    error_message TEXT,
    status_message TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (user_id, media_type, tmdb_id)
);

CREATE INDEX idx_tracks_user ON tracks(user_id);
CREATE INDEX idx_tracks_status ON tracks(status);
CREATE INDEX idx_tracks_user_active ON tracks(user_id, status);
