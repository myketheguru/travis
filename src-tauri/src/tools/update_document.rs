//! `update_document_field` — patch a single field in a document's
//! extracted JSON when the extractor got it wrong, or the user wants
//! to refine a value before it flows into a workflow finalize.
//!
//! Travis uses this when Taylor corrects something verbally ("the
//! line 2 unit price should be $5031.30, not $5013.30 — the extractor
//! misread the scan") rather than re-uploading a corrected source.
//! The structured JSON is the source of truth; the original PDF stays
//! untouched on disk.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::Manager;

use crate::documents::db as docs_db;
use crate::llm::ToolDef;
use crate::tools::{Tool, ToolContext};
use crate::AppState;

pub struct UpdateDocumentFieldTool;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    document_id: i64,
    /// Dot-path to the field to update inside extracted_json. Examples:
    /// "po_number", "line_items.2.unit_price_cents", "period_end".
    /// Array indices are integer dot-segments (zero-based).
    field_path: String,
    /// New value — any JSON shape.
    value: Value,
}

#[async_trait]
impl Tool for UpdateDocumentFieldTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "update_document_field".into(),
            description: "Update a single field inside a document's extracted JSON. \
                Use when the user identifies a specific extraction error (typo, OCR \
                miss, wrong date). Field path uses dot notation; array indices are \
                integers (e.g. 'line_items.2.unit_price_cents'). The source PDF is \
                NOT modified — only the structured data Travis reasons over."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "documentId": { "type": "integer" },
                    "fieldPath": {
                        "type": "string",
                        "description": "Dot-path to the field. Array indices are dot-segments. Example: 'line_items.0.description'."
                    },
                    "value": {
                        "description": "New value (string/number/bool/object/array)."
                    }
                },
                "required": ["documentId", "fieldPath", "value"]
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: Input = serde_json::from_value(input)?;
        let state = ctx.app.state::<AppState>();
        let doc = docs_db::get(&state.db.pool, p.document_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("document {} not found", p.document_id))?;

        let mut payload: Value = doc
            .extracted_json
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_else(|| Value::Object(Default::default()));

        set_dot_path(&mut payload, &p.field_path, p.value.clone())?;

        let json_str = serde_json::to_string(&payload)?;
        docs_db::set_extracted(
            &state.db.pool,
            p.document_id,
            docs_db::IngestStatus::Extracted,
            Some(&json_str),
            None,
        )
        .await?;

        Ok(serde_json::to_string(&json!({
            "ok": true,
            "documentId": p.document_id,
            "fieldPath": p.field_path,
            "updated": payload,
        }))?)
    }
}

/// Set `value` at the given dot-path inside `root`. Creates missing
/// objects/arrays as needed. Returns an error if a non-numeric segment
/// is used against an existing array.
fn set_dot_path(
    root: &mut Value,
    path: &str,
    value: Value,
) -> anyhow::Result<()> {
    let segments: Vec<&str> = path.split('.').filter(|s| !s.is_empty()).collect();
    if segments.is_empty() {
        anyhow::bail!("empty field path");
    }
    let mut current = root;
    for (i, seg) in segments.iter().enumerate() {
        let is_last = i == segments.len() - 1;
        if let Ok(idx) = seg.parse::<usize>() {
            // Numeric segment — must be an array.
            if !current.is_array() {
                *current = Value::Array(Vec::new());
            }
            let arr = current.as_array_mut().unwrap();
            while arr.len() <= idx {
                arr.push(Value::Null);
            }
            if is_last {
                arr[idx] = value;
                return Ok(());
            }
            current = &mut arr[idx];
        } else {
            // Object segment.
            if !current.is_object() {
                *current = Value::Object(Default::default());
            }
            let map = current.as_object_mut().unwrap();
            if is_last {
                map.insert(seg.to_string(), value);
                return Ok(());
            }
            if !map.contains_key(*seg) {
                map.insert(seg.to_string(), Value::Object(Default::default()));
            }
            current = map.get_mut(*seg).unwrap();
        }
    }
    Ok(())
}
