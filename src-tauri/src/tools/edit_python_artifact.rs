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
    /// v0.20.13 — planner integration. Same semantics as run_python:
    /// when (planId, planStepKey) are set, the tool checks the step
    /// cache before re-running. If the edited script + inputs
    /// produce the same hash as a cached run, the cached result is
    /// returned in milliseconds. On miss, runs and auto-records.
    #[serde(default)]
    plan_id: Option<i64>,
    #[serde(default)]
    plan_step_key: Option<String>,
    /// v0.20.14 — DAG-style pipe. Same shape as run_python's
    /// step_inputs: mount cached step results as JSON files.
    #[serde(default)]
    step_inputs: Vec<StepInputRef>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct StepInputRef {
    from_step_key: String,
    #[serde(default)]
    as_file: Option<String>,
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
                 and preserves the lineage.\n\n\
                 IMPORTANT — chat presentation: when this tool returns \
                 `generatedDocumentIds: [N1, N2, ...]`, you MUST include each id as a \
                 `doc#N` marker in your final reply. That's what triggers the UI to \
                 render the clickable file card. Example: 'Updated — doc#16'. Without \
                 the marker the user sees no card."
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
                    },
                    "planId": {
                        "type": "integer",
                        "description": "v0.20.13 — when working inside a plan, pass the planId so the edit is cache-aware. Same semantics as run_python's planId."
                    },
                    "planStepKey": {
                        "type": "string",
                        "description": "v0.20.13 — paired with planId. The step key the edited script implements (e.g. 'generate_invoice_pdf')."
                    },
                    "stepInputs": {
                        "type": "array",
                        "description": "v0.20.14 — DAG-style pipe. Same as run_python's stepInputs: mount cached results from prior plan steps as files under INPUTS_DIR.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "fromStepKey": { "type": "string" },
                                "asFile": { "type": "string" }
                            },
                            "required": ["fromStepKey"]
                        }
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
        let libraries_snapshot = p.libraries.clone();

        // v0.20.14 — resolve step_inputs (DAG-style pipe). Mirrors
        // the run_python flow.
        let mut edit_extra_input_files: std::collections::HashMap<String, Vec<u8>> =
            std::collections::HashMap::new();
        let mut step_input_summary: Vec<(String, String, Option<String>)> = Vec::new();
        if !p.step_inputs.is_empty() {
            if let Some(plan_id) = p.plan_id {
                for inp in &p.step_inputs {
                    let key = inp.from_step_key.trim();
                    if key.is_empty() {
                        continue;
                    }
                    let as_file = inp
                        .as_file
                        .clone()
                        .unwrap_or_else(|| format!("_step_{key}.json"));
                    match crate::plans::get_step(&pool_for_artifact, plan_id, key).await {
                        Ok(Some(step)) => {
                            let body = step.result_json.clone().unwrap_or_else(|| "null".into());
                            edit_extra_input_files.insert(as_file.clone(), body.into_bytes());
                            step_input_summary.push((
                                key.to_string(),
                                as_file,
                                step.result_hash.clone(),
                            ));
                        }
                        Ok(None) => {
                            return Err(anyhow::anyhow!(
                                "step_inputs: referenced step '{key}' not found in plan {plan_id}"
                            ));
                        }
                        Err(e) => return Err(anyhow::anyhow!("step_inputs: {e}")),
                    }
                }
            } else {
                return Err(anyhow::anyhow!(
                    "step_inputs requires planId"
                ));
            }
        }

        // v0.20.13 — planner cache check (same as run_python).
        let plan_cache = match (p.plan_id, p.plan_step_key.as_deref()) {
            (Some(plan_id), Some(key)) if !key.trim().is_empty() => {
                let mut hash = match crate::plans::input_hash(
                    &pool_for_artifact,
                    &script_for_artifact,
                    &input_doc_ids_snapshot,
                    &libraries_snapshot,
                )
                .await
                {
                    Ok(h) => h,
                    Err(e) => {
                        tracing::warn!("plan input_hash failed: {e}");
                        String::new()
                    }
                };
                if !hash.is_empty() && !step_input_summary.is_empty() {
                    hash = crate::plans::extend_hash_with_step_inputs(
                        &hash,
                        &step_input_summary,
                    );
                }
                if !hash.is_empty() {
                    match crate::plans::cache_hit_payload(
                        &pool_for_artifact,
                        plan_id,
                        key,
                        &hash,
                    )
                    .await
                    {
                        Ok(Some(cached)) => {
                            tracing::info!(
                                "edit_python_artifact: plan cache HIT — plan={plan_id} key={key}"
                            );
                            let payload = json!({
                                "ok": true,
                                "fromCache": true,
                                "planId": plan_id,
                                "planStepKey": key,
                                "purpose": p.purpose,
                                "stdout": "",
                                "stderr": "",
                                "executionMs": 0,
                                "generatedDocumentIds": cached.get("documentIds")
                                    .cloned()
                                    .unwrap_or(serde_json::json!([])),
                                "result": cached.get("result").cloned()
                                    .unwrap_or(serde_json::Value::Null),
                                "error": null,
                                "artifactId": null,
                                "supersedesArtifactId": p.supersedes_artifact_id,
                                "note": "Result returned from plan cache. Edit re-ran would have produced the same output."
                            });
                            return Ok(serde_json::to_string(&payload)?);
                        }
                        Ok(None) => {
                            tracing::info!(
                                "edit_python_artifact: plan cache miss — plan={plan_id} key={key}"
                            );
                        }
                        Err(e) => {
                            tracing::warn!("plan cache_hit_payload failed: {e}");
                        }
                    }
                    Some((plan_id, key.to_string(), hash))
                } else {
                    None
                }
            }
            _ => None,
        };

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
                extra_input_files: edit_extra_input_files,
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

        // v0.20.13 — auto-record into the plan step on success.
        if let Some((plan_id, key, hash)) = plan_cache {
            let status = if outcome.error.is_none() { "done" } else { "failed" };
            let result_json = serde_json::json!({
                "stdout": outcome.stdout,
                "stderr": outcome.stderr,
                "executionMs": outcome.execution_ms,
                "purpose": p.purpose,
                "generatedDocumentNames": outcome.generated_document_names,
                "supersedesArtifactId": p.supersedes_artifact_id,
            })
            .to_string();
            if let Err(e) = crate::plans::record_step_with_hash(
                &pool_for_artifact,
                plan_id,
                &key,
                status,
                &result_json,
                &outcome.generated_document_ids,
                &hash,
                outcome.error.as_deref(),
            )
            .await
            {
                tracing::warn!("plan record_step_with_hash failed: {e}");
            }
        }

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
