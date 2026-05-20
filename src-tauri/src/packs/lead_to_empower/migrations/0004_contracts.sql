-- Lead to Empower pack — first-class contracts.
--
-- LTE_INVOICING_SPEC v0.4.0 deferred a `contract` table on the
-- "don't abstract on n=1" guardrail. Reality check from the COO:
-- multiple master agreements run in parallel, not one. The guardrail
-- doesn't apply past n=1, so the abstraction is overdue rather than
-- premature.
--
-- This migration:
--   1. CREATEs `contract` — typed master-agreement table with
--      term, ceiling, status, parent_solicitation.
--   2. Backfills one row per distinct `engagement.contract_ref`
--      string (workspace-scoped) so existing data inherits a
--      contract row automatically.
--   3. ALTERs engagement / work_order / purchase_order to add
--      a soft FK `contract_id`. `contract_ref` stays as the
--      display field; the FK becomes the source of truth.
--   4. UPDATEs the new FKs from the existing string columns.
--
-- All ALTERs are additive + nullable so pre-existing rows that
-- never carried a contract_ref keep `NULL` and render as
-- "unassigned" in the UI.
--
-- Pack schema_version 4. Pack version bumped 0.4.0 -> 0.5.0.

-- ---------------------------------------------------------------------------
-- contract — the master agreement. One row per distinct contract;
-- engagements (and through them, work orders, POs, invoices) roll up
-- under a contract for ceiling/expiry reporting.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS contract (
    id                     INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id           INTEGER NOT NULL DEFAULT 1,
    ref                    TEXT NOT NULL,
    name                   TEXT,
    counterparty           TEXT,
    parent_solicitation    TEXT,
    term_start             TEXT,
    term_end               TEXT,
    ceiling_cents          INTEGER NOT NULL DEFAULT 0,
    signed_at              TEXT,
    status                 TEXT NOT NULL DEFAULT 'active'
        CHECK (status IN ('draft','active','expired','terminated','archived')),
    notes                  TEXT,
    pdf_path               TEXT,
    created_at             TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at             TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (workspace_id, ref)
);
CREATE INDEX IF NOT EXISTS idx_contract_workspace ON contract(workspace_id);
CREATE INDEX IF NOT EXISTS idx_contract_status ON contract(status);
CREATE INDEX IF NOT EXISTS idx_contract_term_end ON contract(term_end);

-- ---------------------------------------------------------------------------
-- Backfill: one contract row per distinct (workspace_id, contract_ref)
-- already in engagement. Re-runnable via INSERT OR IGNORE (UNIQUE
-- constraint on workspace_id+ref handles dupes). `name` defaults to
-- the ref so the auto-CRUD list view shows something useful before
-- the user adds detail.
-- ---------------------------------------------------------------------------
INSERT OR IGNORE INTO contract (workspace_id, ref, name, status)
SELECT DISTINCT
    workspace_id,
    TRIM(contract_ref) AS ref,
    TRIM(contract_ref) AS name,
    'active' AS status
FROM engagement
WHERE contract_ref IS NOT NULL
  AND TRIM(contract_ref) != '';

-- ---------------------------------------------------------------------------
-- Additive FKs. Soft (ON DELETE SET NULL) so deleting a contract
-- leaves orphaned engagements visible rather than cascading away
-- historical data — Taylor can re-link or archive.
-- ---------------------------------------------------------------------------
ALTER TABLE engagement ADD COLUMN contract_id INTEGER
    REFERENCES contract(id) ON DELETE SET NULL;
CREATE INDEX IF NOT EXISTS idx_engagement_contract ON engagement(contract_id);

ALTER TABLE work_order ADD COLUMN contract_id INTEGER
    REFERENCES contract(id) ON DELETE SET NULL;
CREATE INDEX IF NOT EXISTS idx_work_order_contract ON work_order(contract_id);

ALTER TABLE purchase_order ADD COLUMN contract_id INTEGER
    REFERENCES contract(id) ON DELETE SET NULL;
CREATE INDEX IF NOT EXISTS idx_purchase_order_contract ON purchase_order(contract_id);

-- ---------------------------------------------------------------------------
-- Backfill the new FKs by joining on the string ref / engagement_id.
-- engagement / work_order point at contract directly via shared
-- contract_ref; purchase_order inherits via its engagement.
-- Subquery form rather than UPDATE...FROM for SQLite compatibility.
-- ---------------------------------------------------------------------------
UPDATE engagement
SET contract_id = (
    SELECT c.id FROM contract c
    WHERE c.workspace_id = engagement.workspace_id
      AND c.ref = TRIM(engagement.contract_ref)
    LIMIT 1
)
WHERE contract_ref IS NOT NULL
  AND TRIM(contract_ref) != '';

UPDATE work_order
SET contract_id = (
    SELECT c.id FROM contract c
    WHERE c.workspace_id = work_order.workspace_id
      AND c.ref = TRIM(work_order.contract_ref)
    LIMIT 1
)
WHERE contract_ref IS NOT NULL
  AND TRIM(contract_ref) != '';

UPDATE purchase_order
SET contract_id = (
    SELECT e.contract_id FROM engagement e
    WHERE e.id = purchase_order.engagement_id
);
