CREATE TABLE IF NOT EXISTS oauth_account (
    provider     TEXT PRIMARY KEY,           -- 'google_calendar', later 'outlook_calendar', etc.
    account_id   TEXT,                       -- email or sub from id_token
    scopes       TEXT NOT NULL DEFAULT '',   -- space-separated granted scopes
    access_token TEXT,                       -- short-lived; refresh on demand
    expires_at   TEXT,                       -- ISO-8601 UTC
    connected_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at   TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

UPDATE meta SET value = '15' WHERE key = 'schema_version';
