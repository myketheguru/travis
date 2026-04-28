-- Behavioral memory: event log + detected patterns
-- See memory_and_context_prd.md §3.4 (Behavioral Memory) and §9 (Continuous Learning Loop),
-- and prd.md §4.5 (Automation Engine).

CREATE TABLE IF NOT EXISTS event_log (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    kind         TEXT NOT NULL,
    ref_kind     TEXT,
    ref_id       INTEGER,
    payload_json TEXT,
    created_at   TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_event_kind_created
    ON event_log(kind, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_event_created
    ON event_log(created_at DESC);

CREATE TABLE IF NOT EXISTS detected_pattern (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    sequence_json TEXT NOT NULL,
    occurrences   INTEGER NOT NULL DEFAULT 0,
    last_seen     TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    suggested_at  TEXT,
    dismissed_at  TEXT,
    automated_at  TEXT,
    created_at    TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE UNIQUE INDEX IF NOT EXISTS uq_detected_pattern_sequence
    ON detected_pattern(sequence_json);
CREATE INDEX IF NOT EXISTS idx_detected_pattern_last_seen
    ON detected_pattern(last_seen DESC);

UPDATE meta SET value = '7' WHERE key = 'schema_version';
