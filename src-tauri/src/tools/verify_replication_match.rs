//! `verify_replication_match` — vision-based comparison between a
//! generated doc and its sample. Closes the loop on the replication
//! flow: Travis writes the HTML, weasyprint renders, this tool asks
//! Claude vision "do these match? what's different?" The LLM uses the
//! returned report to decide whether to iterate.
//!
//! Implementation: render both PDFs to images (200 DPI, page 1) via
//! the bundled CPython + pypdfium2, then make a multimodal chat call
//! to the user's configured provider with both images attached.
//! Returns structured mismatch text.

use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::Path;
use tauri::Manager;

use crate::documents::storage;
use crate::llm::{self, ChatOptions, Message, MessageImage, ToolDef};
use crate::secrets;
use crate::tools::{Tool, ToolContext};
use crate::AppState;

const RENDER_PAGE_PY: &str = r#"
import sys, os
import pypdfium2 as pdfium

def main(pdf_path, out_path, dpi=200, page_idx=0):
    pdf = pdfium.PdfDocument(pdf_path)
    page_count = len(pdf)
    if page_count == 0:
        raise RuntimeError("PDF has no pages")
    page_idx = max(0, min(page_idx, page_count - 1))
    page = pdf[page_idx]
    bitmap = page.render(scale=dpi/72)
    img = bitmap.to_pil()
    img.save(out_path, format='PNG')
    pdf.close()

if __name__ == "__main__":
    main(
        sys.argv[1],
        sys.argv[2],
        int(sys.argv[3]) if len(sys.argv) > 3 else 200,
        int(sys.argv[4]) if len(sys.argv) > 4 else 0,
    )
"#;

const COMPARISON_PROMPT: &str = "\
You are looking at two rendered PDFs. The FIRST image is the original SAMPLE the user wants \
to replicate. The SECOND image is Travis's GENERATED attempt. \n\n\
Compare them carefully and return a structured report of mismatches. Cover:\n\
- Layout structure (logo placement, section ordering, decorative bands/rules)\n\
- Typography (font choices, weight, size, color matches)\n\
- Specific elements: invoice number placement, address fields style, table structure, \
empty rows, signature block, footer notes\n\
- Decorative details: color bands, watermarks, double underlines, label phrasing \
(e.g. 'To:' vs 'Bill To:')\n\n\
For EACH mismatch, give:\n\
- WHAT is different (specific element)\n\
- WHERE it's different (e.g. 'logo: top-right in attempt, top-left in sample')\n\
- HOW to fix it in HTML+CSS (concrete CSS change)\n\n\
End with an overall match assessment: 'CLOSE_ENOUGH' (90%+ match, ship as-is) or \
'NEEDS_REFINEMENT' (list the top 3 fixes to apply next).";

pub struct VerifyReplicationMatchTool;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    generated_document_id: i64,
    sample_document_id: i64,
    /// Render DPI. Default 200 — good vision quality without huge tokens.
    #[serde(default)]
    dpi: Option<u32>,
}

