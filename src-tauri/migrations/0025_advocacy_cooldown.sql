-- BRAIN.md capability #6 — self-advocacy cooldown.
--
-- The journal extractor already records capability_gaps into
-- app_feedback per turn. What's missing is the surfacing loop:
-- when the same gap fires repeatedly, Travis should once say "I
-- keep stalling on X because Y isn't set up — want to fix that?"
-- and then not pester again until either the user addresses it or
-- a fresh-evidence cooldown passes.
--
-- This column stamps every app_feedback row when a surface fires
-- against its capability. The query that surfaces gaps excludes
-- capabilities whose recent rows are still inside the cooldown.

ALTER TABLE app_feedback ADD COLUMN last_advocacy_surfaced_at TEXT;
CREATE INDEX IF NOT EXISTS idx_app_feedback_capability
    ON app_feedback(capability);
CREATE INDEX IF NOT EXISTS idx_app_feedback_advocacy
    ON app_feedback(capability, last_advocacy_surfaced_at);
