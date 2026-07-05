//! Model Context Protocol (MCP) client.
//!
//! MCP is Anthropic's open standard for LLM tool interop:
//! https://modelcontextprotocol.io. Servers expose typed tools, the
//! Travis desktop registers each as a first-class tool in the LLM
//! registry — the LLM can call `slack_send`, `github_create_pr`, etc.
//! without Travis needing to bake each integration in.
//!
//! Transport: HTTP JSON-RPC 2.0 with POST-only calls. Full MCP spec
//! also supports SSE for server->client notifications; we skip that
//! for MVP since Travis is call-response, not streaming.
//!
//! Config: server list stored in `mcp_server` table (see migration
//! 0056). Startup iterates the list, calls `tools/list`, and wraps
//! each returned tool as an [`McpTool`] in the LLM ToolRegistry.

pub mod client;
pub mod cmd;
pub mod db;
pub mod tool;

pub use client::McpClient;
pub use tool::McpTool;
