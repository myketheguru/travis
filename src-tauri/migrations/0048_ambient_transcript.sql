-- v0.28.4 — ambient_transcript. Passive-capture speech transcripts
-- for meetings, calls, and thinking-out-loud. When the user has
-- ambient listening on, every VAD-bounded utterance is transcribed
-- + saved here. Travis can query these via the
-- `get_ambient_transcripts` tool to answer "what was decided in the
-- meeting?" or "what did they say about Q4?".
--
-- Kept intentionally minimal: no session/tag/entity linkage yet.
-- The LLM tool queries by time window; UI browses newest-first.

CREATE TABLE IF NOT EXISTS ambient_transcript (
  id           INTEGER PRIMARY KEY AUTOINCREMENT,
  text         TEXT NOT NULL,
  occurred_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_ambient_transcript_occurred_at
  ON ambient_transcript (occurred_at DESC);
