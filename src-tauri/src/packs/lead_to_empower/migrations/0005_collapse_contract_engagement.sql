-- Collapse the contract <-> engagement distinction per Taylor's
-- feedback (2026-06-04): "engagement and contract is too broad and
-- they might even mean the same thing."
--
-- The two-table design (master contract → many engagements) was an
-- abstraction layer I added that didn't survive contact with her
-- real work. In her vocabulary, ONE contract IS ONE piece of work
-- at one school. Multiple contracts per school is normal (math
-- contract + science contract); a "team" is a scope item within a
-- contract, not a sub-engagement.
--
-- Migration shape:
--   1. ALTER engagement to absorb every contract-shape field.
--   2. Backfill those fields from any standalone contract row that's
--      already linked via engagement.contract_id.
--   3. For standalone contracts with NO linked engagement, synthesise
--      an engagement row so no data is lost.
--   4. Leave the standalone contract table in place (don't drop in
--      this migration — a follow-up cleanup will remove it once we
--      confirm nothing reads from it). All NEW writes go to engagement.
--
-- Pack schema_version 5. Pack version bumped 0.6.0 -> 0.7.0.

-- ---------------------------------------------------------------------------
-- 1. Additive columns on engagement (the new unified "contract" record).
-- ---------------------------------------------------------------------------
ALTER TABLE engagement ADD COLUMN ref                  TEXT;
ALTER TABLE engagement ADD COLUMN ceiling_cents        INTEGER NOT NULL DEFAULT 0;
ALTER TABLE engagement ADD COLUMN term_start           TEXT;
ALTER TABLE engagement ADD COLUMN term_end             TEXT;
ALTER TABLE engagement ADD COLUMN signed_at            TEXT;
ALTER TABLE engagement ADD COLUMN parent_solicitation  TEXT;
ALTER TABLE engagement ADD COLUMN pdf_path             TEXT;
ALTER TABLE engagement ADD COLUMN counterparty         TEXT;
ALTER TABLE engagement ADD COLUMN contract_status      TEXT NOT NULL DEFAULT 'active'
    CHECK (contract_status IN ('draft','active','expired','terminated','archived'));

-- ---------------------------------------------------------------------------
-- 2. Backfill from the standalone contract table for any engagement
--    that's already linked.
-- ---------------------------------------------------------------------------
UPDATE engagement
SET ref = (SELECT ref FROM contract WHERE contract.id = engagement.contract_id),
    ceiling_cents = COALESCE(
        (SELECT ceiling_cents FROM contract WHERE contract.id = engagement.contract_id), 0
    ),
    term_start = (SELECT term_start FROM contract WHERE contract.id = engagement.contract_id),
    term_end = (SELECT term_end FROM contract WHERE contract.id = engagement.contract_id),
    signed_at = (SELECT signed_at FROM contract WHERE contract.id = engagement.contract_id),
    parent_solicitation = (
        SELECT parent_solicitation FROM contract WHERE contract.id = engagement.contract_id
    ),
    pdf_path = (SELECT pdf_path FROM contract WHERE contract.id = engagement.contract_id),
    counterparty = (
        SELECT counterparty FROM contract WHERE contract.id = engagement.contract_id
    ),
    contract_status = COALESCE(
        (SELECT status FROM contract WHERE contract.id = engagement.contract_id), 'active'
    )
WHERE contract_id IS NOT NULL;

-- ---------------------------------------------------------------------------
-- 3. Synthesise engagement rows for standalone contracts that don't
--    yet have one. These appear with school_id NULL until Taylor
--    refines them — better than silently dropping data.
--    Re-runnable via the NOT IN guard.
-- ---------------------------------------------------------------------------
INSERT INTO engagement (
    workspace_id, name, school_id, stage, contract_ref, ref,
    ceiling_cents, term_start, term_end, signed_at,
    parent_solicitation, pdf_path, counterparty, contract_status,
    contract_id
)
SELECT
    c.workspace_id,
    COALESCE(c.name, c.ref) AS name,
    NULL AS school_id,
    'accountable' AS stage,
    c.ref AS contract_ref,
    c.ref,
    COALESCE(c.ceiling_cents, 0),
    c.term_start,
    c.term_end,
    c.signed_at,
    c.parent_solicitation,
    c.pdf_path,
    c.counterparty,
    COALESCE(c.status, 'active'),
    c.id
FROM contract c
WHERE c.id NOT IN (
    SELECT contract_id FROM engagement WHERE contract_id IS NOT NULL
);

-- ---------------------------------------------------------------------------
-- 4. Indexes on the new columns we'll filter on.
-- ---------------------------------------------------------------------------
CREATE INDEX IF NOT EXISTS idx_engagement_ref ON engagement(workspace_id, ref);
CREATE INDEX IF NOT EXISTS idx_engagement_contract_status
    ON engagement(workspace_id, contract_status);
CREATE INDEX IF NOT EXISTS idx_engagement_term_end ON engagement(term_end);
