-- v0.28.21 — finance pack. Bills, subscriptions, receipts.

CREATE TABLE IF NOT EXISTS bill (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  workspace_id  INTEGER NOT NULL DEFAULT 1,
  name          TEXT NOT NULL,               -- 'ConEd', 'Verizon', 'Landlord'
  amount_cents  INTEGER,
  currency      TEXT DEFAULT 'USD',
  cadence       TEXT NOT NULL DEFAULT 'monthly',  -- monthly, quarterly, yearly, one-time
  due_day       INTEGER,                     -- day of month for recurring bills
  next_due_at   TEXT,
  paid_last_at  TEXT,
  autopay       INTEGER NOT NULL DEFAULT 0,
  notes         TEXT,
  created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_bill_due ON bill(next_due_at);

CREATE TABLE IF NOT EXISTS subscription (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  workspace_id  INTEGER NOT NULL DEFAULT 1,
  name          TEXT NOT NULL,               -- 'Netflix', 'Adobe CC', 'gym'
  amount_cents  INTEGER,
  currency      TEXT DEFAULT 'USD',
  cadence       TEXT NOT NULL DEFAULT 'monthly',
  next_renewal_at TEXT,
  status        TEXT NOT NULL DEFAULT 'active',  -- active, cancelled, paused
  category      TEXT,                        -- streaming, software, health, etc.
  notes         TEXT,
  created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_subscription_renewal ON subscription(next_renewal_at);
CREATE INDEX IF NOT EXISTS idx_subscription_status ON subscription(status);
