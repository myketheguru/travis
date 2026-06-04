-- Workflow recipes + dialogue manager (Slice 1 of the docs/workflow
-- substrate). Travis runs a recipe per active conversation: when the
-- user expresses intent ("invoice PS498 for Jan-Feb"), Travis identifies
-- the recipe, checks what slots are already known, asks for what's
-- missing, and finalises through the existing action-handler path.
--
-- One active workflow per conversation for v1. Multi-workflow stacking
-- can come later if real usage demands it.

CREATE TABLE IF NOT EXISTS workflow_state (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    conversation_id     INTEGER NOT NULL,
    recipe_name         TEXT NOT NULL,
    status              TEXT NOT NULL DEFAULT 'active'
        CHECK (status IN ('active', 'completed', 'abandoned')),

    -- JSON map: slot_name -> {value: <any>, source: 'user_typed' |
    -- 'graph_resolved' | 'extracted' | 'user_dropped', resolved_at: TEXT}
    -- LLM fills these via the workflowOps extraction field; Rust persists.
    slots_json          TEXT NOT NULL DEFAULT '{}',

    -- Optional rationale Travis recorded when starting — the user-stated
    -- intent in their own words. Helps with abandonment reasoning.
    started_intent      TEXT,

    started_at          TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_activity_at    TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    completed_at        TEXT
);
CREATE INDEX IF NOT EXISTS idx_workflow_state_conv
    ON workflow_state(conversation_id, status);
CREATE INDEX IF NOT EXISTS idx_workflow_state_recipe
    ON workflow_state(recipe_name, status);
