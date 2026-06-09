-- 0039_message_response_kind.sql
--
-- v0.17.0 — Reasoning-only response card UI (#177).
--
-- Adds a classifier slot to conversation_message. The agent loop in
-- journal.rs stamps this after each turn using
-- events::classify_response. Values:
--
--   extraction      — finished work delivered (the typical case)
--   text_response   — answer / clarifying question without an
--                     extraction call
--   reasoning_only  — thinking blocks + planning text but no tool
--                     call; surfaces as a distinct chat card
--
-- Nullable so historical messages (and any agent-loop path that
-- forgets to stamp) read as untyped → UI falls back to the existing
-- bubble shape. New code should always stamp.

ALTER TABLE conversation_message
  ADD COLUMN response_kind TEXT;
