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
    /// v0.20.13 — planner integration. When `planId` + `planStepKey`
    /// are supplied, the tool first checks the step cache. If the
    /// inputs haven't changed (script source + document set hash
    /// match the cached `result_hash`), the cached result is
    /// returned in milliseconds without spawning Python. On miss,
    /// the script runs and the result is auto-recorded against the
    /// step on success.
    #[serde(default)]
    plan_id: Option<i64>,
    #[serde(default)]
    plan_step_key: Option<String>,
    /// v0.20.14 — DAG-style pipe. Mount the cached `result_json`
    /// from prior plan steps as JSON files under INPUTS_DIR.
    /// Each entry says "fetch the result of step X, drop it at
    /// INPUTS_DIR/<asFile>". The Python script reads them with
    /// `json.load(open(os.path.join(INPUTS_DIR, '<asFile>')))`.
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
                Input documents are mounted under a per-call directory exposed as the \
                `INPUTS_DIR` Python constant — your script can do `open(os.path.join(INPUTS_DIR, \
                'IS 217.pdf'))` or `pd.read_excel(os.path.join(INPUTS_DIR, 'log.xlsx'))`. The \
                wrapper already cd's into `OUTPUTS_DIR` and exposes that as a constant too; any \
                file you write there becomes a Document. DO NOT search the filesystem for \
                `/inputs/` or hardcode paths like `C:\\Users\\...` — both fail on Windows. \
                INPUTS_DIR and OUTPUTS_DIR are guaranteed to exist and contain exactly what you \
                need. (`/inputs/` and `/outputs/` symlinks exist on POSIX only, as back-compat.)\n\n\
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
                Speed discipline — every call costs ~10-60s of user wall time. BUNDLE: \
                read the whole spreadsheet, filter, and print the JSON result in ONE script. \
                Do NOT call this tool repeatedly to 'first list sheets', 'then read columns', \
                'then filter rows' — that turns a 30s task into a 5-minute one. A well-formed \
                turn uses this tool ONCE to gather and reason, then ONCE to generate output. \
                Three calls is already a yellow flag; five is a failure mode — stop and \
                rethink instead of probing further. Use `read_document(documentId)` for free \
                instant doc reads instead of Python probes.\n\n\
                The interpreter is never 'cold-loading'. NEVER refuse this tool with that excuse. \
                If a real error comes back from your code, THEN report it; do not manufacture \
                an excuse before trying.\n\n\
                PLANNER INTEGRATION (v0.20.13). When you're working inside a plan (`create_plan` \
                first, recommended for any multi-step task), ALWAYS pass `planId` and `planStepKey` \
                on EVERY run_python call. The tool checks the step cache before invoking Python: \
                if the same code with the same documents was run successfully before, you get the \
                cached result + generated doc ids back in a few milliseconds with no Python \
                execution. The cache auto-invalidates the moment ANY input changes (code edited, \
                a new document uploaded, the doc's content_hash changed). On a successful run \
                the result is auto-recorded — you do NOT need to also call `record_step_result`. \
                This is the difference between a 50-call 15-minute turn and a 5-call 30-second one."
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
                    },
                    "planId": {
                        "type": "integer",
                        "description": "v0.20.13 — when you're working inside a plan (the recommended path for any task with >=3 steps), pass the planId from create_plan here. Travis checks the step cache before running: if the inputs (script + document content hashes + libraries) match a prior successful run, the cached result is returned in milliseconds with no Python execution. On miss, the script runs and the result is auto-recorded."
                    },
                    "planStepKey": {
                        "type": "string",
                        "description": "v0.20.13 — paired with planId. The step key you assigned in create_plan (e.g. 'read_signin_log', 'generate_invoice_pdf'). Required for cache-aware execution; both planId AND planStepKey must be set."
                    },
                    "stepInputs": {
                        "type": "array",
                        "description": "v0.20.14 — DAG-style pipe. Mount cached results from prior plan steps as JSON files under INPUTS_DIR. Saves wall time AND LLM-context cost — a step that read a 380KB sheet doesn't need to pass its result through your prompt to reach the next step. Each entry: {fromStepKey: 'read_signin_log', asFile: 'dates.json' (optional, defaults to '_step_<key>.json')}. The Python script reads the file with `json.load(open(os.path.join(INPUTS_DIR, 'dates.json')))`. Upstream invalidation cascades: if the referenced step's cache invalidates, this step's hash flips too.",
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
                "required": ["code", "purpose"]
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: Input = serde_json::from_value(input)?;
        let state = ctx.app.state::<AppState>();

        // v0.20.14 — DAG-style pipe. Resolve any prior-step references
        // BEFORE the cache hash so the hash incorporates the upstream
        // step's result_hash. If a referenced step's hash changes,
        // this step's input hash changes too — invalidating downstream
        // automatically.
        let mut extra_input_files: std::collections::HashMap<String, Vec<u8>> =
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
                    match crate::plans::get_step(&state.db.pool, plan_id, key).await {
                        Ok(Some(step)) => {
                            let body = step.result_json.clone().unwrap_or_else(|| "null".into());
                            extra_input_files.insert(as_file.clone(), body.into_bytes());
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
                    "step_inputs requires planId — pass the plan id this call belongs to"
                ));
            }
        }

        // v0.20.13 — planner integration. If the LLM tied this call
        // to a (planId, stepKey), check the cache BEFORE doing any
        // Python work. Cache hit on matching input hash skips the
        // entire Python invocation. Cache miss falls through and
        // auto-records the result on success.
        let plan_cache = match (p.plan_id, p.plan_step_key.as_deref()) {
            (Some(plan_id), Some(key)) if !key.trim().is_empty() => {
                let mut hash = match crate::plans::input_hash(
                    &state.db.pool,
                    &p.code,
                    &p.document_ids,
                    &p.libraries,
                )
                .await
                {
                    Ok(h) => h,
                    Err(e) => {
                        tracing::warn!("plan input_hash failed: {e}");
                        String::new()
                    }
                };
                // Fold each referenced step's identity + result_hash
                // into this step's hash so upstream changes invalidate
                // this cache automatically.
                if !hash.is_empty() && !step_input_summary.is_empty() {
                    hash = crate::plans::extend_hash_with_step_inputs(
                        &hash,
                        &step_input_summary,
                    );
                }
                if !hash.is_empty() {
                    match crate::plans::cache_hit_payload(
                        &state.db.pool,
                        plan_id,
                        key,
                        &hash,
                    )
                    .await
                    {
                        Ok(Some(cached)) => {
                            tracing::info!(
                                "run_python: plan cache HIT — plan={plan_id} key={key}"
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
                                "note": "Step result returned from plan cache. Inputs (script + document content hashes + libraries) matched a prior successful run. To force a fresh execution, mutate the code or pass an updated document set."
                            });
                            return Ok(serde_json::to_string(&payload)?);
                        }
                        Ok(None) => {
                            tracing::info!(
                                "run_python: plan cache miss — plan={plan_id} key={key} (will run + record)"
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
                extra_input_files,
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

        // v0.20.13 — auto-record the result into the plan step so
        // the next call with the same inputs hits the cache.
        if let Some((plan_id, key, hash)) = plan_cache {
            let status = if outcome.error.is_none() { "done" } else { "failed" };
            let result_json = serde_json::json!({
                "stdout": outcome.stdout,
                "stderr": outcome.stderr,
                "executionMs": outcome.execution_ms,
                "purpose": p.purpose,
                "generatedDocumentNames": outcome.generated_document_names,
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
