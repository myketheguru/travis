//! `preview_document` — LLM-invokable tool that opens a document in
//! the OS default viewer.
//!
//! When Taylor says "show me that invoice" or "open the PO", Travis
//! calls this tool. The file opens in Preview/Acrobat/whatever — no
//! manual navigation needed.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::Manager;

use crate::documents::db as docs_db;
use crate::documents::storage;
use crate::llm::ToolDef;
use crate::tools::{Tool, ToolContext};
use crate::AppState;

pub struct PreviewDocumentTool;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    document_id: i64,
}

#[async_trait]
impl Tool for PreviewDocumentTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "preview_document".into(),
            description: "Open a document with the OS default viewer (Preview, \
                Acrobat, browser — whatever handles the file's mime type). Use when \
                the user asks to see / open / view / preview a specific document \
                they've ingested or that Travis generated. Returns the absolute \
                path that was opened."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "documentId": { "type": "integer" }
                },
                "required": ["documentId"]
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        use tauri_plugin_opener::OpenerExt;
        let p: Input = serde_json::from_value(input)?;
        let state = ctx.app.state::<AppState>();
        let doc = docs_db::get(&state.db.pool, p.document_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("document {} not found", p.document_id))?;

        let data_dir = ctx
            .app
            .path()
            .app_data_dir()
            .map_err(|e| anyhow::anyhow!("app data dir: {e}"))?;
        let storage_root = storage::storage_root(&data_dir)?;
        let abs = storage::absolute_path(&storage_root, std::path::Path::new(&doc.relative_path));
        let abs_str = abs.to_string_lossy().into_owned();

        ctx.app
            .opener()
            .open_path(abs_str.clone(), None::<&str>)
            .map_err(|e| anyhow::anyhow!("opener failed: {e}"))?;

        Ok(serde_json::to_string(&json!({
            "ok": true,
            "openedPath": abs_str,
            "documentId": p.document_id,
        }))?)
    }
}
