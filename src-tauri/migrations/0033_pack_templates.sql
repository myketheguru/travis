-- v0.14.0 Slice 7 — pack_template memory.
--
-- When run_python produces a great document (sample-matching invoice,
-- custom sign-in sheet) and the user confirms it's right, Travis saves
-- the styling JSON + working Python so the NEXT time the same
-- counterparty needs that document, the code path is instant — no
-- re-analyzing the sample, no re-writing the script.
--
-- Templates are scoped by (workspace, kind, counterparty_hint) — so
-- "IS 217 invoice", "PS 89 sign-in sheet", and "default sign-in sheet"
-- can all coexist.

CREATE TABLE IF NOT EXISTS pack_template (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id        INTEGER NOT NULL DEFAULT 1,
    pack_slug           TEXT NOT NULL,             -- 'lead-to-empower' usually
    kind                TEXT NOT NULL,             -- 'invoice' | 'sign_in_sheet' | 'work_order' | 'other'
    label               TEXT NOT NULL,             -- user-given name: "IS 217 invoice layout"
    counterparty_hint   TEXT,                      -- "IS 217" or "PS 89" — used to find this template later
    styling_json        TEXT NOT NULL,             -- cached styling features from analyze_document_styling
    generation_code     TEXT NOT NULL,             -- the working Python (reusable on next run)
    sample_document_id  INTEGER REFERENCES document(id) ON DELETE SET NULL,
    used_count          INTEGER NOT NULL DEFAULT 0,
    last_used_at        TEXT,
    created_at          TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at          TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (workspace_id, pack_slug, kind, label)
);
CREATE INDEX IF NOT EXISTS idx_pack_template_lookup
    ON pack_template(workspace_id, pack_slug, kind, counterparty_hint);
CREATE INDEX IF NOT EXISTS idx_pack_template_used
    ON pack_template(last_used_at DESC);
