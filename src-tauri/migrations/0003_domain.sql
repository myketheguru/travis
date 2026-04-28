CREATE TABLE IF NOT EXISTS coach (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT NOT NULL,
    email       TEXT,
    rate_cents  INTEGER,
    notes       TEXT,
    created_at  TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at  TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_coach_name ON coach(name);

CREATE TABLE IF NOT EXISTS school (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    name          TEXT NOT NULL,
    district      TEXT,
    contact_name  TEXT,
    contact_email TEXT,
    notes         TEXT,
    created_at    TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at    TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_school_name ON school(name);

CREATE TABLE IF NOT EXISTS coach_hours (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    coach_id      INTEGER NOT NULL REFERENCES coach(id) ON DELETE CASCADE,
    school_id     INTEGER NOT NULL REFERENCES school(id) ON DELETE RESTRICT,
    session_date  TEXT NOT NULL,
    hours         REAL NOT NULL CHECK (hours > 0),
    description   TEXT,
    created_at    TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at    TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_hours_coach_date ON coach_hours(coach_id, session_date);
CREATE INDEX IF NOT EXISTS idx_hours_school_date ON coach_hours(school_id, session_date);

CREATE TABLE IF NOT EXISTS signing_sheet (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    coach_id      INTEGER NOT NULL REFERENCES coach(id) ON DELETE CASCADE,
    school_id     INTEGER NOT NULL REFERENCES school(id) ON DELETE RESTRICT,
    period_start  TEXT NOT NULL,
    period_end    TEXT NOT NULL,
    signed_at     TEXT,
    signed_by     TEXT,
    pdf_path      TEXT,
    notes         TEXT,
    created_at    TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at    TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK (period_start <= period_end)
);
CREATE INDEX IF NOT EXISTS idx_sheet_period ON signing_sheet(coach_id, school_id, period_start, period_end);

CREATE TABLE IF NOT EXISTS invoice (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    number            TEXT UNIQUE NOT NULL,
    recipient         TEXT NOT NULL,
    coach_id          INTEGER REFERENCES coach(id) ON DELETE SET NULL,
    school_id         INTEGER REFERENCES school(id) ON DELETE SET NULL,
    signing_sheet_id  INTEGER REFERENCES signing_sheet(id) ON DELETE SET NULL,
    period_start      TEXT NOT NULL,
    period_end        TEXT NOT NULL,
    hours_total       REAL NOT NULL DEFAULT 0,
    rate_cents        INTEGER NOT NULL DEFAULT 0,
    amount_cents      INTEGER NOT NULL DEFAULT 0,
    status            TEXT NOT NULL DEFAULT 'draft',
    issued_at         TEXT,
    paid_at           TEXT,
    notes             TEXT,
    created_at        TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at        TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK (period_start <= period_end),
    CHECK (status IN ('draft','sent','paid','void'))
);
CREATE INDEX IF NOT EXISTS idx_invoice_status ON invoice(status);
CREATE INDEX IF NOT EXISTS idx_invoice_coach ON invoice(coach_id);

CREATE TABLE IF NOT EXISTS task (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    title         TEXT NOT NULL,
    description   TEXT,
    status        TEXT NOT NULL DEFAULT 'open',
    priority      INTEGER NOT NULL DEFAULT 0,
    due_at        TEXT,
    link_kind     TEXT,
    link_id       INTEGER,
    source        TEXT NOT NULL DEFAULT 'manual',
    completed_at  TEXT,
    created_at    TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at    TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK (status IN ('open','done','snoozed','dropped')),
    CHECK (link_kind IS NULL OR link_kind IN ('coach','school','coach_hours','signing_sheet','invoice')),
    CHECK ((link_kind IS NULL) = (link_id IS NULL))
);
CREATE INDEX IF NOT EXISTS idx_task_status ON task(status);
CREATE INDEX IF NOT EXISTS idx_task_due ON task(due_at);
CREATE INDEX IF NOT EXISTS idx_task_link ON task(link_kind, link_id);

UPDATE meta SET value = '3' WHERE key = 'schema_version';
