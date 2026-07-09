-- v0.28.19 — voice_utterance. Each intent capture writes a WAV to
-- app_data_dir + a row here linking the audio artifact to the
-- conversation_message. The message renders with a compact audio
-- player + collapsible transcript.

CREATE TABLE IF NOT EXISTS voice_utterance (
  id           INTEGER PRIMARY KEY AUTOINCREMENT,
  message_id   INTEGER NOT NULL REFERENCES conversation_message(id) ON DELETE CASCADE,
  audio_path   TEXT NOT NULL,
  duration_ms  INTEGER NOT NULL,
  transcript   TEXT NOT NULL,
  created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_voice_utterance_message
  ON voice_utterance (message_id);
