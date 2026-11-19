CREATE TABLE browse_cache (
    key TEXT PRIMARY KEY,
    items TEXT NOT NULL,
    expires_at INTEGER NOT NULL
);
