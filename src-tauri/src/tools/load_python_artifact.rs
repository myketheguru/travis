//! `load_python_artifact` LLM tool — fetch a saved artifact's script
//! + outputs so the LLM can edit it across restarts.
//!
//! v0.15.3 introduced `python_artifact` persistence; v0.15.3's
//! `edit_python_artifact` requires the LLM to have the prior script
//! in its conversation context. After a restart, a context window
//! churn, or a long pause where condensation has dropped older
//! turns, the prior script isn't in context anymore — only the
//! `artifactId` is reachable (via a memory hit or the user's
//! mention).
//!
//! This tool closes the gap: pass the artifact id, get back the
//! full script + the doc ids of inputs/outputs + the purpose +
//! lineage info. The LLM can then call `edit_python_artifact` with
//! a focused change against the loaded script.
//!
//! Use case the LLM should reach for this:
//! - User says "edit the invoice you made last week" or "tweak the
//!   sign-in sheet from yesterday" — Travis searches memory, finds
//!   the artifact id, calls `load_python_artifact(id)` to get the
//!   script back into context, then `edit_python_artifact` with
//!   the small change.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::Manager;

use crate::llm::ToolDef;
use crate::tools::{Tool, ToolContext};
use crate::AppState;

pub struct LoadPythonArtifactTool;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    /// The artifact id to load. Usually surfaced by `search_memory`
    /// or `search_conversations` recall when the user references a
    /// prior generated doc.
    artifact_id: i64,
}

#[async_trait]
impl Tool for LoadPythonArtifactTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "load_python_artifact".into(),
            description:
                "Load the saved script + metadata for a Python artifact you (or a prior \
                 Travis session) generated earlier. Use this when the user asks for an \
                 edit to a document Travis produced previously and the prior script ISN'T \
                 already in your current conversation context — typical after a restart, \
                 a long pause, or when the user references work done in a prior thread \
                 (\"edit the invoice from last week\", \"tweak the sign-in sheet from \
                 yesterday\"). \n\n\
                 Returns: {script, purpose, inputDocIds, outputDocumentIds, stdout, stderr, \
                 error, supersededBy, createdAt}. Once you have the script, call \
                 `edit_python_artifact` with the focused change applied.\n\n\
                 If the user references the artifact fuzzily (\"the invoice from last \
                 week\", \"that sign-in sheet\"), call `search_memory` or \
                 `search_conversations` first to find the artifact id, then load it here."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "artifactId": { "type": "integer", "description": "The python_artifact row id." }
                },
                "required": ["artifactId"]
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: Input = serde_json::from_value(input)?;
        let state = ctx.app.state::<AppState>();
        let workspace_id = state.workspace.read().await.active_id;
        let row: Option<(
            i64,
            String,
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            Option<i64>,
            Option<String>,
            Option<i64>,
            String,
            Option<i64>,
        )> = sqlx::query_as(
            "SELECT id, purpose, script, input_doc_ids, output_document_ids,
                    stdout, stderr, execution_ms, error, superseded_by, created_at,
                    conversation_id
             FROM python_artifact
             WHERE id = ?1 AND workspace_id = ?2",
        )
        .bind(p.artifact_id)
        .bind(workspace_id)
        .fetch_optional(&state.db.pool)
        .await?;

        match row {
            Some((
                id,
                purpose,
                script,
                input_doc_ids,
                output_document_ids,
                stdout,
                stderr,
                execution_ms,
                error,
                superseded_by,
                created_at,
                conversation_id,
            )) => {
                let in_ids: Vec<i64> = serde_json::from_str(&input_doc_ids).unwrap_or_default();
                let out_ids: Vec<i64> =
                    serde_json::from_str(&output_document_ids).unwrap_or_default();
                Ok(json!({
                    "ok": true,
                    "artifactId": id,
                    "purpose": purpose,
                    "script": script,
                    "inputDocIds": in_ids,
                    "outputDocumentIds": out_ids,
                    "stdout": stdout,
                    "stderr": stderr,
                    "executionMs": execution_ms,
                    "error": error,
                    "supersededBy": superseded_by,
                    "conversationId": conversation_id,
                    "createdAt": created_at,
                })
                .to_string())
            }
            None => Ok(json!({
                "ok": false,
                "error": format!("artifact #{} not found in this workspace", p.artifact_id),
            })
            .to_string()),
        }
    }
}
