CREATE TABLE IF NOT EXISTS reminder (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    text          TEXT NOT NULL,
    kind          TEXT NOT NULL DEFAULT 'time'
                  CHECK (kind IN ('time','context','behavioral')),
    remind_at     TEXT,
    fired_at      TEXT,
    dismissed_at  TEXT,
    source        TEXT NOT NULL DEFAULT 'manual',
    link_kind     TEXT,
    link_id       INTEGER,
    created_at    TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at    TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_reminder_due ON reminder(fired_at, remind_at);
CREATE INDEX IF NOT EXISTS idx_reminder_created ON reminder(created_at DESC);

UPDATE meta SET value = '6' WHERE key = 'schema_version';
