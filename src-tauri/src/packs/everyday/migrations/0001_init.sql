-- Everyday pack — Phase 2 tables.
--
-- One table: saved_place. Stores addresses the user cares about
-- with geocoded lat/lng so route_to_place doesn't have to re-geocode
-- every time. Reminders + notes reuse core tables (core reminders,
-- core documents/notes) so they aren't repeated here.

CREATE TABLE IF NOT EXISTS saved_place (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    name                TEXT    NOT NULL,          -- 'Dr. Chen's office', 'Amanda's place'
    address             TEXT    NOT NULL,          -- human-readable address the LLM asked geocode for
    lat                 REAL    NOT NULL,
    lng                 REAL    NOT NULL,
    tags                TEXT    NOT NULL DEFAULT '[]',   -- JSON array of tags
    notes               TEXT,                       -- freeform notes ('parking is around the back')
    created_at          TEXT    NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at          TEXT    NOT NULL DEFAULT CURRENT_TIMESTAMP,
    cloud_id            TEXT
);

CREATE INDEX IF NOT EXISTS idx_saved_place_name
    ON saved_place (LOWER(name));

CREATE INDEX IF NOT EXISTS idx_saved_place_created
    ON saved_place (created_at DESC);
