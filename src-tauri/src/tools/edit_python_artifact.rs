//! `edit_python_artifact` LLM tool — iterative refinement of a
//! generated artifact.
//!
//! Built on top of v0.15.3's python_artifact substrate. The LLM
//! already produced an artifact via `run_python` and got back an
//! `artifactId`. When the user asks for a small change ("remove
//! the school name from the To: block", "add 7 hours to row 1",
//! "the signature line a tiny bit down"), the LLM calls
//! `edit_python_artifact` with the prior `artifactId` plus the
//! edited script. The new artifact links back via `superseded_by`,
//! giving us diff-able lineage for future v0.16 case work.
//!
//! Design note: the LLM does the script editing itself — this tool
//! doesn't perform an internal LLM call. The LLM has the prior
//! script in its context (from the previous turn) plus the user's
//! change description; it produces the new script directly and
//! passes it here. This keeps the tool one synchronous run, no
//! extra LLM call cost, and makes the diff visible in the
//! conversation.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::Manager;

use crate::interpreter::cmd::{run_python as run_python_cmd, RunPythonParams};
use crate::llm::ToolDef;
use crate::tools::run_python::{persist_artifact, ArtifactRow};
use crate::tools::{Tool, ToolContext};
use crate::AppState;

pub struct EditPythonArtifactTool;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    /// The artifact id from a previous `run_python` (or
    /// `edit_python_artifact`) call to base this edit on. Travis
    /// uses this to link lineage so the lineage chain is
    /// diff-able.
    supersedes_artifact_id: i64,
    /// One-line description of the change being applied. Surfaced
    /// to the user as the step name. e.g. "Removed the
    /// '11 days delivered' annotation from the invoice line".
    purpose: String,
    /// The new full Python script to run (edited version of the
    /// prior artifact's script). The LLM produces this directly
    /// from its context — small focused change applied.
    code: String,
    /// Document ids to mount at /inputs/. Usually carries over
    /// from the prior artifact; the LLM passes them explicitly
    /// so the tool doesn't need to look the prior up itself.
    #[serde(default)]
    document_ids: Vec<i64>,
    /// Extra pure-Python libraries (rare for an edit).
    #[serde(default)]
    libraries: Vec<String>,
}

#[async_trait]
impl Tool for EditPythonArtifactTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "edit_python_artifact".into(),
            description:
                "Apply a SMALL edit to a Python artifact you generated earlier with \
                 `run_python`, and re-run it. Use this when the user asks for an \
                 incremental change to a document you just produced — \"remove the \
                 note\", \"add 7 hours to row 1\", \"signature line a tiny bit down\", \
                 \"change the school name to X\".\n\n\
                 Inputs: `supersedesArtifactId` (the `artifactId` from the prior \
                 run_python response), `purpose` (one-line description of the \
                 change), `code` (the new full Python source — copy the prior script \
                 from your context and apply the change), and the usual \
                 `documentIds` + `libraries`.\n\n\
                 The new artifact is linked to the prior via `superseded_by` so the \
                 lineage is diff-able. Output files appear in the chat as new \
                 FileCards. Prefer this over `run_python` from scratch whenever the \
                 user's request is a tweak to existing work — it's faster, cheaper, \
                 and preserves the lineage."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "supersedesArtifactId": {
                        "type": "integer",
                        "description": "Artifact id from the previous run_python or edit_python_artifact call."
                    },
                    "purpose": {
                        "type": "string",
                        "description": "One-line description of the change. e.g. 'Removed the note row from the invoice line items'."
                    },
                    "code": {
                        "type": "string",
                        "description": "The new full Python source after applying the edit. Don't send only a diff — send the whole script."
                    },
                    "documentIds": {
                        "type": "array",
                        "items": { "type": "integer" },
                        "description": "Document ids to mount at /inputs/. Usually the same set as the prior artifact."
                    },
                    "libraries": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Extra pure-Python libraries beyond the preinstalled set. Usually empty for an edit."
                    }
                },
                "required": ["supersedesArtifactId", "purpose", "code"]
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: Input = serde_json::from_value(input)?;
        let state = ctx.app.state::<AppState>();
        let conversation_id = ctx.conversation_id;

        // Clone the persistence handles before consuming state into
        // run_python_cmd.
        let pool_for_artifact = state.db.pool.clone();
        let workspace_id = state.workspace.read().await.active_id;

        // Sanity-check the supersedes link — fetch the prior row to
        // confirm it exists. Doesn't read the prior script (the LLM
        // already has it in its context); we just verify the id is
        // real so we don't dangle the lineage.
        let prior_exists: Option<(i64,)> =
            sqlx::query_as("SELECT id FROM python_artifact WHERE id = ?1")
                .bind(p.supersedes_artifact_id)
                .fetch_optional(&pool_for_artifact)
                .await
                .ok()
                .flatten();
        if prior_exists.is_none() {
            anyhow::bail!(
                "edit_python_artifact: supersedesArtifactId {} doesn't exist. \
                 Call run_python from scratch instead.",
                p.supersedes_artifact_id
            );
        }

        let purpose_for_artifact = p.purpose.clone();
        let script_for_artifact = p.code.clone();
        let input_doc_ids_snapshot = p.document_ids.clone();

        let outcome = run_python_cmd(
            ctx.app.clone(),
            state,
            RunPythonParams {
                code: p.code,
                purpose: p.purpose.clone(),
                document_ids: p.document_ids,
                libraries: p.libraries,
                conversation_id,
                workflow_state_id: None,
                timeout_secs: None,
            },
        )
        .await
        .map_err(|e| anyhow::anyhow!("edit_python_artifact run failed: {e}"))?;

        let artifact_id = persist_artifact(
            &pool_for_artifact,
            workspace_id,
            ArtifactRow {
                conversation_id,
                purpose: &purpose_for_artifact,
                script: &script_for_artifact,
                input_doc_ids: &input_doc_ids_snapshot,
                output_document_ids: &outcome.generated_document_ids,
                stdout: Some(outcome.stdout.as_str()),
                stderr: Some(outcome.stderr.as_str()),
                execution_ms: Some(outcome.execution_ms),
                error: outcome.error.as_deref(),
                superseded_by: Some(p.supersedes_artifact_id),
            },
        )
        .await;

        let payload = json!({
            "ok": outcome.error.is_none(),
            "purpose": p.purpose,
            "stdout": outcome.stdout,
            "stderr": outcome.stderr,
            "executionMs": outcome.execution_ms,
            "generatedDocumentIds": outcome.generated_document_ids,
            "generatedDocumentNames": outcome.generated_document_names,
            "error": outcome.error,
            "artifactId": artifact_id,
            "supersedesArtifactId": p.supersedes_artifact_id,
        });
        Ok(serde_json::to_string(&payload)?)
    }
}
