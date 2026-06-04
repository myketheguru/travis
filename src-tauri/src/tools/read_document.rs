//! `read_document` — surfaces a previously-ingested document's
//! extracted JSON (and optional raw text snippet) to the LLM.
//!
//! Use when the user references a document by id ("look at doc#42")
//! or when the dialogue manager has linked a document to the active
//! workflow and Travis needs to reason over its fields.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::Manager;

use crate::documents::db as docs_db;
use crate::llm::ToolDef;
use crate::tools::{Tool, ToolContext};
use crate::AppState;

pub struct ReadDocumentTool;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    document_id: i64,
}

#[async_trait]
impl Tool for ReadDocumentTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "read_document".into(),
            description: "Read a previously-ingested document by its id. Returns its \
                kind, display name, file size, ingest status, and (when extracted) the \
                structured JSON the extractor produced — e.g. PO number, line items, \
                signed dates. Use when the user references a doc by '#id' or when the \
                active workflow has a document slot filled."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "documentId": { "type": "integer", "description": "The document's id (e.g. from a workflow slot or list_documents)." }
                },
                "required": ["documentId"]
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: Input = serde_json::from_value(input)?;
        let state = ctx.app.state::<AppState>();
        let doc = docs_db::get(&state.db.pool, p.document_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("document {} not found", p.document_id))?;

        let extracted = doc
            .extracted_json
            .as_deref()
            .and_then(|s| serde_json::from_str::<Value>(s).ok());

        let payload = json!({
            "id": doc.id,
            "kind": doc.kind,
            "displayName": doc.display_name,
            "originalFilename": doc.original_filename,
            "mimeType": doc.mime_type,
            "sizeBytes": doc.size_bytes,
            "ingestStatus": doc.ingest_status,
            "extractionError": doc.extraction_error,
            "extracted": extracted,
            "source": doc.source,
            "createdAt": doc.created_at,
        });
        Ok(serde_json::to_string(&payload)?)
    }
}
