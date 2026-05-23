-- BRAIN.md capability #7 — wellbeing: affect signals.
--
-- Light tone + theme tracking pulled from each journal capture by
-- the LLM. The observer reads these to detect patterns (a theme
-- dwelt-on for days, a sudden drained streak). The proactive thread
-- can then surface ONE observation — not therapy, not wellness
-- performance, just a colleague noticing.
--
-- Privacy posture (load-bearing): this table is the most sensitive
-- thing the user generates. NEVER included in data exports.
-- NEVER reachable through pack_query / pack_introspect (it's not a
-- pack table, and the schema lives in core; the export module
-- filters by name regardless). Workspace-scoped from the start so
-- sensitive workspaces stay isolated.

CREATE TABLE IF NOT EXISTS affect_signal (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id        INTEGER NOT NULL DEFAULT 1,
    journal_entry_id    INTEGER NOT NULL REFERENCES journal_entry(id) ON DELETE CASCADE,

    tone                TEXT NOT NULL DEFAULT 'neutral'
        CHECK (tone IN ('concerned','energised','drained','stuck','neutral')),

    -- 1-3 short phrases naming the worries / topics the user keeps
    -- returning to. JSON array of strings. Optional.
    themes_json         TEXT,

    created_at          TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_affect_signal_workspace ON affect_signal(workspace_id);
CREATE INDEX IF NOT EXISTS idx_affect_signal_entry ON affect_signal(journal_entry_id);
CREATE INDEX IF NOT EXISTS idx_affect_signal_created ON affect_signal(created_at);
CREATE INDEX IF NOT EXISTS idx_affect_signal_tone ON affect_signal(tone);
