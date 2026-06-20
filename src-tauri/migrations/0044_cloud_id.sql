-- v2 Phase 2.4 — stable cross-device identifiers.
--
-- Each entity that syncs gets a `cloud_id`: a 32-hex-char random token
-- generated at insert time (or backfilled here for existing rows). It's
-- the cross-device key — local `id` is per-install and useless for
-- matching events from other devices.
--
-- Apply pipeline uses this to be idempotent: when we pull a memory.add
-- event whose cloud_id already exists locally, we skip the insert.
-- When we pull a conversation.upsert whose cloud_id matches a local
-- conversation, we update in place (with full message replace).

ALTER TABLE embedding    ADD COLUMN cloud_id TEXT;
ALTER TABLE conversation ADD COLUMN cloud_id TEXT;

UPDATE embedding
   SET cloud_id = lower(hex(randomblob(16)))
 WHERE cloud_id IS NULL;

UPDATE conversation
   SET cloud_id = lower(hex(randomblob(16)))
 WHERE cloud_id IS NULL;

-- Partial unique index — NULLs (briefly during INSERT) are allowed
-- but any populated cloud_id must be unique. This is the constraint
-- that makes idempotent apply correct.
CREATE UNIQUE INDEX IF NOT EXISTS idx_embedding_cloud_id
    ON embedding(cloud_id) WHERE cloud_id IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_conversation_cloud_id
    ON conversation(cloud_id) WHERE cloud_id IS NOT NULL;
