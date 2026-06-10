-- 0006_engagement_terms.sql
--
-- v0.20.0 — promote period + ceiling from `summary` text-stash to
-- typed columns on `engagement`.
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
ALTER TABLE engagement ADD COLUMN ceiling_cents INTEGER;

CREATE INDEX IF NOT EXISTS idx_engagement_period
  ON engagement(period_start, period_end);
