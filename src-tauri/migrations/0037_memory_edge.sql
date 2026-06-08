-- v0.16.4 — Typed-edge memory graph (AutoMem-style).
--
-- Per the v0.15.x research synthesis, AutoMem's typed edge vocabulary
-- (LEADS_TO, EVOLVED_INTO, DERIVED_FROM, etc.) is the cross-document
-- reconciliation primitive Travis needs — far more useful than flat
-- embedding similarity. Maps 1:1 to L2E invoicing: PO LEADS_TO
-- invoice, invoice DERIVED_FROM sign-in sheet, amendment EVOLVED_INTO
-- from contract, corrected total INVALIDATES prior one.
--
-- Distinct from the existing `relation` table (entity-to-entity edges
-- in the pack spine) — that's for persistent entity graph topology
-- (works_at, supervises, etc.). `memory_edge` is for *artifact /
-- claim / document* lineage and temporal reasoning. Different
-- vocabulary, different lifecycle.
--
-- The 11 AutoMem-style relationship types are stored as TEXT for
-- forward compatibility — Travis can invent new types during
-- reasoning if needed, but the canonical set is:
--   RELATES_TO        — generic association (when no stronger applies)
--   LEADS_TO          — temporal/causal: PO LEADS_TO invoice
--   OCCURRED_BEFORE   — temporal ordering: invoice #1 OCCURRED_BEFORE #2
--   PREFERS_OVER      — choice: user PREFERS_OVER plain vs separated invoice #
--   EXEMPLIFIES       — Pattern node: this invoice EXEMPLIFIES "L2E DoF format"
--   CONTRADICTS       — two claims that can't both be true
--   REINFORCES        — additional evidence for an existing claim
--   INVALIDATED_BY    — formally superseded: corrected total INVALIDATES prior
--   EVOLVED_INTO      — versioned: artifact v1 EVOLVED_INTO v2
--   DERIVED_FROM      — provenance: invoice DERIVED_FROM sign-in sheet
--   PART_OF           — composition: invoice PART_OF case
--
-- Each edge links two nodes — represented by (kind, id) pairs so
-- the same table can connect across document/claim/case/artifact
-- without proliferating join tables.

CREATE TABLE memory_edge (
    id          INTEGER PRIMARY KEY,
    workspace_id INTEGER NOT NULL,
    -- Source side
    from_kind   TEXT NOT NULL,  -- 'document', 'claim', 'case', 'artifact', 'entity', etc.
    from_id     INTEGER NOT NULL,
    -- Target side
    to_kind     TEXT NOT NULL,
    to_id       INTEGER NOT NULL,
    -- Relationship type — see canonical list above
    relation    TEXT NOT NULL,
    -- Optional metadata blob (JSON). Confidence, source attribution,
    -- discovered_at timestamp, etc.
    attributes_json TEXT,
    created_at  TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_memory_edge_from
    ON memory_edge(from_kind, from_id);
CREATE INDEX idx_memory_edge_to
    ON memory_edge(to_kind, to_id);
CREATE INDEX idx_memory_edge_relation
    ON memory_edge(relation);
CREATE INDEX idx_memory_edge_workspace
    ON memory_edge(workspace_id);

-- Prevent exact-duplicate edges (same source + target + relation).
-- Allows multiple relations between the same pair (PO LEADS_TO invoice
-- + PO PART_OF case — both valid simultaneously).
CREATE UNIQUE INDEX idx_memory_edge_unique
    ON memory_edge(workspace_id, from_kind, from_id, to_kind, to_id, relation);
