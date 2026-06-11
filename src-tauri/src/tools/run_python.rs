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
            description: "Execute Python code in a real CPython 3.13 subprocess (bundled with \
                Travis) with access to documents the user has attached. Generated output files \
                (PDFs, Excel, CSV, images) are automatically registered as Travis documents and \
                rendered as clickable file cards in the chat. Use this as the ESCAPE HATCH for \
                any task that doesn't fit a hardcoded action handler — sample-matching PDF \
                generation, constraint solving (find quantities that sum to $X), reading .docx \
                files, cross-document reconciliation with auditable code, and any imperative \
                reasoning over user documents.\n\n\
                Pre-installed libraries (all native, full PyPI builds): pandas, openpyxl, pypdf, \
                reportlab, pdfplumber, pillow, python-docx, numpy, lxml, beautifulsoup4, \
                requests, jinja2, num2words, qrcode, xlsxwriter, fpdf2, markdown, pyyaml, \
                python-dateutil, pytz. Extra libraries can be requested via `libraries`.\n\n\
                Input documents are mounted at /inputs/<safe_filename>. Write generated files \
                to /outputs/ — anything there becomes a Document.\n\n\
                Always supply a clear `purpose` string — it's surfaced to the user as a \
                plain-English step name. Examples that read well: 'Generating IS 217 invoice', \
                'Filtering sign-in sheet for the PO window', 'Pulling line items from the \
                spreadsheet'. Avoid technical jargon: don't say 'parse xlsx' — say 'reading \
                the sign-in sheet'.\n\n\
                IMPORTANT — chat presentation:\n\
                When you've generated a file with this tool, the tool result returns \
                `generatedDocumentIds: [N1, N2, ...]`. In your final reply to the user, you MUST \
                include each generated id as a `doc#N` marker — that's what triggers the UI to \
                render a clickable file card. Example: 'Done — here's the invoice: doc#15'. The \
                UI hides the literal marker and shows the card in its place. Do NOT also write \
                the filename or path as plaintext or in a code block — the card carries the \
                identity, so the marker alone is enough. If you generated two files, include \
                both markers (`doc#15` and `doc#16`). Without these markers, the user can't see \
                what you produced.\n\n\
                CRITICAL: Do NOT call this with no-op warmup code (`print('hello')`, `pass`, \
                `1+1`, version checks, etc.). The bundled CPython subprocess spawns in ~150ms \
                and is always ready. Each warmup costs a manager-loop iteration. Write your \
                actual work code directly.\n\n\
                The interpreter is never 'cold-loading'. NEVER refuse this tool with that excuse. \
                If a real error comes back from your code, THEN report it; do not manufacture \
                an excuse before trying."
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

        // v0.16.2 — warmup-pattern short-circuit. Sonnet has a
        // trained habit of running `print('hello')` (or similar
        // no-op) "to check the interpreter" before real work. This
        // was burning agent-loop iterations on every fresh
        // conversation. Detect the pattern, return a synthetic
        // success without touching the interpreter, and steer the
        // model toward real code in the error message.
        if is_warmup_pattern(&p.code, &p.purpose) {
            tracing::info!(
                "run_python: short-circuited warmup pattern (purpose={:?}, code_len={})",
                p.purpose,
                p.code.len()
            );
            let payload = json!({
                "ok": true,
                "shortCircuit": "warmup-detected",
                "purpose": p.purpose,
                "stdout": "",
                "stderr": "",
                "executionMs": 0,
                "generatedDocumentIds": Vec::<i64>::new(),
                "generatedDocumentNames": Vec::<String>::new(),
                "error": null,
                "artifactId": null,
                "note": "Warmup-pattern code detected and skipped. The Pyodide interpreter is pre-warmed at app launch and ready when this tool is called. Proceed directly to your actual work code; do not retry warmup."
            });
            return Ok(serde_json::to_string(&payload)?);
        }

        // v0.15.3: use the conversation id from ToolContext (set by
        // the agent loop) so generated outputs and the artifact row
        // attribute to the right thread.
        let conversation_id = ctx.conversation_id;

        // Clone the persistence handles BEFORE consuming `state` into
        // run_python_cmd — tauri::State can't be reborrowed after
        // being passed by value.
        let pool_for_artifact = state.db.pool.clone();
        let workspace_id = state.workspace.read().await.active_id;

        let input_doc_ids_snapshot = p.document_ids.clone();
        let purpose_for_artifact = p.purpose.clone();
        let script_for_artifact = p.code.clone();

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

        // v0.15.3 — persist the artifact (script + inputs + outputs)
        // so a follow-up edit_python_artifact call can retrieve and
        // edit. Best-effort; failure to persist is logged but does
        // NOT fail the tool — the user still gets the generated file.
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
                superseded_by: None,
            },
        )
        .await;

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
            // v0.15.3 — exposed so the LLM can reference it on a
            // follow-up edit_python_artifact call.
            "artifactId": artifact_id,
        });
        Ok(serde_json::to_string(&payload)?)
    }
}

