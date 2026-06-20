-- v2 Phase 2.2 — sync outbox + cursor tracking.
--
-- Outbox pattern: writes to local entities also enqueue a change event
-- in the same transaction. A background worker drains the outbox into
-- the cloud's /sync/push endpoint. Crash-safe by construction — a row
-- in the outbox is the truth that "this write hasn't reached the cloud
-- yet" regardless of how the process exits between write and ack.
--
-- Rows are deleted on successful push (we don't need history once the
-- cloud has it; the cloud's UserState DO is the durable log). Failed
-- pushes increment attempts and stash last_error for diagnostics.

CREATE TABLE IF NOT EXISTS sync_outbox (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    kind          TEXT    NOT NULL,
    payload       TEXT    NOT NULL,
    source_device TEXT,
    created_at    TEXT    NOT NULL DEFAULT CURRENT_TIMESTAMP,
    attempts      INTEGER NOT NULL DEFAULT 0,
    last_error    TEXT
);

CREATE INDEX IF NOT EXISTS idx_sync_outbox_created
    ON sync_outbox(created_at);
