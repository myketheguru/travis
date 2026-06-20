-- v2 Phase 2.5 — document files sync.
--
-- The cloud already exposes /sync/files (R2-backed, content-addressed by
-- SHA-256). This migration brings documents into the sync graph with
-- two pieces:
--
-- 1. cloud_id on `document` — same model as embedding + conversation
--    (Phase 2.4). Cross-device key for matching doc.upsert events.
--    Two device-local rows can point at the same content_hash (dedup)
--    but each row is a distinct sync entity with its own cloud_id.
--
-- 2. `file_upload_queue` — separate from sync_outbox because file
--    bytes flow through /sync/files (multi-step put-url + PUT)
--    rather than /sync/push (single batched JSON POST). The engine
--    drains both queues per cycle. INSERT OR IGNORE keyed on
--    content_hash means duplicate enqueues (same file referenced by
--    multiple doc rows) only upload once.

ALTER TABLE document ADD COLUMN cloud_id TEXT;

UPDATE document
   SET cloud_id = lower(hex(randomblob(16)))
 WHERE cloud_id IS NULL;

CREATE UNIQUE INDEX IF NOT EXISTS idx_document_cloud_id
    ON document(cloud_id) WHERE cloud_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS file_upload_queue (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    content_hash  TEXT    NOT NULL,
    relative_path TEXT    NOT NULL,
    mime_type     TEXT    NOT NULL,
    size_bytes    INTEGER NOT NULL,
    attempts      INTEGER NOT NULL DEFAULT 0,
    last_error    TEXT,
    created_at    TEXT    NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_file_upload_queue_hash
    ON file_upload_queue(content_hash);
