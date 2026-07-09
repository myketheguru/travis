-- v0.28.21 — people pack. Contacts as first-class entities so
-- follow-ups, birthdays, and relationship-aware retrieval have a home.

CREATE TABLE IF NOT EXISTS contact (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  workspace_id  INTEGER NOT NULL DEFAULT 1,
  display_name  TEXT NOT NULL,
  first_name    TEXT,
  last_name     TEXT,
  email         TEXT,
  phone         TEXT,
  relationship  TEXT,        -- 'friend', 'family', 'coworker', 'client', 'partner', 'other'
  organization  TEXT,        -- company / school / church / etc.
  birthday      TEXT,        -- ISO date (YYYY-MM-DD); year optional (--MM-DD)
  notes         TEXT,
  last_contact_at TEXT,
  created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  updated_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_contact_display_name ON contact(display_name);
CREATE INDEX IF NOT EXISTS idx_contact_workspace ON contact(workspace_id);
