-- v0.20.12 — first-class plan / step substrate.
--
-- Before this migration the agent loop was monolithic: every turn
-- replayed the same expensive sequence (read sheet, filter dates,
-- generate PDF). The IS 217 invoice trace showed 50+ run_python
-- calls re-doing work the prior turn had already done. The chat
-- surface had no concept of "this step was completed; use the
-- cached output."
--
-- A `plan` is a goal-scoped sequence of named steps the LLM
-- creates at the top of a complex turn. A `plan_step` records
-- what was attempted, its status, its cached result, and (when
-- applicable) the document ids it produced. Subsequent calls to
-- run a step with the same key return the cached result instantly
-- rather than re-executing.

CREATE TABLE IF NOT EXISTS plan (
  id              INTEGER PRIMARY KEY AUTOINCREMENT,
  conversation_id INTEGER NOT NULL REFERENCES conversation(id) ON DELETE CASCADE,
  goal            TEXT NOT NULL,
  -- 'active' | 'completed' | 'abandoned' | 'failed'
  status          TEXT NOT NULL DEFAULT 'active',
  created_at      TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_plan_conversation ON plan(conversation_id, status);

CREATE TABLE IF NOT EXISTS plan_step (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  plan_id       INTEGER NOT NULL REFERENCES plan(id) ON DELETE CASCADE,
  -- Short stable identifier the LLM picks: 'read_sample_styling',
  -- 'extract_service_dates', 'generate_invoice_pdf'. The pair
  -- (plan_id, key) is unique so the LLM calling the same key
  -- twice returns the cached output.
  key           TEXT NOT NULL,
  -- One-line human description rendered in the chat as the step
  -- label ("Extracting IS 217 service dates from the sign-in log").
  purpose       TEXT NOT NULL,
  -- 'pending' | 'running' | 'done' | 'failed' | 'skipped'
  status        TEXT NOT NULL DEFAULT 'pending',
  -- Comma-separated list of prior step keys this step depends on.
  -- The planner enforces ordering when set.
  depends_on    TEXT,
  -- Cached output of the step. Whatever the underlying action
  -- produced (run_python result JSON, read_document text, etc.).
  result_json   TEXT,
  -- Newline-joined list of document ids this step generated.
  -- Plucked from run_python's generatedDocumentIds when applicable.
  document_ids  TEXT,
  -- SHA-256 of the result_json for cross-plan dedup. Future work
  -- (v0.21+) — same input hash across plans → reuse the result.
  result_hash   TEXT,
  error         TEXT,
  started_at    TEXT,
  completed_at  TEXT,
  created_at    TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at    TEXT NOT NULL DEFAULT (datetime('now')),
  UNIQUE (plan_id, key)
);

CREATE INDEX IF NOT EXISTS idx_plan_step_status ON plan_step(plan_id, status);
CREATE INDEX IF NOT EXISTS idx_plan_step_hash ON plan_step(result_hash);
