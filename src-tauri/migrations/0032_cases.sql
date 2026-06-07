-- v0.14.0 Slice 6 — Long-running cases.
--
-- Working memory's 30-minute TTL is way too short for real ops work
-- — Taylor's PS 89 reconciliation conversation with Claude ran across
-- 3 days with mid-stream corrections. A "case" is a named multi-
-- session work unit that survives across conversations, holds a
-- rolling summary + decisions + artifact references, and lets Travis
-- pick up where he left off.

CREATE TABLE IF NOT EXISTS travis_case (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id      INTEGER NOT NULL DEFAULT 1,
    name              TEXT NOT NULL,         -- "PS 89 invoice #3"
    summary           TEXT,                  -- rolling 2-4 sentence state-of-play
    status            TEXT NOT NULL DEFAULT 'open'
        CHECK (status IN ('open', 'paused', 'closed')),
    parent_case_id    INTEGER REFERENCES travis_case(id) ON DELETE SET NULL,
    -- Conversations that touched this case. Soft FK; just a tag.
    conversation_ids_json TEXT NOT NULL DEFAULT '[]',
    started_at        TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_activity_at  TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    closed_at         TEXT,
    -- Per-workspace uniqueness on open cases prevents accidental dupes
    UNIQUE (workspace_id, name, status)
);
CREATE INDEX IF NOT EXISTS idx_case_workspace ON travis_case(workspace_id);
CREATE INDEX IF NOT EXISTS idx_case_status ON travis_case(status);
CREATE INDEX IF NOT EXISTS idx_case_activity ON travis_case(last_activity_at);

CREATE TABLE IF NOT EXISTS case_artifact (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    case_id      INTEGER NOT NULL REFERENCES travis_case(id) ON DELETE CASCADE,
    kind         TEXT NOT NULL,    -- 'document' | 'decision' | 'note' | 'reconciliation' | 'output'
    payload_json TEXT NOT NULL,    -- shape depends on kind
    document_id  INTEGER,          -- when kind='document' or 'output'
    created_at   TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_case_artifact_case ON case_artifact(case_id, created_at);
