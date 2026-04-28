CREATE TABLE IF NOT EXISTS app_feedback (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    capability    TEXT NOT NULL,
    context       TEXT,
    source_kind   TEXT,
    source_id     INTEGER,
    addressed_at  TEXT,
    created_at    TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_feedback_created ON app_feedback(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_feedback_capability ON app_feedback(capability);

UPDATE meta SET value = '12' WHERE key = 'schema_version';
