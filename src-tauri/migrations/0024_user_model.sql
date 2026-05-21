-- Phase 4.5 / BRAIN.md capability #3 — user model.
--
-- A derived snapshot of the user's activity patterns: active hours,
-- typical capture length, cadence, ask-vs-capture ratio. Written by
-- the user_model background pass; consumed by the persona block so
-- Travis adapts timing + length without being told.
--
-- One JSON blob on the single-row user_profile keeps the change
-- additive and reversible (no new table to migrate later). The
-- shape is owned by src/persona/user_model.rs.

ALTER TABLE user_profile ADD COLUMN derived_model_json TEXT;
ALTER TABLE user_profile ADD COLUMN derived_model_at TEXT;
