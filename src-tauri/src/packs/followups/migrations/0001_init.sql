-- v0.28.21 — follow-ups pack. Commitments the user made in
-- conversation ('I'll send you the deck', 'let me get back to you').
-- Cross-references with the Sent folder eventually.

CREATE TABLE IF NOT EXISTS followup (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  workspace_id  INTEGER NOT NULL DEFAULT 1,
  title         TEXT NOT NULL,           -- 'send Sarah the Q3 deck'
  person        TEXT,                    -- 'Sarah Chen' (display_name from contact)
  due_by        TEXT,                    -- optional ISO date
  status        TEXT NOT NULL DEFAULT 'open',  -- open, done, dropped
  source        TEXT,                    -- how it was captured: 'user', 'ambient', 'inbox'
  notes         TEXT,
  created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  completed_at  TEXT
);

CREATE INDEX IF NOT EXISTS idx_followup_status ON followup(status);
CREATE INDEX IF NOT EXISTS idx_followup_person ON followup(person);
