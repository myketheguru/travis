-- v0.21.4 Tier 1 — inbox triage.
--
-- One row per classified inbox message. Populated by
-- crate::email::inbox_observer running every 5 minutes against the
-- user's connected Gmail / Outlook account.
--
-- Persisted so the splash feed can show "since you last checked,
-- here's what arrived" without re-classifying on every open. Noise
-- rows are not persisted (filtered in the observer before insert).

CREATE TABLE IF NOT EXISTS inbox_triage (
    id                      INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id            INTEGER NOT NULL DEFAULT 1,

    -- Provider attribution + identity. Composite uniqueness key.
    provider                TEXT NOT NULL,
    provider_message_id     TEXT NOT NULL,
    thread_id               TEXT,

    -- Snapshot of the message at triage time. The provider's view of
    -- the message can change later (read state, labels) — we don't
    -- chase it; this is a triage snapshot.
    from_email              TEXT NOT NULL,
    from_name               TEXT,
    subject                 TEXT NOT NULL,
    received_at             TEXT NOT NULL,
    is_unread               INTEGER NOT NULL DEFAULT 0,

    -- Triage output.
    urgency                 TEXT NOT NULL,             -- 'urgent' | 'important' | 'fyi'
    summary_one_line        TEXT NOT NULL,
    suggested_next_step     TEXT,

    -- Lifecycle.
    handled_at              TEXT,                       -- when the user dismissed / actioned it
    created_at              TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,

    UNIQUE(workspace_id, provider, provider_message_id)
);

CREATE INDEX IF NOT EXISTS idx_inbox_triage_recent
    ON inbox_triage(workspace_id, received_at DESC);

CREATE INDEX IF NOT EXISTS idx_inbox_triage_urgent_unhandled
    ON inbox_triage(workspace_id, urgency, handled_at)
    WHERE urgency = 'urgent' AND handled_at IS NULL;
