-- v0.28.21 — household pack. Grocery lists, errands, chores.

CREATE TABLE IF NOT EXISTS grocery_item (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  workspace_id  INTEGER NOT NULL DEFAULT 1,
  name          TEXT NOT NULL,
  quantity      TEXT,             -- '2 lbs', '1 gallon', 'a bunch'
  category      TEXT,             -- 'produce', 'dairy', 'pantry', 'household'
  store         TEXT,             -- optional preferred store
  purchased_at  TEXT,             -- when bought; NULL means still on list
  created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_grocery_open ON grocery_item(purchased_at);

CREATE TABLE IF NOT EXISTS chore (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  workspace_id  INTEGER NOT NULL DEFAULT 1,
  name          TEXT NOT NULL,
  cadence       TEXT,             -- 'daily', 'weekly', 'monthly', 'as-needed'
  assigned_to   TEXT,
  last_done_at  TEXT,
  notes         TEXT,
  created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
