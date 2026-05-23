-- BRAIN.md capability #4 — collaboration: initiatives layer.
--
-- A typed cluster of related work — "April invoicing push",
-- "audit response", "NYCPS HS Math bid". Tasks and conversations
-- can optionally tag an initiative so Travis can pick up where
-- the user left off across sessions.
--
-- Workspace-scoped from the start. Soft FKs onto core tables;
-- existing rows keep NULL and behave unchanged.

CREATE TABLE IF NOT EXISTS initiative (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id    INTEGER NOT NULL DEFAULT 1,

    name            TEXT NOT NULL,
    -- One-paragraph rolling summary Travis maintains as the
    -- initiative progresses. Optional at create time.
    summary         TEXT,

    status          TEXT NOT NULL DEFAULT 'active'
        CHECK (status IN ('active', 'paused', 'closed')),

    -- Who is holding the next move: travis | user | external (a
    -- third party we're waiting on — school principal, vendor).
    -- Optional; used by the resumption block in the prompt.
    owner_kind      TEXT
        CHECK (owner_kind IN ('travis', 'user', 'external')),
    -- Free-text label for owner ("Carol Ann Gilligan", "Jacob",
    -- empty when owner_kind = user/travis).
    owner_label     TEXT,

    -- Reference into a typed entity (the school the initiative is
    -- centred on, the contract, etc.). Nullable; not enforced as
    -- a hard FK since the spine entity table is shared across packs.
    entity_id       INTEGER REFERENCES entity(id) ON DELETE SET NULL,

    -- Notes from Travis: last decision, what changed since last
    -- contact, open questions. Free-form text; the LLM curates.
    last_decision   TEXT,
    open_questions  TEXT,

    last_activity_at TEXT,
    closed_at       TEXT,
    created_at      TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at      TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_initiative_workspace ON initiative(workspace_id);
CREATE INDEX IF NOT EXISTS idx_initiative_status ON initiative(status);
CREATE INDEX IF NOT EXISTS idx_initiative_entity ON initiative(entity_id);
CREATE INDEX IF NOT EXISTS idx_initiative_activity ON initiative(last_activity_at);

-- Tasks can optionally tag an initiative. Soft FK; existing rows
-- keep NULL.
ALTER TABLE task ADD COLUMN initiative_id INTEGER
    REFERENCES initiative(id) ON DELETE SET NULL;
CREATE INDEX IF NOT EXISTS idx_task_initiative ON task(initiative_id);

-- Same for conversations. Lets the chat thread restoration look
-- up the right initiative-themed thread without scanning text.
ALTER TABLE conversation ADD COLUMN initiative_id INTEGER
    REFERENCES initiative(id) ON DELETE SET NULL;
CREATE INDEX IF NOT EXISTS idx_conversation_initiative ON conversation(initiative_id);
