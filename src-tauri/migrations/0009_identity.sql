CREATE TABLE IF NOT EXISTS entity_index (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    kind             TEXT NOT NULL CHECK(kind IN ('coach','school','dept')),
    normalized_name  TEXT NOT NULL,
    display_name     TEXT NOT NULL,
    mentions_count   INTEGER NOT NULL DEFAULT 0,
    first_seen       TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_seen        TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    attributes_json  TEXT,
    UNIQUE(kind, normalized_name)
);
CREATE INDEX IF NOT EXISTS idx_entity_kind_mentions ON entity_index(kind, mentions_count DESC);

UPDATE meta SET value = '9' WHERE key = 'schema_version';
