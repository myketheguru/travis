-- v0.15.3 — python_artifact: persisted record of every run_python call.
--
-- Why: lets Travis iterate on a generated document by editing the prior
-- script instead of regenerating from scratch. When the user says
-- "remove the note", "add 7 hours to row 1", "the signature line a
-- tiny bit down", the LLM looks up the prior artifact's script, makes
-- a small edit, and re-runs — and the new row points back at the
-- prior via superseded_by so the lineage is diff-able.
--
-- This is the substrate that v0.16's typed-edge memory graph will
-- wire EVOLVED_INTO edges onto (Claude.ai-parity research, AutoMem
-- pattern).

CREATE TABLE python_artifact (
    id                   INTEGER PRIMARY KEY,
    conversation_id      INTEGER,
    workspace_id         INTEGER NOT NULL,
    purpose              TEXT    NOT NULL,
    script               TEXT    NOT NULL,
    -- JSON array of input document IDs (mounted at /inputs/)
    input_doc_ids        TEXT    NOT NULL DEFAULT '[]',
    -- JSON array of generated document IDs (from /outputs/)
    output_document_ids  TEXT    NOT NULL DEFAULT '[]',
    stdout               TEXT,
    stderr               TEXT,
    execution_ms         INTEGER,
    error                TEXT,
    -- Self-reference: if this artifact superseded a prior one (via
    -- edit_python_artifact), points back to the original. Null for
    -- first-of-lineage artifacts.
    superseded_by        INTEGER REFERENCES python_artifact(id) ON DELETE SET NULL,
    created_at           TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_python_artifact_conv ON python_artifact(conversation_id);
CREATE INDEX idx_python_artifact_ws   ON python_artifact(workspace_id);
CREATE INDEX idx_python_artifact_superseded ON python_artifact(superseded_by);
