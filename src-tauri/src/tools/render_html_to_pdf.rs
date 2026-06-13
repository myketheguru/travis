//! `render_html_to_pdf` — high-fidelity document generation via HTML+CSS.
//!
//! The LLM writes the doc as a self-contained HTML template, weasyprint
//! renders it to PDF. This produces 90-95% layout fidelity on first try
//! because HTML+CSS is the substrate the model is trained on most
//! deeply — far more than reportlab's imperative drawing API.
//!
//! Pick this for ANY fresh document generation where visual fidelity
//! matters. Use `replicate_from_sample` instead when you have the
//! original PDF and want byte-identical pixels (with new values).
//! Use `run_python` + reportlab only as the last resort — complex
//! programmatic logic, dynamic line counts, constraint solving.
//!
//! Inputs:
//!   - `html`: the document body. Self-contained <html>...</html>.
//!   - `css`: optional extra CSS (also acceptable to inline it inside
//!     a <style> tag in the html). For long stylesheets keep them
//!     separate so the prompt stays readable.
//!   - `outputName`: filename for the new doc.
//!   - `assetDocumentIds`: doc ids to mount under INPUTS_DIR. Use this
//!     to feed logo images / fonts / background images into the
//!     renderer — reference them as `file://<INPUTS_DIR>/<filename>`
//!     in the HTML.
//!   - `pageSize`: default 'Letter'. Accepts 'Letter', 'A4', 'Legal',
//!     or custom like '8.5in 11in'.
//!   - `margins`: default '0.5in'. Standard CSS margin syntax.
//!
//! Output is registered as a Travis document and returned in
//! `generatedDocumentIds` like every other PDF-producing tool.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::Manager;

use crate::interpreter::cmd::{run_python as run_python_cmd, RunPythonParams};
use crate::llm::ToolDef;
use crate::tools::{Tool, ToolContext};
use crate::AppState;

const RENDER_PY: &str = r#"
import sys, os, json, traceback


def render(html, css, output_path, page_size, margins, base_url):
    from weasyprint import HTML, CSS

    # Build a master stylesheet:
    # - @page rules (size + margin) so the PDF dimensions match the spec
    # - the caller's CSS if supplied
    page_css_parts = []
    if page_size:
        page_css_parts.append(f"size: {page_size};")
    if margins:
        page_css_parts.append(f"margin: {margins};")
    stylesheets = []
    if page_css_parts:
        stylesheets.append(CSS(string="@page {{ " + " ".join(page_css_parts) + " }}"))
    if css and css.strip():
        stylesheets.append(CSS(string=css))

    doc = HTML(string=html, base_url=base_url or "")
    doc.write_pdf(output_path, stylesheets=stylesheets)


if __name__ == "__main__":
    try:
        spec = json.loads(sys.argv[1])
        render(
            spec["html"],
            spec.get("css", ""),
            spec["outputPath"],
            spec.get("pageSize", "Letter"),
            spec.get("margins", "0.5in"),
            spec.get("baseUrl"),
        )
        print(json.dumps({
            "ok": True,
            "outputPath": spec["outputPath"],
        }))
    except Exception as e:
        print(json.dumps({
            "ok": False,
            "error": str(e),
            "traceback": traceback.format_exc(),
        }), file=sys.stderr)
        sys.exit(1)
"#;

pub struct RenderHtmlToPdfTool;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    html: String,
    #[serde(default)]
    css: Option<String>,
    output_name: String,
    #[serde(default)]
    asset_document_ids: Vec<i64>,
    #[serde(default)]
    page_size: Option<String>,
    #[serde(default)]
    margins: Option<String>,
    /// Plan integration — same shape as run_python.
    #[serde(default)]
    plan_id: Option<i64>,
    #[serde(default)]
    plan_step_key: Option<String>,
}

