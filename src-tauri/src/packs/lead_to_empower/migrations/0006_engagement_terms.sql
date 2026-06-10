-- 0006_engagement_terms.sql
--
-- v0.20.0 — promote period from `summary` text-stash to typed columns
-- on `engagement`. v0.20.4 hotfix: ceiling_cents removed from this
-- migration — it was already added by 0005_collapse_contract_engagement
-- (NOT NULL DEFAULT 0), so re-adding it here crashed on first launch
-- with "duplicate column name ceiling_cents".
--
-- v0.19.4 introduced the `engagement_enrichment` extraction field
-- that the LLM emits whenever a PO/WO doc reveals business terms
-- (activity window, ceiling dollars). v0.19.5's policy work clarified
-- that these are critical fields needing typed access for filtering,
-- alerts, and the Manage UI's relationship drill-down.
--
-- SQLite ALTER TABLE ADD COLUMN supports nullable defaults, which
-- is what we want here — existing engagement rows pre-date the
-- enrichment pipeline and stay unaffected (summary text remains for
-- audit until v0.21 prunes it).

ALTER TABLE engagement ADD COLUMN period_start TEXT;
ALTER TABLE engagement ADD COLUMN period_end   TEXT;

CREATE INDEX IF NOT EXISTS idx_engagement_period
  ON engagement(period_start, period_end);
