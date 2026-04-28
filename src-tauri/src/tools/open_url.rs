use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use tauri_plugin_opener::OpenerExt;

use crate::llm::ToolDef;

use super::{Tool, ToolContext};

pub struct OpenUrlTool;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    url: String,
}

#[async_trait]
impl Tool for OpenUrlTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "open_url".into(),
            description: "Open a URL in the user's default browser. Use sparingly — only when the user explicitly asked to be taken somewhere or when handing off a link is the obvious next step.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "Full http(s) URL." }
                },
                "required": ["url"]
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: Input = serde_json::from_value(input)?;
        let url = p.url.trim();
        if !url.starts_with("http://") && !url.starts_with("https://") {
            anyhow::bail!("url must start with http:// or https://");
        }
        ctx.app
            .opener()
            .open_url(url, None::<&str>)
            .map_err(|e| anyhow::anyhow!("open: {e}"))?;
        Ok(format!("Opened {url} in the default browser."))
    }
}
