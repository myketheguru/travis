-- v0.15.4 — error_event: structured persistence for LLM / tool / parse
-- errors so the user can see what's going wrong instead of staring at
-- a generic "Travis hit an error" message in the chat.
--
-- Rows are written whenever a fail-soft path fires in journal_ingest
-- (LLM 4xx, tool input parse failure, agent-loop iter cap, retry-also-
-- empty, etc.). The Diagnostics UI reads recent rows and lets the user
-- copy them for bug reports.

CREATE TABLE error_event (
    id              INTEGER PRIMARY KEY,
    conversation_id INTEGER,
    -- Short category: 'llm_api', 'parse', 'iter_cap', 'tool_call',
    -- 'capture_bg', 'other'. Used by UI for filtering and colour.
    kind            TEXT    NOT NULL,
    -- One-line summary the UI shows in the list view.
    message         TEXT    NOT NULL,
    -- Optional JSON payload with whatever context we had — LLM raw
    -- response, tool input, err_msg from the agent loop, etc.
    detail_json     TEXT,
    -- Where in the codebase the error fired ('journal::retry',
    -- 'journal::agent_loop', 'capture::run_background', etc.).
    source          TEXT,
    created_at      TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_error_event_conv    ON error_event(conversation_id);
CREATE INDEX idx_error_event_kind    ON error_event(kind);
CREATE INDEX idx_error_event_created ON error_event(created_at DESC);
