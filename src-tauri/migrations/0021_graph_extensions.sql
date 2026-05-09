-- Phase 4 — Knowledge graph foundation (KNOWLEDGE_GRAPH.md slice 1).
--
-- Extends the existing entity table with the columns needed to make
-- it the canonical memory layer: embeddings for graph-aware
-- retrieval, confidence + tags for ambient categorisation, archive
-- + pack_table_id for the typed-projection bridge.
--
-- All new columns are nullable or have defaults so the migration is
-- safe to run on a populated DB without backfilling. The embedding
-- pipeline (slice 7) will lazily fill in vectors for existing rows
-- as they're touched.
--
-- The `relation` and `event` tables already carry workspace_id from
-- 0020_workspaces.sql; nothing new there at the schema level. Their
-- helper functions in spine/relation.rs and spine/event.rs grow new
-- params over the next few slices but no SQL change is needed.

-- ---------------------------------------------------------------------
-- entity extensions
-- ---------------------------------------------------------------------

ALTER TABLE entity ADD COLUMN embedding_vector BLOB;
ALTER TABLE entity ADD COLUMN embedding_indexed_at TEXT;

-- Confidence in the entity's `kind`. Pack-table projections write
-- 1.0 (the row exists, the kind is certain). Ambient extractions
-- start lower and rise with mentions / user confirmation.
ALTER TABLE entity ADD COLUMN confidence REAL NOT NULL DEFAULT 1.0;

-- User-assigned free-form tags. Comma-separated; searchable.
ALTER TABLE entity ADD COLUMN tags TEXT;

-- Soft-archive. Archived entities don't appear in retrieval but
-- their mention timeline (and back-references from journal entries)
-- stays intact.
ALTER TABLE entity ADD COLUMN archived_at TEXT;

-- Back-reference to the pack-typed table row this entity projects
-- from. NULL for ambient-only entities. (kind, pack_slug,
-- pack_table_id) together identify the source row uniquely.
ALTER TABLE entity ADD COLUMN pack_table_id INTEGER;

-- ---------------------------------------------------------------------
-- Indexes — the queries we'll actually run
-- ---------------------------------------------------------------------

-- "Hide archived" filter sits on every entity query.
CREATE INDEX idx_entity_archived ON entity(archived_at);

-- Pack-projection dedup: "is there already a pack-rooted entity with
-- this slug + table_id?" (slice 6).
CREATE INDEX idx_entity_pack_table ON entity(pack_slug, pack_table_id);

-- Workspace-scoped kind queries: "all person:* entities in the
-- visible workspaces" (slice 12 — People tab).
CREATE INDEX idx_entity_kind_workspace ON entity(kind, workspace_id);

-- Mention timeline: entity detail page pulls events for an entity
-- ordered by occurred_at DESC. CREATE INDEX IF NOT EXISTS so a
-- future migration can add it earlier without conflict.
CREATE INDEX IF NOT EXISTS idx_event_entity ON event(entity_id, occurred_at DESC);

-- Relation traversal: "outgoing edges from X with kind = ..." and
-- the inverse. Co-mention dedup (slice 5) hits both.
CREATE INDEX IF NOT EXISTS idx_relation_from ON relation(from_entity, kind);
CREATE INDEX IF NOT EXISTS idx_relation_to ON relation(to_entity, kind);

-- ---------------------------------------------------------------------

UPDATE meta SET value = '21' WHERE key = 'schema_version';
