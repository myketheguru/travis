CREATE TABLE IF NOT EXISTS telemetry_config (
    id            INTEGER PRIMARY KEY CHECK(id = 1),
    sink_kind     TEXT NOT NULL DEFAULT 'off' CHECK(sink_kind IN ('off','http','firebase')),
    endpoint_url  TEXT,
    enabled       INTEGER NOT NULL DEFAULT 0,
    last_sent_at  TEXT,
    updated_at    TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
INSERT OR IGNORE INTO telemetry_config (id, sink_kind, enabled) VALUES (1, 'off', 0);

CREATE TABLE IF NOT EXISTS telemetry_event (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    kind         TEXT NOT NULL,
    payload_json TEXT NOT NULL DEFAULT '{}',
    created_at   TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    sent_at      TEXT,
    attempts     INTEGER NOT NULL DEFAULT 0,
    last_error   TEXT
);
CREATE INDEX IF NOT EXISTS idx_telemetry_pending
    ON telemetry_event(created_at) WHERE sent_at IS NULL;

UPDATE meta SET value = '14' WHERE key = 'schema_version';
