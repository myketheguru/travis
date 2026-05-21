-- Phase 4.5 #2 + #7 — claims layer + entity_facts column.
--
-- These two land together because they're complementary substrate:
--   facts = LLM-extracted typed claims about an entity (role,
--           relationship, contact, context, etc.) persisted in
--           entity.attributes_json under a `facts` key
--   claim = a persisted reasoning conclusion Travis itself derived,
--           with confidence + source attribution
--
-- Facts go in attributes_json (already exists; just a JSON convention
-- the extractor and consolidator agree on — see journal.rs).
-- Claims need a typed table so they can be indexed, decayed, merged.
--
-- Schema-version bumped to 23.

CREATE TABLE IF NOT EXISTS claim (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id    INTEGER NOT NULL DEFAULT 1,

    -- Subject: the entity this claim is about. Required.
    entity_id       INTEGER NOT NULL REFERENCES entity(id) ON DELETE CASCADE,

    -- Optional secondary entity (for relational claims —
    -- "Maria coaches at PS 142" has both Maria and PS 142).
    other_entity_id INTEGER REFERENCES entity(id) ON DELETE SET NULL,

    -- The predicate vocabulary. Open-ended on purpose; common values:
    -- 'role', 'relationship', 'contact', 'context', 'preference',
    -- 'derived'. Travis can invent new ones during reasoning.
    predicate       TEXT NOT NULL,

    -- The claim's value. Free text — what Travis concluded.
    -- "math coach at PS 142", "prefers email", "usually replies within
    -- 24h", etc.
    value           TEXT NOT NULL,

    -- Confidence band (matches ConfidenceBand: 'high'|'medium'|'low').
    -- Travis writes 'low' for one-data-point inferences and grades up
    -- as more evidence accumulates.
    confidence      TEXT NOT NULL DEFAULT 'medium'
        CHECK (confidence IN ('high','medium','low')),

    -- Where the claim came from: 'extraction' (LLM extracted from a
    -- single capture), 'derived' (Travis reasoned over multiple
    -- events), 'user_confirmed' (the user explicitly confirmed),
    -- 'consolidation' (the periodic consolidation tick consolidated
    -- many mentions into this).
    source          TEXT NOT NULL DEFAULT 'extraction'
        CHECK (source IN ('extraction','derived','user_confirmed','consolidation')),

    -- Optional pointer to the journal_entry or event that seeded the
    -- claim. Lets Travis answer "where did you get that?" honestly.
    source_journal_entry_id INTEGER,

    -- Soft contradiction flag. When Travis discovers two claims that
    -- can't both be true (Maria works at PS 142 vs. Maria works at PS 207),
    -- both rows are kept but `contested = 1` so retrieval can surface
    -- the conflict.
    contested       INTEGER NOT NULL DEFAULT 0,

    -- Soft delete for the consolidation pass — when a stale claim is
    -- superseded by a fresher one, set superseded_at rather than
    -- DELETE so we keep a history.
    superseded_at   TEXT,

    created_at      TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at      TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_claim_workspace ON claim(workspace_id);
CREATE INDEX IF NOT EXISTS idx_claim_entity ON claim(entity_id);
CREATE INDEX IF NOT EXISTS idx_claim_other ON claim(other_entity_id);
CREATE INDEX IF NOT EXISTS idx_claim_predicate ON claim(predicate);
CREATE INDEX IF NOT EXISTS idx_claim_active ON claim(entity_id, superseded_at);
CREATE INDEX IF NOT EXISTS idx_claim_contested ON claim(contested) WHERE contested = 1;

-- entity.last_consolidated_at — when did the consolidation pass last
-- summarise this entity's events into attributes_json. NULL = never.
-- Phase 4.5 #3 reads + writes this to amortise the consolidation work.
ALTER TABLE entity ADD COLUMN last_consolidated_at TEXT;
CREATE INDEX IF NOT EXISTS idx_entity_consolidation
    ON entity(last_consolidated_at);
