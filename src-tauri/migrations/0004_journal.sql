CREATE TABLE IF NOT EXISTS journal_entry (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    raw_text        TEXT NOT NULL,
    extraction_json TEXT,
    extraction_ok   INTEGER NOT NULL DEFAULT 0,
    error_message   TEXT,
    provider        TEXT,
    model           TEXT,
    created_at      TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_journal_created ON journal_entry(created_at DESC);

UPDATE meta SET value = '4' WHERE key = 'schema_version';
