//! MCP JSON-RPC 2.0 HTTP client.
//!
//! Implements the two operations Travis actually needs today:
//!   - `tools/list`  — discover the tools a server exposes
//!   - `tools/call`  — invoke a tool with typed args, get typed result
//!
//! Skips: SSE notifications, resources, prompts, sampling. If we ever
//! need bidirectional streaming, upgrade to SSE.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone)]
pub struct McpClient {
    http: reqwest::Client,
    url: String,
    /// Optional bearer token for authenticated servers.
    auth: Option<String>,
}

impl McpClient {
    pub fn new(http: reqwest::Client, url: String, auth: Option<String>) -> Self {
        Self { http, url, auth }
    }

    /// GET/POST agnostic JSON-RPC call. Sends the payload as POST +
    /// application/json, returns the parsed `result` object. Errors
    /// map JSON-RPC errors to anyhow errors with the code + message.
    async fn call(&self, method: &str, params: Value) -> Result<Value> {
        let payload = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params,
        });
        let mut req = self.http.post(&self.url).json(&payload);
        if let Some(token) = &self.auth {
            req = req.bearer_auth(token);
        }
        let resp = req.send().await?.error_for_status()?;
        let body: RpcResponse = resp.json().await?;
        match (body.result, body.error) {
            (Some(r), _) => Ok(r),
            (_, Some(e)) => Err(anyhow!("MCP error {}: {}", e.code, e.message)),
            _ => Err(anyhow!("MCP: response missing both result and error")),
        }
    }

    /// List all tools this server exposes. Returned tools should be
    /// wrapped in [`crate::mcp::McpTool`] and registered.
    pub async fn list_tools(&self) -> Result<Vec<McpToolDescriptor>> {
        let result = self.call("tools/list", json!({})).await?;
        // Standard MCP response: { tools: [{ name, description, inputSchema }] }
        let tools = result
            .get("tools")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let mut out = Vec::with_capacity(tools.len());
        for t in tools {
            out.push(serde_json::from_value(t)?);
        }
        Ok(out)
    }

    /// Invoke a tool. Returns the raw result value from the server —
    /// caller stringifies for the LLM.
    pub async fn call_tool(&self, name: &str, arguments: Value) -> Result<Value> {
        self.call(
            "tools/call",
            json!({
                "name": name,
                "arguments": arguments,
            }),
        )
        .await
    }
}

// ─── Wire types ───────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct RpcResponse {
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<RpcError>,
}

#[derive(Debug, Deserialize)]
struct RpcError {
    code: i32,
    message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolDescriptor {
    /// Machine name — used by tools/call and shown to the LLM.
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    /// JSON Schema for the tool's arguments.
    #[serde(default)]
    pub input_schema: Value,
}
