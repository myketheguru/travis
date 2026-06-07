//! `run_python` LLM tool — the escape hatch from hardcoded handlers.
//!
//! Forwards LLM-emitted Python code into the interpreter window and
//! returns the result (stdout, stderr, generated document references).
//! This is the v0.14.0 capability that lets Travis match arbitrary
//! sample layouts, do constraint solving, and read formats it doesn't
//! natively ingest — anything Pyodide can do.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::Manager;

use crate::interpreter::cmd::{run_python as run_python_cmd, RunPythonParams};
use crate::llm::ToolDef;
use crate::tools::{Tool, ToolContext};
use crate::AppState;

pub struct RunPythonTool;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    code: String,
    purpose: String,
    #[serde(default)]
    document_ids: Vec<i64>,
    #[serde(default)]
    libraries: Vec<String>,
}

#[async_trait]
impl Tool for RunPythonTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "run_python".into(),
            description: "Execute Python code in a sandboxed Pyodide interpreter (CPython compiled \
                to WASM) with access to documents the user has attached. Generated output files \
                (PDFs, Excel, CSV, images) are automatically registered as Travis documents and \
                returned. Use this as the ESCAPE HATCH for any task that doesn't fit a hardcoded \
                action handler — sample-matching PDF generation, constraint solving (find \
                quantities that sum to $X), reading .docx files, cross-document reconciliation \
                with auditable code, and any imperative reasoning over user documents.\n\n\
                Pre-installed libraries: pandas, openpyxl, pypdf, reportlab, pillow, python-docx, \
                numpy. Extra libraries (pure Python only) can be requested via the `libraries` \
                parameter and will be installed via micropip.\n\n\
                Input documents are mounted at /inputs/<safe_filename>. Write generated files to \
                /outputs/ — anything there becomes a Document. Working directory and /tmp are \
                scratch space.\n\n\
                Always supply a clear `purpose` string — it's surfaced to the user as the step name."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "code": {
                        "type": "string",
                        "description": "Python source to execute. Use standard libraries plus the preinstalled set."
                    },
                    "purpose": {
                        "type": "string",
                        "description": "One-line description of what this code is doing — surfaced as a named step. e.g. 'Building IS 217 invoice matching the supplied sample template'."
                    },
                    "documentIds": {
                        "type": "array",
                        "items": { "type": "integer" },
                        "description": "Travis document ids to mount at /inputs/. Call find_documents or read_document first to get ids."
                    },
                    "libraries": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Extra pure-Python libraries to install via micropip beyond the preinstalled set."
                    }
                },
                "required": ["code", "purpose"]
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: Input = serde_json::from_value(input)?;
        let state = ctx.app.state::<AppState>();

        // Get the active conversation if any — for attributing outputs
        // back to the right thread. Workflow tools without a clear
        // conversation just pass None.
        let conversation_id: Option<i64> = None;

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
        .map_err(|e| anyhow::anyhow!("run_python failed: {e}"))?;

        // Return a structured JSON summary to the LLM
        let payload = json!({
            "ok": outcome.error.is_none(),
            "purpose": p.purpose,
            "stdout": outcome.stdout,
            "stderr": outcome.stderr,
            "executionMs": outcome.execution_ms,
            "generatedDocumentIds": outcome.generated_document_ids,
            "generatedDocumentNames": outcome.generated_document_names,
            "error": outcome.error,
        });
        Ok(serde_json::to_string(&payload)?)
    }
}