#[async_trait]
impl Tool for VerifyReplicationMatchTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "verify_replication_match".into(),
            description: "Vision-based comparison between a generated doc and its sample. Use \
                AFTER `render_html_to_pdf` (or `replicate_from_sample`) to check whether the \
                output actually matches the sample. The tool renders both PDFs to images, \
                shows them to Claude vision side-by-side, and returns a structured mismatch \
                report.\n\n\
                Returned report includes: layout mismatches, typography differences, specific \
                element placement issues, decorative-detail gaps, and concrete CSS fixes for \
                each. Ends with `CLOSE_ENOUGH` (ship) or `NEEDS_REFINEMENT` (top 3 fixes).\n\n\
                Pattern: generate → verify → if NEEDS_REFINEMENT, edit your HTML applying the \
                suggested fixes → re-render → verify again. 2-3 iterations should converge to \
                CLOSE_ENOUGH on most documents."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "generatedDocumentId": { "type": "integer" },
                    "sampleDocumentId":    { "type": "integer" },
                    "dpi": { "type": "integer", "description": "Render DPI. Default 200." }
                },
                "required": ["generatedDocumentId", "sampleDocumentId"]
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: Input = serde_json::from_value(input)?;
        let state = ctx.app.state::<AppState>();
        let pool = &state.db.pool;
        let dpi = p.dpi.unwrap_or(200);

        let sample_png = render_pdf_first_page(&ctx.app, pool, p.sample_document_id, dpi).await?;
        let generated_png =
            render_pdf_first_page(&ctx.app, pool, p.generated_document_id, dpi).await?;

        // Build the multimodal user message and dispatch to the
        // user's configured provider.
        let prof_row: (String, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT llm_provider, ollama_url, model FROM user_profile WHERE id = 1",
        )
        .fetch_one(pool)
        .await
        .map_err(|e| anyhow::anyhow!("user_profile: {e}"))?;
        let (llm_provider, ollama_url, model) = prof_row;
        let api_key = match llm_provider.as_str() {
            "travis_cloud" => None,
            "claude" | "openai" => secrets::get_api_key(&llm_provider),
            _ => None,
        };
        let provider = llm::build(
            &llm_provider,
            api_key.as_deref(),
            ollama_url.as_deref(),
            model.as_deref(),
            ctx.http.clone(),
        )
        .map_err(|e| anyhow::anyhow!("build provider: {e}"))?;

        let mut compare_msg = Message::user(
            "Sample is image 1, generated attempt is image 2. Compare carefully.",
        );
        compare_msg.images = vec![
            MessageImage {
                mime_type: "image/png".into(),
                base64_data: B64.encode(&sample_png),
            },
            MessageImage {
                mime_type: "image/png".into(),
                base64_data: B64.encode(&generated_png),
            },
        ];

        let resp = provider
            .chat(
                vec![compare_msg],
                ChatOptions {
                    system: Some(COMPARISON_PROMPT.to_string()),
                    max_tokens: Some(1800),
                    temperature: Some(0.2),
                    cache_system: true,
                    cache_conversation: false,
                    json_mode: false,
                },
            )
            .await
            .map_err(|e| anyhow::anyhow!("vision compare chat: {e}"))?;

        Ok(json!({
            "report": resp.content,
            "sampleDocumentId": p.sample_document_id,
            "generatedDocumentId": p.generated_document_id,
        })
        .to_string())
    }
}

async fn render_pdf_first_page(
    app: &tauri::AppHandle,
    pool: &sqlx::SqlitePool,
    document_id: i64,
    dpi: u32,
) -> anyhow::Result<Vec<u8>> {
    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT relative_path, mime_type FROM document WHERE id = ?1",
    )
    .bind(document_id)
    .fetch_optional(pool)
    .await?;
    let (rel_path, mime) =
        row.ok_or_else(|| anyhow::anyhow!("document {document_id} not found"))?;
    if !mime.contains("pdf") {
        anyhow::bail!("document {document_id} is not a PDF (mime={mime})");
    }
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| anyhow::anyhow!("app_data_dir: {e}"))?;
    let storage_root = storage::storage_root(&data_dir)?;
    let pdf_abs = storage::absolute_path(&storage_root, Path::new(&rel_path));
    if !pdf_abs.exists() {
        anyhow::bail!("PDF file missing on disk: {}", pdf_abs.display());
    }

    let py_bin = crate::python_runtime::resolve_python_bin(app)
        .ok_or_else(|| anyhow::anyhow!("bundled python not found"))?;

    let scratch = std::env::temp_dir()
        .join("travis-verify-render")
        .join(document_id.to_string());
    if scratch.exists() {
        let _ = std::fs::remove_dir_all(&scratch);
    }
    std::fs::create_dir_all(&scratch)?;
    let script_path = scratch.join("_render.py");
    tokio::fs::write(&script_path, RENDER_PAGE_PY).await?;
    let out_path = scratch.join("page.png");

    let mut cmd = tokio::process::Command::new(&py_bin);
    cmd.arg("-u")
        .arg(&script_path)
        .arg(&pdf_abs)
        .arg(&out_path)
        .arg(dpi.to_string())
        .env("PYTHONIOENCODING", "utf-8")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let output = cmd.output().await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "render PDF subprocess failed (status {:?}): {stderr}",
            output.status.code()
        );
    }

    let bytes = tokio::fs::read(&out_path).await?;
    let _ = tokio::fs::remove_dir_all(&scratch).await;
    Ok(bytes)
}
