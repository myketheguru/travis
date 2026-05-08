-- Phase 2 — Workspaces (WORKSPACES.md). A workspace is a scoped
-- namespace for operational data. Every existing scoped table gets
-- a `workspace_id` column; existing rows are backfilled into a
-- default `Personal` workspace so users see no behaviour change on
-- upgrade.
--
-- Scoped tables (this migration adds workspace_id to each):
--   Core: task, reminder, journal_entry, embedding, conversation,
--         entity, relation, event, summary, email_sent.
--   L2E pack tables (created by 0003_domain.sql): coach, school,
--         coach_hours, signing_sheet, invoice.
--
-- Tutoring pack tables (tutor, student, session, progress_report)
-- get their workspace_id column from a per-pack migration that
-- runs after this one — see src-tauri/src/packs/tutoring/migrations
-- /0002_workspace_id.sql.
--
-- Tables intentionally NOT scoped (per WORKSPACES.md):
--   user_profile, meta, oauth_account, smtp_config, behavioral
--   event_log + detected_pattern, app_feedback, proposed_action,
--   conversation_message (derives from conversation), telemetry_event,
--   feature flags.

CREATE TABLE workspace (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    slug          TEXT NOT NULL UNIQUE,
    name          TEXT NOT NULL,
    category      TEXT NOT NULL DEFAULT 'personal'
                  CHECK (category IN
                    ('work','personal','health','therapy',
                     'legal','finance','other')),
    cross_visible INTEGER NOT NULL DEFAULT 1
                  CHECK (cross_visible IN (0, 1)),
    archived_at   TEXT,
    created_at    TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at    TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX idx_workspace_archived ON workspace(archived_at);

-- Default workspace — every existing row backfills into this one.
-- Gets id=1 because it's the first INSERT into the AUTOINCREMENT
-- table; the ADD COLUMN clauses below default new workspace_id
-- columns to 1 to ensure backfill.
INSERT INTO workspace (slug, name, category, cross_visible)
VALUES ('personal', 'Personal', 'personal', 1);

-- Active workspace — read at startup into AppState.active_workspace_id.
INSERT INTO meta (key, value, updated_at)
VALUES ('active_workspace_id', '1', CURRENT_TIMESTAMP)
ON CONFLICT(key) DO UPDATE SET
    value = excluded.value,
    updated_at = CURRENT_TIMESTAMP;

-- ---------------------------------------------------------------------
-- Core tables
-- ---------------------------------------------------------------------

ALTER TABLE task           ADD COLUMN workspace_id INTEGER NOT NULL DEFAULT 1;
CREATE INDEX idx_task_workspace ON task(workspace_id);

ALTER TABLE reminder       ADD COLUMN workspace_id INTEGER NOT NULL DEFAULT 1;
CREATE INDEX idx_reminder_workspace ON reminder(workspace_id);

ALTER TABLE journal_entry  ADD COLUMN workspace_id INTEGER NOT NULL DEFAULT 1;
CREATE INDEX idx_journal_entry_workspace ON journal_entry(workspace_id);

-- Denormalised on embedding for query speed — semantic-memory search
-- filters by workspace at scan time without joining through
-- journal_entry. The pack journal-extractor stamps embedding rows
-- with the journal entry's workspace_id at insert time.
ALTER TABLE embedding      ADD COLUMN workspace_id INTEGER NOT NULL DEFAULT 1;
CREATE INDEX idx_embedding_workspace ON embedding(workspace_id);

ALTER TABLE conversation   ADD COLUMN workspace_id INTEGER NOT NULL DEFAULT 1;
CREATE INDEX idx_conversation_workspace ON conversation(workspace_id);

ALTER TABLE entity         ADD COLUMN workspace_id INTEGER NOT NULL DEFAULT 1;
CREATE INDEX idx_entity_workspace ON entity(workspace_id);

ALTER TABLE relation       ADD COLUMN workspace_id INTEGER NOT NULL DEFAULT 1;
CREATE INDEX idx_relation_workspace ON relation(workspace_id);

ALTER TABLE event          ADD COLUMN workspace_id INTEGER NOT NULL DEFAULT 1;
CREATE INDEX idx_event_workspace ON event(workspace_id);

ALTER TABLE summary        ADD COLUMN workspace_id INTEGER NOT NULL DEFAULT 1;
CREATE INDEX idx_summary_workspace ON summary(workspace_id);

ALTER TABLE email_sent     ADD COLUMN workspace_id INTEGER NOT NULL DEFAULT 1;
CREATE INDEX idx_email_sent_workspace ON email_sent(workspace_id);

-- ---------------------------------------------------------------------
-- L2E pack tables (defined in 0003_domain.sql)
-- ---------------------------------------------------------------------

ALTER TABLE coach          ADD COLUMN workspace_id INTEGER NOT NULL DEFAULT 1;
CREATE INDEX idx_coach_workspace ON coach(workspace_id);

ALTER TABLE school         ADD COLUMN workspace_id INTEGER NOT NULL DEFAULT 1;
CREATE INDEX idx_school_workspace ON school(workspace_id);

ALTER TABLE coach_hours    ADD COLUMN workspace_id INTEGER NOT NULL DEFAULT 1;
CREATE INDEX idx_coach_hours_workspace ON coach_hours(workspace_id);

ALTER TABLE signing_sheet  ADD COLUMN workspace_id INTEGER NOT NULL DEFAULT 1;
CREATE INDEX idx_signing_sheet_workspace ON signing_sheet(workspace_id);

ALTER TABLE invoice        ADD COLUMN workspace_id INTEGER NOT NULL DEFAULT 1;
CREATE INDEX idx_invoice_workspace ON invoice(workspace_id);

UPDATE meta SET value = '20' WHERE key = 'schema_version';
