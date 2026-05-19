-- Lead to Empower pack — quote / margin scenarios (LTE_QUOTE_SPEC.md).
--
-- Second pack-owned migration (meta.pack.lead-to-empower.schema_version
-- = 2). Persists pre-sale pricing scenarios so the user can keep and
-- compare staffing options on a bid (e.g. the 46-school NYCPS HS Math
-- pursuit). The Appendix G cost model itself lives in Rust
-- (pricing.rs) and is exposed conversationally by the read-only
-- `lte_quote_margin` tool; this table stores the *inputs* of a saved
-- scenario. Computed totals are derived on demand by the tool (it can
-- be pointed at a saved quote's inputs) — see LTE_QUOTE_SPEC.md §4 for
-- why stored-computed columns are deferred to a custom quote UI slice.
--
-- workspace_id from the start (AUTHORING_PACKS.md). Auto-CRUD stamps
-- the active workspace on insert.

CREATE TABLE IF NOT EXISTS quote (
    id                      INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id            INTEGER NOT NULL DEFAULT 1,
    name                    TEXT NOT NULL,
    module_id               INTEGER NOT NULL REFERENCES catalog_module(id) ON DELETE RESTRICT,
    engagement_id           INTEGER REFERENCES engagement(id) ON DELETE SET NULL,
    participants            INTEGER NOT NULL DEFAULT 0,
    instructors             INTEGER,
    sessions                INTEGER,
    hours_per_session       REAL,
    facilitator_rate_cents  INTEGER NOT NULL DEFAULT 10000,
    ga_cents                INTEGER NOT NULL DEFAULT 72500,
    material_cents          INTEGER NOT NULL DEFAULT 0,
    rental_cents            INTEGER NOT NULL DEFAULT 0,
    list_price_cents        INTEGER,
    in_kind_cents           INTEGER NOT NULL DEFAULT 0,
    notes                   TEXT,
    created_at              TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at              TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_quote_workspace ON quote(workspace_id);
CREATE INDEX IF NOT EXISTS idx_quote_module ON quote(module_id);
CREATE INDEX IF NOT EXISTS idx_quote_engagement ON quote(engagement_id);
