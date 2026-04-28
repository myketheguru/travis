CREATE TABLE IF NOT EXISTS summary (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    kind          TEXT NOT NULL CHECK(kind IN ('daily','weekly')),
    period_start  TEXT NOT NULL,
    period_end    TEXT NOT NULL,
    content       TEXT NOT NULL,
    source_count  INTEGER NOT NULL DEFAULT 0,
    model         TEXT,
    provider      TEXT,
    created_at    TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(kind, period_start)
);
CREATE INDEX IF NOT EXISTS idx_summary_kind_end ON summary(kind, period_end DESC);

UPDATE meta SET value = '8' WHERE key = 'schema_version';