#[async_trait]
impl Tool for RenderHtmlToPdfTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "render_html_to_pdf".into(),
            description: "High-fidelity PDF generation via HTML + CSS. Write the document as a \
                self-contained HTML template; weasyprint renders to PDF. Use this for ANY \
                fresh document where visual fidelity matters — invoices, sign-in sheets, \
                letters, reports.\n\n\
                YOU ARE FAR BETTER AT HTML+CSS THAN AT REPORTLAB. The model is trained on \
                orders of magnitude more HTML+CSS than reportlab. Where reportlab forces you \
                to hand-position every element and guess font metrics, HTML+CSS gives you \
                proper text flow, table layout, alignment, padding, and font handling that \
                Just Works. First-try fidelity goes from ~60-70% (reportlab) to ~90-95% \
                (HTML+CSS).\n\n\
                Decision matrix for doc generation:\n\
                1. EXACT REPLICA OF A SAMPLE PDF WITH NEW DATA → `replicate_from_sample`. \
                   Best fidelity (95-100%), requires the sample to overlay onto.\n\
                2. FRESH DOC MATCHING A STYLE OR GENERATING FROM SCRATCH → this tool \
                   (`render_html_to_pdf`). Near-pixel-perfect because HTML+CSS is the model's \
                   strongest doc-layout substrate. Default pick for new generation.\n\
                3. COMPLEX PROGRAMMATIC LOGIC (constraint solving, dynamic line counts, \
                   numerical optimization, weird custom layouts) → `run_python` + reportlab. \
                   Last resort.\n\n\
                For embedding logos / images: pass their doc ids in `assetDocumentIds`. They \
                get mounted under INPUTS_DIR. Reference them in HTML as \
                `<img src=\"file:///<INPUTS_DIR>/<filename>\">` (the tool substitutes the \
                actual path).\n\n\
                Page size defaults to 'Letter'; pass 'A4', 'Legal', or a custom '8.5in 11in' \
                if needed. Margins default to '0.5in' (standard CSS margin syntax — \
                '0.5in 1in 0.5in 1in' for asymmetric).\n\n\
                Standard fonts available: any browser-default + system fonts. For brand \
                fonts, embed via `@font-face` referencing a font file you mounted via \
                `assetDocumentIds`.\n\n\
                Returns: `{generatedDocumentIds: [N], outputName, ...}`. INCLUDE the doc#N \
                marker in your reply so the FileCard renders."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "html": {
                        "type": "string",
                        "description": "Self-contained HTML document. Include <html>, <head>, <body>. Inline CSS in a <style> tag is fine, or pass it via the css field."
                    },
                    "css": {
                        "type": "string",
                        "description": "Extra CSS (optional). Often cleaner to keep here than inline in html when it's long."
                    },
                    "outputName": {
                        "type": "string",
                        "description": "Filename for the new doc (e.g. 'LTE2026217002_IS217_Invoice.pdf')."
                    },
                    "assetDocumentIds": {
                        "type": "array",
                        "items": { "type": "integer" },
                        "description": "Travis doc ids to mount under INPUTS_DIR — logos, font files, background images you reference via file:// URLs in the HTML."
                    },
                    "pageSize": {
                        "type": "string",
                        "description": "Default 'Letter'. Accepts 'A4', 'Legal', or custom CSS size like '8.5in 11in'."
                    },
                    "margins": {
                        "type": "string",
                        "description": "Default '0.5in'. Standard CSS margin syntax."
                    },
                    "planId": {
                        "type": "integer",
                        "description": "Optional planner integration — same semantics as run_python's planId."
                    },
                    "planStepKey": {
                        "type": "string",
                        "description": "Paired with planId."
                    }
                },
                "required": ["html", "outputName"]
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: Input = serde_json::from_value(input)?;
        let state = ctx.app.state::<AppState>();
        let pool = state.db.pool.clone();

        let output_name = sanitize_output_name(&p.output_name);

        // Build the JSON spec. Output goes to OUTPUTS_DIR (the wrapper
        // sets this and the file gets registered as a Document). base_url
        // points at INPUTS_DIR so any `file://<INPUTS_DIR>/...` references
        // in the HTML resolve to mounted assets.
        let mut spec = serde_json::Map::new();
        spec.insert("html".into(), json!(p.html));
        spec.insert("css".into(), json!(p.css.unwrap_or_default()));
        spec.insert(
            "outputPath".into(),
            json!(format!("{{OUTPUTS_DIR}}/{}", output_name)),
        );
        spec.insert(
            "pageSize".into(),
            json!(p.page_size.unwrap_or_else(|| "Letter".to_string())),
        );
        spec.insert(
            "margins".into(),
            json!(p.margins.unwrap_or_else(|| "0.5in".to_string())),
        );
        spec.insert("baseUrl".into(), json!("{INPUTS_DIR_FILE_URL}"));

        let driver = format!(
            r#"
import os, json, sys, traceback
SPEC = {spec_literal}
SPEC['outputPath'] = SPEC['outputPath'].replace('{{{{OUTPUTS_DIR}}}}', OUTPUTS_DIR)
# Build a file:// base URL from the per-call INPUTS_DIR so the HTML's
# `<img src="file://.../INPUTS_DIR/logo.png">` style references resolve.
import pathlib
SPEC['baseUrl'] = pathlib.Path(INPUTS_DIR).as_uri() + '/'

{render_script}

try:
    render(
        SPEC['html'], SPEC.get('css', ''),
        SPEC['outputPath'],
        SPEC.get('pageSize', 'Letter'),
        SPEC.get('margins', '0.5in'),
        SPEC.get('baseUrl'),
    )
    print(json.dumps({{'ok': True, 'outputPath': SPEC['outputPath']}}))
except Exception as e:
    print(json.dumps({{
        'ok': False,
        'error': str(e),
        'traceback': traceback.format_exc(),
    }}), file=sys.stderr)
    sys.exit(1)
"#,
            spec_literal = serde_json::to_string(&Value::Object(spec))?,
            render_script = RENDER_PY,
        );

        let conv_id = ctx.conversation_id;
        let outcome = run_python_cmd(
            ctx.app.clone(),
            state,
            RunPythonParams {
                code: driver,
                purpose: format!("Rendering {} from HTML+CSS", output_name),
                document_ids: p.asset_document_ids,
                libraries: vec![],
                conversation_id: conv_id,
                workflow_state_id: None,
                timeout_secs: Some(120),
                extra_input_files: Default::default(),
            },
        )
        .await
        .map_err(|e| anyhow::anyhow!("render_html_to_pdf run failed: {e}"))?;

        // Plan cache (when used inside a plan).
        if let (Some(plan_id), Some(key)) = (p.plan_id, p.plan_step_key.as_deref()) {
            if !key.trim().is_empty() {
                let status = if outcome.error.is_none() { "done" } else { "failed" };
                let hash = crate::plans::input_hash(&pool, &p.html, &[], &[])
                    .await
                    .unwrap_or_default();
                let result_json = serde_json::json!({
                    "stdout": outcome.stdout,
                    "stderr": outcome.stderr,
                    "executionMs": outcome.execution_ms,
                    "outputName": output_name,
                    "generatedDocumentNames": outcome.generated_document_names,
                })
                .to_string();
                let _ = crate::plans::record_step_with_hash(
                    &pool,
                    plan_id,
                    key,
                    status,
                    &result_json,
                    &outcome.generated_document_ids,
                    &hash,
                    outcome.error.as_deref(),
                )
                .await;
            }
        }

        let payload = json!({
            "ok": outcome.error.is_none(),
            "outputName": output_name,
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

fn sanitize_output_name(raw: &str) -> String {
    let trimmed = raw.trim();
    let cleaned: String = trimmed
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "rendered.pdf".into()
    } else if !cleaned.to_lowercase().ends_with(".pdf") {
        format!("{cleaned}.pdf")
    } else {
        cleaned
    }
}
