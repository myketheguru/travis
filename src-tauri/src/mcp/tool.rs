//! `McpTool` — adapter that wraps a remote MCP tool as a local
//! [`Tool`] the LLM registry can invoke.
//!
//! The wrapper carries the server URL + tool name + descriptor.
//! On execute, it constructs a fresh MCP client (cheap, shares the
//! ToolContext's reqwest::Client) and calls `tools/call`. The
//! response is stringified as JSON for the LLM.

use async_trait::async_trait;
use serde_json::Value;

use crate::llm::ToolDef;
use crate::mcp::client::{McpClient, McpToolDescriptor};
use crate::tools::{Tool, ToolContext};

pub struct McpTool {
    server_url: String,
    auth: Option<String>,
    descriptor: McpToolDescriptor,
    /// Namespaced local name to avoid collisions across servers.
    /// Format: `mcp_<server_slug>_<tool_name>`.
    local_name: String,
}

impl McpTool {
    pub fn new(
        server_url: String,
        server_slug: &str,
        auth: Option<String>,
        descriptor: McpToolDescriptor,
    ) -> Self {
        let local_name = format!("mcp_{}_{}", server_slug, descriptor.name);
        Self {
            server_url,
            auth,
            descriptor,
            local_name,
        }
    }
}

#[async_trait]
impl Tool for McpTool {
    fn definition(&self) -> ToolDef {
        let desc_str = self
            .descriptor
            .description
            .as_deref()
            .unwrap_or("Remote MCP tool.");
        ToolDef {
            name: self.local_name.clone(),
            description: format!("[MCP] {}", desc_str),
            input_schema: self.descriptor.input_schema.clone(),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let client = McpClient::new(
            ctx.http.clone(),
            self.server_url.clone(),
            self.auth.clone(),
        );
        let result = client.call_tool(&self.descriptor.name, input).await?;
        // Stringify — LLMs handle JSON gracefully as tool output.
        Ok(serde_json::to_string(&result)?)
    }
}
