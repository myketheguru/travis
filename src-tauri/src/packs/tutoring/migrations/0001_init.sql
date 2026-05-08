-- Tutoring pack — initial schema.
--
-- Vertical: 1:1 tutoring agencies. Tutors paired with students, sessions
-- logged with what was covered, periodic progress reports sent to the
-- parent, billing the parent for hours. Tier A vertical #3 in MARKET.md.
--
-- Pack tables follow the spine pattern: each domain object also has a
-- mirroring entity row populated by pack code (PACKS.md "data model:
-- spine vs typed tables"). Cross-pack queries hit the spine; pack
-- queries hit these typed tables directly.
--
-- This is a per-pack migration tracked in `meta.pack.tutoring.schema_version`,
-- independent of core's `_sqlx_migrations`.

CREATE TABLE IF NOT EXISTS tutor (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT NOT NULL,
    email       TEXT,
    phone       TEXT,
    rate_cents  INTEGER,
    subjects    TEXT,
    notes       TEXT,
    created_at  TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at  TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_tutor_name ON tutor(name);

CREATE TABLE IF NOT EXISTS student (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    name          TEXT NOT NULL,
    grade         TEXT,
    parent_name   TEXT,
    parent_email  TEXT,
    parent_phone  TEXT,
    notes         TEXT,
    created_at    TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at    TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_student_name ON student(name);

CREATE TABLE IF NOT EXISTS session (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    tutor_id          INTEGER NOT NULL REFERENCES tutor(id) ON DELETE CASCADE,
    student_id        INTEGER NOT NULL REFERENCES student(id) ON DELETE RESTRICT,
    subject           TEXT,
    session_date      TEXT NOT NULL,
    duration_minutes  INTEGER NOT NULL CHECK (duration_minutes > 0),
    status            TEXT NOT NULL DEFAULT 'completed'
                      CHECK (status IN ('scheduled','completed','cancelled','no_show')),
    notes             TEXT,
    homework          TEXT,
    created_at        TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at        TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_session_tutor_date   ON session(tutor_id, session_date);
CREATE INDEX IF NOT EXISTS idx_session_student_date ON session(student_id, session_date);
CREATE INDEX IF NOT EXISTS idx_session_status       ON session(status);

CREATE TABLE IF NOT EXISTS progress_report (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    student_id    INTEGER NOT NULL REFERENCES student(id) ON DELETE CASCADE,
    period_start  TEXT NOT NULL,
    period_end    TEXT NOT NULL,
    content       TEXT NOT NULL,
    sent_at       TEXT,
    sent_to       TEXT,
    created_at    TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at    TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK (period_start <= period_end)
);
CREATE INDEX IF NOT EXISTS idx_report_student ON progress_report(student_id, period_end DESC);
