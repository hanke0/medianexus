CREATE TABLE library_items (
    id TEXT PRIMARY KEY NOT NULL,
    source TEXT NOT NULL CHECK (source IN ('plex', 'jellyfin')),
    media_type TEXT NOT NULL CHECK (media_type IN ('movie', 'tv')),
    external_id TEXT NOT NULL,
    title TEXT NOT NULL,
    year TEXT,
    tmdb_id INTEGER,
    imdb_id TEXT,
    synced_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (source, external_id)
);

CREATE INDEX idx_library_items_tmdb ON library_items(media_type, tmdb_id);
CREATE INDEX idx_library_items_imdb ON library_items(imdb_id);
CREATE INDEX idx_library_items_source ON library_items(source, media_type);
