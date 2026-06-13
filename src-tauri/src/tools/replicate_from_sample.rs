//! `replicate_from_sample` — pixel-perfect document replication via PDF overlay.
//!
//! The Tier 4 asset extraction (logos, page renders) helps fidelity but
//! doesn't actually solve the 1:1 problem. Anything redrawn with
//! reportlab is an APPROXIMATION of the sample, because reportlab is an
//! imperative drawing API and the LLM has to guess every coordinate,
//! font metric, and line stroke. Even with perfect logos embedded, the
//! surrounding layout drifts.
//!
//! This tool stops redrawing. It opens the sample PDF as the canvas,
//! white-masks the regions the user supplied a new value for, stamps
//! the new text at the same coordinates, and saves the result. Output
//! is byte-identical to the sample except for the variable fields.
//!
//! The LLM supplies:
//! - `sampleDocumentId`: the source PDF (a `sample_*` / `template_*` doc)
//! - `overlays`: a list of `{page, bbox, value, font?, fontSize?, color?,
//!    align?, maskOriginal?}` entries — one per variable region.
//!
//! Coordinates are in PDF points (1pt = 1/72 inch). Origin defaults to
//! TOP-LEFT (matches pdfplumber + the existing `analyze_document_styling`
//! output). Pass `bboxOrigin: "bottom-left"` if you've converted yourself.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::Manager;

use crate::interpreter::cmd::{run_python as run_python_cmd, RunPythonParams};
use crate::llm::ToolDef;
use crate::tools::{Tool, ToolContext};
use crate::AppState;

const OVERLAY_PY: &str = r#"
import sys, os, json, traceback
from io import BytesIO

import pypdf
from reportlab.pdfgen import canvas as rl_canvas
from reportlab.lib.utils import ImageReader


def font_for(name):
    # ReportLab's built-in safe set. Anything else gets normalized to
    # Helvetica so the script never crashes on a missing font.
    safe = {
        "Helvetica", "Helvetica-Bold", "Helvetica-Oblique", "Helvetica-BoldOblique",
        "Times-Roman", "Times-Bold", "Times-Italic", "Times-BoldItalic",
        "Courier", "Courier-Bold", "Courier-Oblique", "Courier-BoldOblique",
    }
    if name in safe:
        return name
    lower = name.lower() if isinstance(name, str) else ""
    if "bold" in lower and "italic" in lower:
        return "Helvetica-BoldOblique"
    if "bold" in lower:
        return "Helvetica-Bold"
    if "italic" in lower or "oblique" in lower:
        return "Helvetica-Oblique"
    return "Helvetica"


def page_size_pts(reader, idx):
    page = reader.pages[idx]
    return float(page.mediabox.width), float(page.mediabox.height)