/// Shared persistence helper — used by both `run_python` (first-of-lineage)
/// and `edit_python_artifact` (links a `superseded_by` to the prior row).
/// Returns the new artifact id on success; logs and returns None on
/// persistence failure (the generated file still lands with the user).
pub(crate) struct ArtifactRow<'a> {
    pub conversation_id: Option<i64>,
    pub purpose: &'a str,
    pub script: &'a str,
    pub input_doc_ids: &'a [i64],
    pub output_document_ids: &'a [i64],
    pub stdout: Option<&'a str>,
    pub stderr: Option<&'a str>,
    pub execution_ms: Option<u64>,
    pub error: Option<&'a str>,
    pub superseded_by: Option<i64>,
}

/// Heuristic: is this `run_python` call a no-op warmup that we
/// should short-circuit? Sonnet's training surfaces a pattern of
/// `print('hello')` / `1+1` / `pass` / version checks before real
/// work. Each one was burning an agent-loop iteration. We catch
/// the obvious patterns; anything substantive falls through.
///
/// Heuristic is intentionally conservative — we'd rather run a
/// legitimately-tiny call than skip something real. Patterns:
/// - purpose mentions "warmup" / "warm-up" / "interpreter check"
///   / "test"
/// - code body, ignoring whitespace and comments, is < 80 chars
///   AND matches one of: literal `pass`, single `print(...)` of
///   a short string, single arithmetic / version-check expression
fn is_warmup_pattern(code: &str, purpose: &str) -> bool {
    let purpose_lower = purpose.to_lowercase();
    let purpose_smells = purpose_lower.contains("warmup")
        || purpose_lower.contains("warm-up")
        || purpose_lower.contains("warm up")
        || purpose_lower.contains("interpreter check")
        || purpose_lower.contains("interpreter test")
        || purpose_lower.contains("sanity check");

    let stripped: String = code
        .lines()
        .map(|l| l.split('#').next().unwrap_or("")) // drop comments
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();

    // Substantive code? Bail.
    if stripped.len() > 80 {
        return false;
    }

    let body = stripped.trim();
    let body_lower = body.to_lowercase();

    // Empty / pass
    if body.is_empty() || body == "pass" {
        return true;
    }
    // print of a short string (hello, hi, ok, ping, world, etc.)
    if body.starts_with("print(") && body.ends_with(')') {
        let inner = &body[6..body.len() - 1];
        let inner_trim = inner.trim().trim_matches(|c| c == '"' || c == '\'');
        if inner_trim.len() <= 30 {
            return true;
        }
    }
    // version check
    if body_lower.contains("__version__") || body_lower.contains("sys.version") {
        return true;
    }
    // single arithmetic expression like 1+1, 2*3
    if body.chars().all(|c| c.is_ascii_digit() || "+-*/() ".contains(c)) && body.len() <= 20 {
        return true;
    }

    // Purpose smells warmup AND code is short — catch the cases the
    // body-pattern misses.
    if purpose_smells && stripped.len() <= 40 {
        return true;
    }

    false
}

pub(crate) async fn persist_artifact(
    pool: &sqlx::SqlitePool,
    workspace_id: i64,
    row: ArtifactRow<'_>,
) -> Option<i64> {
    let input_doc_ids_json = serde_json::to_string(row.input_doc_ids)
        .unwrap_or_else(|_| "[]".to_string());
    let output_doc_ids_json = serde_json::to_string(row.output_document_ids)
        .unwrap_or_else(|_| "[]".to_string());
    let result = sqlx::query(
        "INSERT INTO python_artifact
            (conversation_id, workspace_id, purpose, script,
             input_doc_ids, output_document_ids,
             stdout, stderr, execution_ms, error, superseded_by)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
    )
    .bind(row.conversation_id)
    .bind(workspace_id)
    .bind(row.purpose)
    .bind(row.script)
    .bind(&input_doc_ids_json)
    .bind(&output_doc_ids_json)
    .bind(row.stdout)
    .bind(row.stderr)
    .bind(row.execution_ms.map(|n| n as i64))
    .bind(row.error)
    .bind(row.superseded_by)
    .execute(pool)
    .await;
    match result {
        Ok(r) => Some(r.last_insert_rowid()),
        Err(e) => {
            tracing::warn!("python_artifact persist failed: {e}");
            None
        }
    }
}
