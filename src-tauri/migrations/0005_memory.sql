CREATE TABLE IF NOT EXISTS embedding (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    source_kind TEXT NOT NULL,
    source_id   INTEGER NOT NULL,
    text        TEXT NOT NULL,
    vector      BLOB NOT NULL,
    created_at  TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_embedding_source ON embedding(source_kind, source_id);
CREATE INDEX IF NOT EXISTS idx_embedding_created ON embedding(created_at DESC);

UPDATE meta SET value = '5' WHERE key = 'schema_version';