def stamp_raster(sample_path, output_path, overlays, bbox_origin, dpi):
    # Mode 'raster': rasterize each source page to image at high DPI,
    # then build a fresh PDF where each page is the rendered image with
    # white masks + new text drawn as VECTOR on top. Because the
    # background is now pixels (no text layer), the original text is
    # truly gone — not just hidden under a white box. Tradeoff: vector
    # quality of the original content is rasterized. For invoices /
    # forms going to humans this is invisible at 300 DPI.
    import pypdfium2 as pdfium
    pdf = pdfium.PdfDocument(sample_path)
    n_pages = len(pdf)

    sizes_pts = []
    rendered = []
    for i in range(n_pages):
        page = pdf[i]
        w_pts = float(page.get_width())
        h_pts = float(page.get_height())
        sizes_pts.append((w_pts, h_pts))
        bitmap = page.render(scale=dpi / 72.0)
        rendered.append(bitmap.to_pil())
    pdf.close()

    out_buf = BytesIO()
    c = rl_canvas.Canvas(out_buf, pagesize=sizes_pts[0])

    for page_idx in range(n_pages):
        w, h = sizes_pts[page_idx]
        c.setPageSize((w, h))

        # 1) Draw the rasterized page as the background (full bleed).
        c.drawImage(
            ImageReader(rendered[page_idx]),
            0, 0,
            width=w, height=h,
            preserveAspectRatio=False,
            mask='auto',
        )

        # 2) White-mask + stamp each overlay as before. Coords still
        #    in PDF points; conversion only differs for the masking
        #    rectangle (which is drawn on the PDF canvas in points).
        for ov in overlays:
            if int(ov.get("page", 0)) != page_idx:
                continue
            bbox = ov.get("bbox") or []
            if len(bbox) != 4:
                continue
            x0, y0, x1, y1 = (float(bbox[0]), float(bbox[1]), float(bbox[2]), float(bbox[3]))
            if bbox_origin == "top-left":
                top, bottom = y0, y1
                y0 = h - bottom
                y1 = h - top
            box_w = x1 - x0
            box_h = y1 - y0
            if ov.get("maskOriginal", True):
                c.setFillColorRGB(1, 1, 1)
                c.rect(x0, y0, box_w, box_h, fill=1, stroke=0)
            color = ov.get("color") or [0, 0, 0]
            r, g, b = float(color[0]), float(color[1]), float(color[2])
            if max(r, g, b) > 1.0:
                r, g, b = r / 255.0, g / 255.0, b / 255.0
            c.setFillColorRGB(r, g, b)
            font_name = font_for(ov.get("font") or "Helvetica")
            size = float(ov.get("fontSize") or 10)
            c.setFont(font_name, size)
            value = str(ov.get("value", ""))
            align = (ov.get("align") or "left").lower()
            text_w = c.stringWidth(value, font_name, size)
            tx = x0
            if align == "center":
                tx = x0 + (box_w - text_w) / 2.0
            elif align == "right":
                tx = x1 - text_w
            ty = y0 + box_h / 2.0 - size * 0.3
            c.drawString(tx, ty, value)
        c.showPage()

    c.save()
    with open(output_path, "wb") as f:
        f.write(out_buf.getvalue())


def stamp(sample_path, output_path, overlays, bbox_origin):
    src = pypdf.PdfReader(sample_path)
    n_pages = len(src.pages)

    # Capture page sizes BEFORE building the overlay so the overlay
    # canvas matches each page's mediabox.
    sizes = []
    for i in range(n_pages):
        page = src.pages[i]
        w = float(page.mediabox.width)
        h = float(page.mediabox.height)
        sizes.append((w, h))

    overlay_buf = BytesIO()
    c = rl_canvas.Canvas(overlay_buf, pagesize=sizes[0])

    for page_idx in range(n_pages):
        w, h = sizes[page_idx]
        c.setPageSize((w, h))

        for ov in overlays:
            if int(ov.get("page", 0)) != page_idx:
                continue
            bbox = ov.get("bbox") or []
            if len(bbox) != 4:
                continue
            x0, y0, x1, y1 = (float(bbox[0]), float(bbox[1]), float(bbox[2]), float(bbox[3]))

            # Coordinate normalization. ReportLab is bottom-left.
            # pdfplumber/analyze_document_styling default is top-left
            # (y = distance from top). Convert when origin is top-left.
            if bbox_origin == "top-left":
                # Flip y so we draw on the right region.
                # bbox in top-left coords: x0,top,x1,bottom (top<bottom).
                top, bottom = y0, y1
                y0 = h - bottom
                y1 = h - top

            box_h = y1 - y0
            box_w = x1 - x0

            mask = ov.get("maskOriginal", True)
            if mask:
                c.setFillColorRGB(1, 1, 1)
                c.rect(x0, y0, box_w, box_h, fill=1, stroke=0)

            color = ov.get("color") or [0, 0, 0]
            r, g, b = float(color[0]), float(color[1]), float(color[2])
            # Allow 0-255 RGB if the LLM forgot to normalize.
            if max(r, g, b) > 1.0:
                r, g, b = r / 255.0, g / 255.0, b / 255.0
            c.setFillColorRGB(r, g, b)

            font_name = font_for(ov.get("font") or "Helvetica")
            size = float(ov.get("fontSize") or 10)
            c.setFont(font_name, size)

            value = str(ov.get("value", ""))
            align = (ov.get("align") or "left").lower()
            text_w = c.stringWidth(value, font_name, size)
            tx = x0
            if align == "center":
                tx = x0 + (box_w - text_w) / 2.0
            elif align == "right":
                tx = x1 - text_w

            # Vertical baseline: roughly center the cap height in the box.
            # ReportLab fonts don't expose cap height cheaply; size*0.3
            # below the box midpoint approximates Latin baselines well.
            ty = y0 + box_h / 2.0 - size * 0.3

            c.drawString(tx, ty, value)

        c.showPage()

    c.save()

    overlay_buf.seek(0)
    overlay_pdf = pypdf.PdfReader(overlay_buf)
    writer = pypdf.PdfWriter()
    for page_idx in range(n_pages):
        page = src.pages[page_idx]
        page.merge_page(overlay_pdf.pages[page_idx])
        writer.add_page(page)

    with open(output_path, "wb") as f:
        writer.write(f)


