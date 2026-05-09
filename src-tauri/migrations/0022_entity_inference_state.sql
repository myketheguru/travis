-- Phase 4 slice 9 — track when Travis last asked the user to
-- refine an entity's role. Used by the recurring-mention nudge to
-- avoid re-prompting more often than every 30 days, regardless of
-- how many subsequent mentions accumulate.
--
-- NULL = never prompted; ISO-8601 timestamp = last prompt time.

ALTER TABLE entity ADD COLUMN last_clarification_at TEXT;

-- Indexed because the candidate-finder filters by this column.
CREATE INDEX idx_entity_last_clarification
    ON entity(last_clarification_at);

UPDATE meta SET value = '22' WHERE key = 'schema_version';
