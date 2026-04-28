CREATE TABLE IF NOT EXISTS feature_flag_cache (
    id          INTEGER PRIMARY KEY CHECK (id = 1),
    flags_json  TEXT NOT NULL,
    fetched_at  TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    etag        TEXT,
    source_url  TEXT NOT NULL
);

INSERT OR IGNORE INTO meta(key, value) VALUES ('flags_url', 'https://leadtoempower.github.io/travis/config.json');

UPDATE meta SET value = '11' WHERE key = 'schema_version';
