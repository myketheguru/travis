-- Task 313 — MCP transport in desktop LLM tool registry.
--
-- One row per configured MCP server. Startup iterates enabled rows,
-- calls tools/list on each server, wraps returned tools as McpTool
-- and registers them in the read-only tool registry so the LLM can
-- invoke them alongside built-in tools.
--
-- auth_token is stored plaintext — desktop-only DB, user's machine.
-- NEVER include this row in sync_outbox: MCP creds are per-machine,
-- not synced.

CREATE TABLE IF NOT EXISTS mcp_server (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    slug          TEXT    NOT NULL UNIQUE,   -- 'slack', 'github' — kept short; namespaced as mcp_<slug>_<tool>
    label         TEXT    NOT NULL,          -- 'Slack', 'GitHub'
    url           TEXT    NOT NULL,          -- full https URL to server's JSON-RPC endpoint
    auth_token    TEXT,                      -- optional bearer token
    enabled       INTEGER NOT NULL DEFAULT 1,
    created_at    TEXT    NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_mcp_server_enabled
    ON mcp_server (enabled DESC, created_at ASC);
