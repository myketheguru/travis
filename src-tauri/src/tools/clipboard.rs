use async_trait::async_trait;
use serde_json::{json, Value};
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::llm::ToolDef;

use super::{Tool, ToolContext};

pub struct ReadClipboardTool;

#[async_trait]
impl Tool for ReadClipboardTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "read_clipboard".into(),
            description: "Read the current text contents of the user's system clipboard. Use when the user says things like 'summarize what I just copied' or 'process this' without pasting it inline.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, _input: Value) -> anyhow::Result<String> {
        let text = ctx
            .app
            .clipboard()
            .read_text()
            .map_err(|e| anyhow::anyhow!("clipboard read: {e}"))?;
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Ok("(clipboard is empty)".into());
        }
        // Cap to keep things sane.
        let truncated: String = trimmed.chars().take(8000).collect();
        let elided = if trimmed.chars().count() > 8000 {
            "\n\n[…truncated]"
        } else {
            ""
        };
        Ok(format!("Clipboard contents:\n{truncated}{elided}"))
    }
}
