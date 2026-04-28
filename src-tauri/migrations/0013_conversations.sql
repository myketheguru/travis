CREATE TABLE IF NOT EXISTS conversation (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    kind        TEXT NOT NULL DEFAULT 'journal',
    title       TEXT,
    status      TEXT NOT NULL DEFAULT 'open' CHECK(status IN ('open','awaiting_user','resolved')),
    link_kind   TEXT,
    link_id     INTEGER,
    created_at  TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at  TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_conversation_status ON conversation(status, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_conversation_link ON conversation(link_kind, link_id);

CREATE TABLE IF NOT EXISTS conversation_message (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    conversation_id INTEGER NOT NULL REFERENCES conversation(id) ON DELETE CASCADE,
    role            TEXT NOT NULL CHECK(role IN ('user','assistant','system','tool')),
    content         TEXT NOT NULL,
    payload_json    TEXT,
    created_at      TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_conv_msg_thread ON conversation_message(conversation_id, id);

CREATE TABLE IF NOT EXISTS proposed_action (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    conversation_id INTEGER NOT NULL REFERENCES conversation(id) ON DELETE CASCADE,
    kind            TEXT NOT NULL,
    rationale       TEXT,
    params_json     TEXT NOT NULL DEFAULT '{}',
    status          TEXT NOT NULL DEFAULT 'proposed'
                      CHECK(status IN ('proposed','confirmed','declined','applied','failed')),
    result_json     TEXT,
    created_at      TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at      TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_action_conv ON proposed_action(conversation_id, status);

UPDATE meta SET value = '13' WHERE key = 'schema_version';
