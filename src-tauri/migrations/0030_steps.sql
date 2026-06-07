-- v0.14.0 Slice 2 — Step-streaming substrate.
--
-- Every LLM tool call, action handler, and run_python execution
-- produces a stream of typed StepEvents the frontend renders as
-- Claude-style named substeps with checkmarks. This table persists
-- the steps so reopening a conversation re-renders the full history.

CREATE TABLE IF NOT EXISTS step (
    id               TEXT PRIMARY KEY,        -- uuid generated client-side
    conversation_id  INTEGER NOT NULL,
    parent_step_id   TEXT,                    -- for sub-steps (run_python inside a workflow)
    kind             TEXT NOT NULL
        CHECK (kind IN ('tool_call', 'action', 'code_execution', 'thinking', 'workflow_op')),
    name             TEXT NOT NULL,           -- "Reading PO doc"
    detail           TEXT,                    -- "doc#42 (PS 498 PO)"
    status           TEXT NOT NULL DEFAULT 'running'
        CHECK (status IN ('running', 'ok', 'failed', 'cancelled')),
    summary          TEXT,                    -- one-line result
    notes_json       TEXT NOT NULL DEFAULT '[]',  -- array of text notes appended during execution
    started_at       TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    completed_at     TEXT,
    duration_ms      INTEGER
);
CREATE INDEX IF NOT EXISTS idx_step_conversation ON step(conversation_id, started_at);
CREATE INDEX IF NOT EXISTS idx_step_parent ON step(parent_step_id);
