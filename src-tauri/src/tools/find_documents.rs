//! `find_documents` — list ingested documents matching simple filters
//! so the LLM can find the right PO / WO / sign-in sheet without
//! needing the user to remember an id.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::Manager;

use crate::documents::db::{self as docs_db, ListFilter};
use crate::llm::ToolDef;
use crate::tools::{Tool, ToolContext};
use crate::AppState;

pub struct FindDocumentsTool;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    /// Optional document kind filter — "po", "wo", "signed_sheet",
    /// "invoice", "contract", or any value the user has assigned.
    #[serde(default)]
    kind: Option<String>,
    /// Optional entity id — return only docs linked to this entity
    /// (e.g. the engagement Travis is currently scoped to).
    #[serde(default)]
    entity_id: Option<i64>,
    /// Optional conversation scope.
    #[serde(default)]
    conversation_id: Option<i64>,
    #[serde(default)]
    limit: Option<i64>,
}

#[async_trait]
impl Tool for FindDocumentsTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "find_documents".into(),
            description: "List ingested documents in the active workspace, filtered by \
                kind, linked entity, or conversation. Returns a compact list \
                [{id, kind, displayName, ingestStatus, createdAt}] sorted most-recent \
                first. Use to find the PO / signed sheet / WO that should fill a \
                workflow slot before asking the user to drop one."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "kind": { "type": "string", "description": "Filter by document kind ('po', 'wo', 'signed_sheet', etc.)." },
                    "entityId": { "type": "integer", "description": "Filter to docs linked to this entity id." },
                    "conversationId": { "type": "integer", "description": "Filter to docs attached to this conversation." },
                    "limit": { "type": "integer", "description": "Max results (default 25, capped at 100)." }
                }
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: Input = serde_json::from_value(input).unwrap_or(Input {
            kind: None,
            entity_id: None,
            conversation_id: None,
            limit: None,
        });
        let state = ctx.app.state::<AppState>();
        let workspace_id = state.workspace.read().await.active_id;
        let limit = p.limit.unwrap_or(25).clamp(1, 100);

        let docs = docs_db::list(
            &state.db.pool,
            ListFilter {
                workspace_id: Some(workspace_id),
                kind: p.kind,
                entity_id: p.entity_id,
                conversation_id: p.conversation_id,
                workflow_state_id: None,
                limit: Some(limit),
            },
        )
        .await;

        let rows: Vec<Value> = docs
            .into_iter()
            .map(|d| {
                json!({
                    "id": d.id,
                    "kind": d.kind,
                    "displayName": d.display_name,
                    "ingestStatus": d.ingest_status,
                    "sizeBytes": d.size_bytes,
                    "createdAt": d.created_at,
                })
            })
            .collect();

        Ok(serde_json::to_string(&json!({
            "documents": rows,
            "count": rows.len(),
        }))?)
    }
}
