-- Step 6 of the pack refactor (PACKS_AUDIT.md). Removes the L2E-specific
-- CHECK constraint on `task.link_kind` so non-L2E packs aren't blocked
-- from using it. Adds `entity_id` as the forward-looking link to the spine.
--
-- The legacy `link_kind` / `link_id` columns are preserved for now —
-- existing rows keep their values and the UI keeps rendering them. Step 8
-- of the refactor (when the L2E pack lifts) will backfill `entity_id`
-- from `link_kind` / `link_id` by joining through the L2E typed tables
-- and the spine `entity` table, then the legacy columns can be retired.
--
-- SQLite can't drop CHECK constraints in place, so we recreate the table.

CREATE TABLE task_new (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    title         TEXT NOT NULL,
    description   TEXT,
    status        TEXT NOT NULL DEFAULT 'open',
    priority      INTEGER NOT NULL DEFAULT 0,
    due_at        TEXT,
    entity_id     INTEGER REFERENCES entity(id) ON DELETE SET NULL,
    link_kind     TEXT,
    link_id       INTEGER,
    source        TEXT NOT NULL DEFAULT 'manual',
    completed_at  TEXT,
    created_at    TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at    TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK (status IN ('open','done','snoozed','dropped')),
    CHECK ((link_kind IS NULL) = (link_id IS NULL))
);

INSERT INTO task_new
    (id, title, description, status, priority, due_at,
     entity_id, link_kind, link_id, source, completed_at,
     created_at, updated_at)
SELECT
     id, title, description, status, priority, due_at,
     NULL, link_kind, link_id, source, completed_at,
     created_at, updated_at
FROM task;

DROP TABLE task;
ALTER TABLE task_new RENAME TO task;

CREATE INDEX idx_task_status ON task(status);
CREATE INDEX idx_task_due    ON task(due_at);
CREATE INDEX idx_task_link   ON task(link_kind, link_id);
CREATE INDEX idx_task_entity ON task(entity_id);

UPDATE meta SET value = '19' WHERE key = 'schema_version';
