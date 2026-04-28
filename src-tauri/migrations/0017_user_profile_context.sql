-- Richer user_profile context so prompts can be templated to any user
-- instead of hardcoding the original customer's domain.
--
-- context_blurb       free-form description of what the user/org does,
--                     who they serve, and what activities Travis should
--                     pay attention to. Embedded verbatim in system
--                     prompts when present.
-- communication_style optional voice/tone guidance ("warm and direct",
--                     "formal", "concise"). Steers Travis's reply style.

ALTER TABLE user_profile ADD COLUMN context_blurb TEXT;
ALTER TABLE user_profile ADD COLUMN communication_style TEXT;

UPDATE meta SET value = '17' WHERE key = 'schema_version';
