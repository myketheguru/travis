-- 0038_event_log.sql
--
-- Canonical event log per conversation. OpenHands-inspired substrate
-- for branching, time-travel, condenser-based context-window
-- management, and the reasoning-vs-action UI distinction.
--
-- v0.17.0 lands the substrate + dual-write from the agent loop. The
-- existing `conversation_message` table stays the canonical UI read
-- path for now; events are written alongside as ground truth. A
-- future slice can flip reads to project from events, at which point
-- `conversation_message` becomes a (regenerable) view.
--
-- Distinct from `memory_edge` (0037): that's typed relations between
-- artifacts/claims for memory retrieval. This is the immutable
-- conversation history.

CREATE TABLE event (
  id                INTEGER PRIMARY KEY AUTOINCREMENT,
  conversation_id   INTEGER NOT NULL REFERENCES conversation(id) ON DELETE CASCADE,

  -- One of: user_message, agent_response, tool_call, tool_result,
  -- thinking, condensation, error. New kinds are additive — readers
  -- ignore kinds they don't recognise.
  kind              TEXT NOT NULL,

  -- Kind-specific JSON. The contract for each kind lives in
  -- src/events/mod.rs (EventKind enum + payload structs).
  payload_json      TEXT,

  -- Parent event for branching. NULL on the first event in a
  -- conversation. Future "time-travel"/"branch from here" UI follows
  -- parent_event_id back up the chain.
  parent_event_id   INTEGER REFERENCES event(id) ON DELETE SET NULL,

  -- Optional pointer back to the legacy conversation_message row
  -- that mirrors this event. Lets the dual-write keep both tables
  -- consistent and lets future reads switch sides without losing
  -- identity.
  message_id        INTEGER REFERENCES conversation_message(id) ON DELETE SET NULL,

  created_at        TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_event_conv
  ON event (conversation_id, id);

CREATE INDEX idx_event_kind
  ON event (conversation_id, kind, id);
