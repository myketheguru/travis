//! Tauri command surface for the code interpreter.
//!
//! `run_python` is the user-facing entry point: takes code + a list of
//! document IDs to mount + optional library list, returns the result
//! including any generated files (which get registered as Documents).

use std::collections::HashMap;
use std::path::Path;

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::documents::{cmd as docs_cmd, db as docs_db, storage};
use crate::python_runtime;
use crate::AppState;

const DEFAULT_TIMEOUT_SECS: u64 = 60;
const MAX_TIMEOUT_SECS: u64 = 300;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunPythonParams {
    /// Python source to execute.
    pub code: String,
    /// One-line description of what this code is doing — surfaced in
    /// the chat UI as a step name ("Generating IS 217 invoice").
    pub purpose: String,
    /// Document ids to mount into /inputs/ inside the Python VFS.
    #[serde(default)]
    pub document_ids: Vec<i64>,
    /// Extra micropip-installable libraries beyond the preinstalled
    /// set (reportlab, openpyxl, pypdf, pandas, pillow, python-docx).
    #[serde(default)]
    pub libraries: Vec<String>,
    /// Conversation this execution belongs to — used to attribute
    /// generated documents.
    #[serde(default)]
    pub conversation_id: Option<i64>,
    /// Optional workflow_state_id for tagging generated outputs.
    #[serde(default)]
    pub workflow_state_id: Option<i64>,
    /// Execution timeout. Default 60s, capped at 300s.
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    /// v0.20.14 — DAG-style pipe. Extra in-memory files to mount
    /// alongside document_ids. Map of safe filename → raw bytes.
    /// The plan tools use this to inject a prior step's
    /// `result_json` as a file like `_step_read_log.json` so the
    /// Python script can `json.load` it directly without the data
    /// passing through the LLM's context window.
    #[serde(default, skip_serializing)]
    pub extra_input_files: HashMap<String, Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunPythonResult {
    #[serde(rename = "requestId")]
    pub request_id: String,
    pub stdout: String,
    pub stderr: String,
    /// Map of filename → base64 file bytes (raw from interpreter).
    /// The cmd post-processes these into `generated_document_ids`.
    #[serde(default)]
    pub generated_files: HashMap<String, String>,
    pub execution_ms: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunPythonOutcome {
    pub stdout: String,
    pub stderr: String,
    /// IDs of any documents created from generated files.
    pub generated_document_ids: Vec<i64>,
    /// Display names of generated docs in order.
    pub generated_document_names: Vec<String>,
    pub execution_ms: u64,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn run_python(
    app: AppHandle,
    state: State<'_, AppState>,
    params: RunPythonParams,
) -> Result<RunPythonOutcome, String> {
    // v0.18.0 — subprocess-based CPython via python_runtime. Replaces
    // the Pyodide hidden-window architecture entirely; the wait_ready
    // / emit / await listener pattern is gone because real-process
    // spawn is synchronous-ish (~150ms cold) and atomic.
    //
    // `params.libraries` is now informational — every wheel we
    // declared in scripts/fetch-python.mjs is preinstalled. Anything
    // else the LLM names here is ignored (and printed in a warning
    // step so we can spot drift between the LLM's expectations and
    // the bundle).
    let timeout_secs = params
        .timeout_secs
        .unwrap_or(DEFAULT_TIMEOUT_SECS)
        .min(MAX_TIMEOUT_SECS);

    // Load + base64-encode requested documents
    let mut input_files: HashMap<String, String> = HashMap::new();
    // v0.20.14 — mount any extra in-memory files (step result JSON
    // injected by the plan tools, for example) first so user-document
    // names always win on collision.
    for (name, bytes) in &params.extra_input_files {
        let safe = sanitize_filename(name, 0);
        input_files.insert(safe, B64.encode(bytes));
    }
    if !params.document_ids.is_empty() {
        let data_dir = app
            .path()
            .app_data_dir()
            .map_err(|e| format!("resolve app data dir: {e}"))?;
        let storage_root = storage::storage_root(&data_dir)
            .map_err(|e| format!("storage root: {e}"))?;
        for doc_id in &params.document_ids {
            let doc = docs_db::get(&state.db.pool, *doc_id)
                .await
                .map_err(|e| format!("load doc {doc_id}: {e}"))?
                .ok_or_else(|| format!("document {doc_id} not found"))?;
            let abs = storage::absolute_path(&storage_root, Path::new(&doc.relative_path));
            let bytes = std::fs::read(&abs)
                .map_err(|e| format!("read doc {doc_id} bytes: {e}"))?;
            let b64 = B64.encode(&bytes);
            // Use a Python-friendly filename — original kept where possible
            let safe_name = sanitize_filename(&doc.original_filename, *doc_id);
            input_files.insert(safe_name, b64);
        }
    }

    let request_id = format!("rp_{}_{}", chrono::Utc::now().timestamp_millis(), rand_suffix());
    let result = python_runtime::run(
        &app,
        python_runtime::RunParams {
            request_id: request_id.clone(),
            code: params.code.clone(),
            input_files,
            timeout_secs,
        },
    )
    .await;

    // Register any generated files as Documents
    let mut generated_ids: Vec<i64> = Vec::new();
    let mut generated_names: Vec<String> = Vec::new();
    if !result.generated_files.is_empty() {
        let data_dir = app
            .path()
            .app_data_dir()
            .map_err(|e| format!("resolve app data dir: {e}"))?;
        let scratch = data_dir.join("interpreter-out");
        tokio::fs::create_dir_all(&scratch)
            .await
            .map_err(|e| format!("create scratch dir: {e}"))?;

        for (name, b64) in &result.generated_files {
            let bytes = match B64.decode(b64) {
                Ok(b) => b,
                Err(e) => {
                    tracing::warn!("could not decode output {name}: {e}");
                    continue;
                }
            };
            let scratch_path = scratch.join(sanitize_filename(name, 0));
            if let Err(e) = tokio::fs::write(&scratch_path, &bytes).await {
                tracing::warn!("could not write scratch output {name}: {e}");
                continue;
            }
            match docs_cmd::register_generated_document(
                &app,
                state.inner(),
                &scratch_path,
                kind_from_extension(&scratch_path),
                Some(name),
                None,
                params.conversation_id,
            )
            .await
            {
                Ok(doc) => {
                    generated_ids.push(doc.id);
                    generated_names.push(doc.display_name);
                }
                Err(e) => {
                    tracing::warn!("could not register output {name}: {e}");
                }
            }
            // Clean up scratch — bytes are now in managed storage
            let _ = tokio::fs::remove_file(&scratch_path).await;
        }
    }

    Ok(RunPythonOutcome {
        stdout: result.stdout,
        stderr: result.stderr,
        generated_document_ids: generated_ids,
        generated_document_names: generated_names,
        execution_ms: result.execution_ms,
        error: result.error,
    })
}

fn sanitize_filename(name: &str, fallback_id: i64) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        format!("file_{fallback_id}")
    } else {
        cleaned
    }
}

fn kind_from_extension(path: &Path) -> &str {
    match path.extension().and_then(|e| e.to_str()) {
        Some(e) => match e.to_ascii_lowercase().as_str() {
            "pdf" => "generated_pdf",
            "xlsx" | "xls" => "generated_spreadsheet",
            "csv" => "generated_csv",
            "docx" | "doc" => "generated_doc",
            "png" | "jpg" | "jpeg" => "generated_image",
            _ => "generated_file",
        },
        None => "generated_file",
    }
}

fn rand_suffix() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let c = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{c:x}")
}
