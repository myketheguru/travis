-- 0040_pack_memory.sql
--
-- Per-pack memory store. v0.19.0.
--
-- The user wants pack-scoped "rules" / "preferences" / "facts" Travis
-- can store and recall — Claude.ai's Project Knowledge / Memory but
-- pack-aware. A user-stated constraint about a school's invoicing
-- ("never include March 17 service dates for IS 217 — pre-PO window")
-- lands here and is surfaced into future system prompts when that
-- school or contract is in scope.
--
-- Each row is scoped at three increasingly-specific levels:
--   pack_slug                 — applies to every conversation that
--                               touches this pack.
--   target_kind + target_id   — applies only when this specific
--                               entity is in scope (e.g. school#42,
--                               contract#7). The kind is the spine
--                               entity kind ("school", "contract",
--                               "engagement", "coach", …); the id is
--                               the spine entity row id, NOT the pack
--                               table id, so the rule survives pack
--                               table renames or restructures.
--
-- `kind` categorises the memory:
--   rule        — hard constraint ("never invoice before PO window")
--   preference  — soft preference ("Taylor likes payment terms Net 30")
--   constraint  — operational ("DoF route requires PO# in subject line")
--   fact        — context Travis should know ("IS 217 has 2 contracts:
--                 math + ELA — disambiguate before invoicing")
--   correction  — something Travis got wrong before
--                 ("called the school 'Performing Arts' — it's 'IS 217'")
--
-- Relevance decay (mirrors `claim` table from v0.16.3) lets older
-- memories fade — a rule from a year ago about a closed engagement
-- doesn't need to anchor every prompt. New writes start at relevance
-- 1.0; the daily decay job halves them every 180 days. Below floor
-- 0.05 the memory is archived (kept on disk, not loaded into prompt).
-- `pinned = 1` blocks decay for memories the user explicitly marked
-- as permanent.

CREATE TABLE pack_memory (
  id                INTEGER PRIMARY KEY AUTOINCREMENT,
  workspace_id      INTEGER NOT NULL REFERENCES workspace(id) ON DELETE CASCADE,
  pack_slug         TEXT NOT NULL,

  -- One of: rule, preference, constraint, fact, correction.
  kind              TEXT NOT NULL,

  -- Optional entity scope. Both NULL ⇒ pack-wide. Both set ⇒ scoped
  -- to that specific spine entity (target_kind = entity.kind,
  -- target_id = entity.id).
  target_kind       TEXT,
  target_id         INTEGER,

  -- The actual memory text the LLM stored. Keep dense; the system
  -- prompt has limited room.
  content           TEXT NOT NULL,

  -- Where this memory came from — useful for debugging recall.
  -- 'chat' means the LLM wrote it via remember_constraint. 'manual'
  -- means the user typed it in a settings UI. 'extraction' means a
  -- background extraction pass found it.
  source            TEXT NOT NULL DEFAULT 'chat',

  -- Conversation that birthed this memory (for "go to source" links
  -- in the memory-review UI). NULL when source = 'manual'.
  conversation_id   INTEGER REFERENCES conversation(id) ON DELETE SET NULL,

  -- Decay state. New = 1.0, halves every 180 days; archived below
  -- 0.05. `pinned = 1` skips decay.
  relevance_score   REAL NOT NULL DEFAULT 1.0,
  pinned            INTEGER NOT NULL DEFAULT 0,

  created_at        TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
  updated_at        TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Hot path: load memories for a pack in this workspace, scoped to
-- entities currently in conversation context.
CREATE INDEX idx_pack_memory_lookup
  ON pack_memory (workspace_id, pack_slug, target_kind, target_id);

CREATE INDEX idx_pack_memory_kind
  ON pack_memory (workspace_id, pack_slug, kind, relevance_score DESC);