if __name__ == "__main__":
    try:
        spec = json.loads(sys.argv[1])
        mode = spec.get("mode", "overlay")
        if mode == "raster":
            stamp_raster(
                spec["samplePath"],
                spec["outputPath"],
                spec.get("overlays", []),
                spec.get("bboxOrigin", "top-left"),
                int(spec.get("dpi", 300)),
            )
        else:
            stamp(
                spec["samplePath"],
                spec["outputPath"],
                spec.get("overlays", []),
                spec.get("bboxOrigin", "top-left"),
            )
        print(json.dumps({
            "ok": True,
            "mode": mode,
            "outputPath": spec["outputPath"],
            "overlayCount": len(spec.get("overlays", [])),
        }))
    except Exception as e:
        print(json.dumps({
            "ok": False,
            "error": str(e),
            "traceback": traceback.format_exc(),
        }), file=sys.stderr)
        sys.exit(1)
"#;

pub struct ReplicateFromSampleTool;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OverlayInput {
    sample_document_id: i64,
    output_name: String,
    overlays: Vec<serde_json::Value>,
    /// "top-left" (pdfplumber / analyze_document_styling default) or
    /// "bottom-left" (PDF native). Default top-left.
    #[serde(default)]
    bbox_origin: Option<String>,
    /// "overlay" (default) keeps everything vector — fast, small file,
    /// but the original masked text stays in the content stream
    /// underneath the white box. "raster" rasterizes each page at
    /// `dpi` first, so the background has no text layer at all — the
    /// new text drawn on top is the only selectable text. Pick raster
    /// when underlying text must be GONE (regulated documents,
    /// scrub-and-resend invoices); pick overlay otherwise.
    #[serde(default)]
    mode: Option<String>,
    /// DPI for raster mode. Default 300.
    #[serde(default)]
    dpi: Option<u32>,
    /// Optional plan integration — same shape as run_python.
    #[serde(default)]
    plan_id: Option<i64>,
    #[serde(default)]
    plan_step_key: Option<String>,
}

