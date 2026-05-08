-- Tutoring pack — Phase 2 workspace scoping (WORKSPACES.md).
--
-- Adds workspace_id to every Tutoring-pack typed table. Runs after
-- core's 0020_workspaces.sql, which created the `workspace` table
-- and seeded a default Personal workspace at id=1. Existing tutoring
-- rows backfill into Personal.
--
-- Future pack migrations that add tables MUST include workspace_id
-- from the start. This migration only exists for tables that
-- predate workspace scoping. See AUTHORING_PACKS.md.

ALTER TABLE tutor             ADD COLUMN workspace_id INTEGER NOT NULL DEFAULT 1;
CREATE INDEX idx_tutor_workspace ON tutor(workspace_id);

ALTER TABLE student           ADD COLUMN workspace_id INTEGER NOT NULL DEFAULT 1;
CREATE INDEX idx_student_workspace ON student(workspace_id);

ALTER TABLE session           ADD COLUMN workspace_id INTEGER NOT NULL DEFAULT 1;
CREATE INDEX idx_session_workspace ON session(workspace_id);

ALTER TABLE progress_report   ADD COLUMN workspace_id INTEGER NOT NULL DEFAULT 1;
CREATE INDEX idx_progress_report_workspace ON progress_report(workspace_id);
