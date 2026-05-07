-- Pack spine: generic primitives every vertical has in common, used as a
-- rendezvous point between core and pack-typed tables.
--
-- See PACKS.md "The data model: spine vs typed tables" for the full
-- rationale. Three concepts:
--   entity   — generic record (coach, client, case, job, invoice, etc.)
--   relation — typed edge between entities
--   event    — anything that happened, optionally tied to an entity
--
-- Pack code keeps the spine in sync with its own typed tables via explicit
-- writes (no triggers — easier to read). Cross-pack retrieval queries the
-- spine; pack-internal queries hit the typed tables directly.

-- Generalize entity_index → entity. SQLite can't drop CHECK constraints in
-- place, so we recreate the table. We also add `pack_slug` so cross-pack
-- queries can filter by origin.

CREATE TABLE entity (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    kind             TEXT NOT NULL,
    normalized_name  TEXT NOT NULL,
    display_name     TEXT NOT NULL,
    pack_slug        TEXT,
    mentions_count   INTEGER NOT NULL DEFAULT 0,
    first_seen       TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_seen        TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    attributes_json  TEXT,
    UNIQUE(kind, normalized_name)
);

INSERT INTO entity
    (id, kind, normalized_name, display_name,
     mentions_count, first_seen, last_seen, attributes_json)
SELECT
     id, kind, normalized_name, display_name,
     mentions_count, first_seen, last_seen, attributes_json
FROM entity_index;

DROP TABLE entity_index;

CREATE INDEX idx_entity_kind_mentions ON entity(kind, mentions_count DESC);
CREATE INDEX idx_entity_pack          ON entity(pack_slug);

-- Typed edges between entities. `kind` is the relation type, e.g.
-- "works_at", "billed_to", "supervises". Soft string — packs declare what
-- kinds they care about; no enforced enum.
CREATE TABLE relation (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    from_entity      INTEGER NOT NULL REFERENCES entity(id) ON DELETE CASCADE,
    to_entity        INTEGER NOT NULL REFERENCES entity(id) ON DELETE CASCADE,
    kind             TEXT NOT NULL,
    pack_slug        TEXT,
    attributes_json  TEXT,
    created_at       TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX idx_relation_from ON relation(from_entity, kind);
CREATE INDEX idx_relation_to   ON relation(to_entity,   kind);
CREATE INDEX idx_relation_kind ON relation(kind);

-- Anything that happened. Powers activity timelines, audit trails, and
-- pack-supplied summary contributions. `entity_id` is optional because some
-- events (e.g., "user opened the app") aren't tied to a domain entity.
CREATE TABLE event (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    entity_id        INTEGER REFERENCES entity(id) ON DELETE SET NULL,
    kind             TEXT NOT NULL,
    pack_slug        TEXT,
    occurred_at      TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    attributes_json  TEXT,
    created_at       TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX idx_event_entity ON event(entity_id, occurred_at DESC);
CREATE INDEX idx_event_kind   ON event(kind,      occurred_at DESC);
CREATE INDEX idx_event_pack   ON event(pack_slug, occurred_at DESC);

UPDATE meta SET value = '18' WHERE key = 'schema_version';