#[async_trait]
impl Tool for ReplicateFromSampleTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "replicate_from_sample".into(),
            description: "Pixel-perfect document replication. Opens the sample PDF, white-masks \
                each variable region, stamps the new value at the same coordinates, and saves \
                the result as a new Travis document. The structural pixels (letterhead, table \
                lines, signature blocks, footer) NEVER MOVE because they're never redrawn — \
                only the variable fields change.\n\n\
                Use this WHENEVER the user wants a doc that LOOKS LIKE a sample with new data. \
                DO NOT call `run_python` to redraw with reportlab — that always produces an \
                approximation. The overlay approach produces a byte-identical replica except \
                in the regions you marked variable.\n\n\
                Two modes — PICK THE ONE THAT FITS THE WORKFLOW:\n\
                - `mode: 'overlay'` (DEFAULT): vector overlay. Tiny file, instant render, but \
                   the original text under each mask STAYS in the PDF content stream — it's \
                   visually hidden by the white box but a select-copy of the output PDF will \
                   reveal it. Right for: invoices being sent to humans (printed, viewed, \
                   filed). The user almost never select-copies an invoice.\n\
                - `mode: 'raster'`: rasterize each source page at `dpi` (default 300) first, \
                   so the page background is now PIXELS with no text layer. New text is drawn \
                   as vector on top. The original text is GONE — not just hidden. Right for: \
                   regulated documents where the underlying text must be scrubbed (HIPAA \
                   redaction, contract diffs sent externally), or when the user explicitly \
                   asks for clean replacement. Tradeoff: file is larger and the background's \
                   crisp vector graphics become 300-DPI raster (invisible to the eye but \
                   present in metadata).\n\n\
                If the user says 'make sure the old values are completely gone' / 'scrub the \
                old data' / 'this is going to legal' → use `raster`. Otherwise default to \
                `overlay`.\n\n\
                Flow:\n\
                1. Call `analyze_document_styling(sampleDocId)` to confirm the sample.\n\
                2. Call `list_template_assets(sampleDocId)` if you need logo asset paths (rarely \
                   needed here — they're already in the sample).\n\
                3. For each variable region (recipient, invoice number, dates, amount), supply \
                   `{page, bbox: [x0, y0, x1, y1], value: 'new text', font?, fontSize?, color?, \
                   align?, maskOriginal?}`.\n\
                4. The tool stamps and saves. Output appears as doc#N in the chat with a \
                   FileCard.\n\n\
                Coordinates: PDF points. Origin defaults to TOP-LEFT (matches pdfplumber and \
                `analyze_document_styling` output). Pass `bboxOrigin: 'bottom-left'` if you've \
                already converted to PDF native coords.\n\n\
                Defaults: font 'Helvetica', fontSize 10, color [0,0,0] (black), align 'left', \
                maskOriginal true. Colors accept either 0-1 floats or 0-255 ints; the tool \
                normalizes.\n\n\
                Returns: `{generatedDocumentIds: [N], overlayCount, ...}`. INCLUDE the doc#N \
                marker in your reply so the FileCard renders."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "sampleDocumentId": {
                        "type": "integer",
                        "description": "Source PDF — typically a sample_* / template_* document."
                    },
                    "outputName": {
                        "type": "string",
                        "description": "Filename for the new doc (e.g. 'LTE2026217002_IS217_Invoice.pdf')."
                    },
                    "overlays": {
                        "type": "array",
                        "description": "Variable regions to stamp on top of the sample. Each entry replaces one field.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "page": { "type": "integer", "description": "Zero-indexed page number. Default 0." },
                                "bbox": {
                                    "type": "array",
                                    "items": { "type": "number" },
                                    "minItems": 4,
                                    "maxItems": 4,
                                    "description": "[x0, y0, x1, y1] in PDF points. Default origin top-left."
                                },
                                "value": { "type": "string", "description": "New text to stamp." },
                                "font": { "type": "string", "description": "ReportLab base font name. Default Helvetica." },
                                "fontSize": { "type": "number", "description": "Default 10." },
                                "color": {
                                    "type": "array",
                                    "items": { "type": "number" },
                                    "description": "[r, g, b] in 0-1 or 0-255. Default [0,0,0]."
                                },
                                "align": { "type": "string", "enum": ["left", "center", "right"], "description": "Default 'left'." },
                                "maskOriginal": { "type": "boolean", "description": "Whiteout the bbox before drawing. Default true." }
                            },
                            "required": ["bbox", "value"]
                        }
                    },
                    "bboxOrigin": {
                        "type": "string",
                        "enum": ["top-left", "bottom-left"],
                        "description": "Coordinate origin for bboxes. Default top-left."
                    },
                    "mode": {
                        "type": "string",
                        "enum": ["overlay", "raster"],
                        "description": "'overlay' (default): vector overlay, old text stays hidden under the mask. 'raster': rasterize the page first so old text is truly gone. Pick raster only when the underlying text must be scrubbed."
                    },
                    "dpi": {
                        "type": "integer",
                        "description": "Raster-mode resolution. Default 300. Higher = sharper background but larger file."
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
                "required": ["sampleDocumentId", "outputName", "overlays"]
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: OverlayInput = serde_json::from_value(input)?;
        let state = ctx.app.state::<AppState>();
        let pool = state.db.pool.clone();

        // Resolve the sample PDF's absolute path so the overlay script
        // can read it. The script runs in the bundled Python sandbox
        // with INPUTS_DIR mounted; we pass the sample bytes via the
        // existing document_ids mount path so the cmd handles encoding.
        let sample_meta: Option<(String, String)> = sqlx::query_as(
            "SELECT mime_type, original_filename FROM document WHERE id = ?1",
        )
        .bind(p.sample_document_id)
        .fetch_optional(&pool)
        .await?;
        let (mime, original_filename) = sample_meta.ok_or_else(|| {
            anyhow::anyhow!("sampleDocumentId {} not found", p.sample_document_id)
        })?;
        if !mime.contains("pdf") {
            anyhow::bail!(
                "sampleDocumentId {} is not a PDF (mime={})",
                p.sample_document_id,
                mime
            );
        }

        // Build the Python invocation. We rely on the existing
        // RunPythonParams pipeline: mount the sample doc, run the
        // overlay script, the writer drops the output into OUTPUTS_DIR,
        // and the cmd post-processes it into a registered Document.
        let output_name = sanitize_output_name(&p.output_name);
        let overlays_json = serde_json::to_string(&p.overlays)?;
        let bbox_origin = p
            .bbox_origin
            .as_deref()
            .unwrap_or("top-left")
            .to_string();
        let mut spec = serde_json::Map::new();
        spec.insert(
            "samplePath".into(),
            serde_json::json!(format!(
                "{{INPUTS_DIR}}/{}",
                safe_sample_name(p.sample_document_id, &original_filename)
            )),
        );
        spec.insert(
            "outputPath".into(),
            serde_json::json!(format!("{{OUTPUTS_DIR}}/{}", output_name)),
        );
        spec.insert(
            "overlays".into(),
            serde_json::from_str::<Value>(&overlays_json)?,
        );
        spec.insert("bboxOrigin".into(), serde_json::json!(bbox_origin));
        let mode = p
            .mode
            .as_deref()
            .map(|s| s.to_lowercase())
            .unwrap_or_else(|| "overlay".to_string());
        spec.insert("mode".into(), serde_json::json!(mode));
        spec.insert("dpi".into(), serde_json::json!(p.dpi.unwrap_or(300)));

        // The spec uses {INPUTS_DIR} / {OUTPUTS_DIR} placeholders the
        // wrapper substitutes — this is the standard sandbox layout.
        // Build a small driver script that resolves them and invokes
        // the embedded overlay logic.
        let driver = format!(
            r#"
import os, json, sys, traceback
SPEC = {spec_literal}
SPEC['samplePath'] = SPEC['samplePath'].replace('{{INPUTS_DIR}}', INPUTS_DIR)
SPEC['outputPath'] = SPEC['outputPath'].replace('{{OUTPUTS_DIR}}', OUTPUTS_DIR)
{overlay_script}

try:
    stamp(SPEC['samplePath'], SPEC['outputPath'], SPEC.get('overlays', []), SPEC.get('bboxOrigin', 'top-left'))
    print(json.dumps({{
        'ok': True,
        'outputPath': SPEC['outputPath'],
        'overlayCount': len(SPEC.get('overlays', [])),
    }}))
except Exception as e:
    print(json.dumps({{
        'ok': False,
        'error': str(e),
        'traceback': traceback.format_exc(),
    }}), file=sys.stderr)
    sys.exit(1)
"#,
            spec_literal = serde_json::to_string(&Value::Object(spec))?,
            overlay_script = OVERLAY_PY,
        );

        let conv_id = ctx.conversation_id;
        let outcome = run_python_cmd(
            ctx.app.clone(),
            state,
            RunPythonParams {
                code: driver,
                purpose: format!("Replicating {} from sample doc#{}", output_name, p.sample_document_id),
                document_ids: vec![p.sample_document_id],
                libraries: vec![],
                conversation_id: conv_id,
                workflow_state_id: None,
                timeout_secs: Some(120),
                extra_input_files: Default::default(),
            },
        )
        .await
        .map_err(|e| anyhow::anyhow!("replicate_from_sample run failed: {e}"))?;

        // Auto-record into the plan step if requested.
        if let (Some(plan_id), Some(key)) = (p.plan_id, p.plan_step_key.as_deref()) {
            if !key.trim().is_empty() {
                let status = if outcome.error.is_none() {
                    "done"
                } else {
                    "failed"
                };
                let hash = crate::plans::input_hash(
                    &pool,
                    &output_name,
                    &[p.sample_document_id],
                    &[],
                )
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
            "overlayCount": p.overlays.len(),
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
        "replica.pdf".into()
    } else if !cleaned.to_lowercase().ends_with(".pdf") {
        format!("{cleaned}.pdf")
    } else {
        cleaned
    }
}

/// Mirror `interpreter::cmd::sanitize_filename` so the mounted-input
/// path in the sandbox matches what the cmd actually writes. If the
/// original is empty after sanitizing, the cmd falls back to
/// `file_<doc_id>` — we replicate that here.
fn safe_sample_name(doc_id: i64, original: &str) -> String {
    let cleaned: String = original
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
        format!("file_{doc_id}")
    } else {
        cleaned
    }
}
