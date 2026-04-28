CREATE TABLE IF NOT EXISTS email_sent (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    recipient      TEXT NOT NULL,
    subject        TEXT NOT NULL,
    body_preview   TEXT,
    kind           TEXT,
    related_kind   TEXT,
    related_id     INTEGER,
    status         TEXT NOT NULL DEFAULT 'pending'
                       CHECK(status IN ('pending','sent','failed')),
    error_message  TEXT,
    sent_at        TEXT,
    created_at     TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_email_sent_created_at ON email_sent(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_email_sent_status     ON email_sent(status);

CREATE TABLE IF NOT EXISTS smtp_config (
    id            INTEGER PRIMARY KEY CHECK (id = 1),
    host          TEXT NOT NULL,
    port          INTEGER NOT NULL DEFAULT 587,
    username      TEXT NOT NULL,
    from_address  TEXT NOT NULL,
    from_name     TEXT,
    use_tls       INTEGER NOT NULL DEFAULT 1,
    updated_at    TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

UPDATE meta SET value = '10' WHERE key = 'schema_version';
