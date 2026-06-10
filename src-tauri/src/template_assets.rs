//! v0.20.2+ Tier 4 — global, deduped, categorized binary asset library
//! extracted from sample/template PDFs.
//!
//! Pairs with `analyze_document_styling`. That tool sends the PDF to
//! Claude vision and caches a JSON description ("Arial 12pt, navy
//! header, logo at top-left"). This module extracts the actual pixel
//! data — logos, embedded images, full-page renders — so the LLM's
//! Python script can embed them verbatim instead of approximating
//! from the JSON description.
//!
//! Architecture, in order of importance:
//!
//! 1. **Content-addressed assets.** Each extracted image hashed and
//!    stored once at `<app_data>/template_assets/<hash[:2]>/<hash>.png`.
//!    The same L2E logo lifted from twenty sample invoices is one
//!    file on disk and one row in `template_asset`, not twenty.
//!
//! 2. **Kind + display_name.** Each row is categorized at extraction
//!    time via simple heuristics (position, dimensions, page) into
//!    `logo` / `header_banner` / `signature` / `watermark` /
//!    `page_render` / `embedded_image`. The display_name is set from
//!    the source document name so the LLM can ground "use the L2E
//!    logo" against an actual row.
//!
//! 3. **Source linking.** `template_asset_source` is N:M asset ↔ doc
//!    with page + bbox. Future docs can reference any asset by id,
//!    kind, or display_name without needing the original sample
//!    attached. Reuse across docs is the point.
//!
//! 4. **Background-only.** Extraction runs off the chat turn via
//!    `schedule_extraction`, which the capture pipeline calls when a
//!    doc is classified as sample/template. The LLM only sees results
//!    after `status = 'ready'`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tauri::{AppHandle, Manager, State};

use crate::AppState;

/// Inline Python extractor — saves every page raster + every embedded
/// image to a flat output dir, prints a JSON index that the Rust side
/// hashes and ingests into the global asset library.
///
/// Bundled Python deps used (in `resources/python/<slug>/python/Lib/site-packages`):
/// - pdfplumber (bbox extraction)
/// - pypdfium2 (page rasterization)
/// - PIL (Pillow — image I/O)
const EXTRACTOR_PY: &str = r#"
import sys, os, json, traceback
from pathlib import Path

def main(pdf_path, out_dir):
    out = Path(out_dir)
    out.mkdir(parents=True, exist_ok=True)

    result = {
        "pages": [],
        "images": [],
        "fonts": [],
        "colors": [],
    }
    seen_fonts = set()
    seen_colors = set()

    # 1) High-DPI page raster (full-fidelity visual reference).
    try:
        import pypdfium2 as pdfium
        pdf = pdfium.PdfDocument(pdf_path)
        for page_idx in range(len(pdf)):
            page = pdf[page_idx]
            bitmap = page.render(scale=300/72)
            pil_image = bitmap.to_pil()
            raster_path = out / f"page_{page_idx}.png"
            pil_image.save(raster_path)
            result["pages"].append({
                "index": page_idx,
                "width_pts":  float(page.get_width()),
                "height_pts": float(page.get_height()),
                "raster_path": str(raster_path),
                "raster_dpi": 300,
                "raster_width_px":  pil_image.size[0],
                "raster_height_px": pil_image.size[1],
            })
        pdf.close()
    except Exception as e:
        print(f"page raster failed: {e}", file=sys.stderr)

    # 2) Embedded images + fonts + colors via pdfplumber.
    try:
        import pdfplumber
        with pdfplumber.open(pdf_path) as plumb:
            img_counter = 0
            for page_idx, page in enumerate(plumb.pages):
                page_image = None
                for img in page.images:
                    try:
                        if page_image is None:
                            page_image = page.to_image(resolution=300)
                        scale = 300 / 72
                        bbox = (
                            float(img["x0"]) * scale,
                            float(img["top"]) * scale,
                            float(img["x1"]) * scale,
                            float(img["bottom"]) * scale,
                        )
                        cropped = page_image.original.crop(bbox)
                        img_path = out / f"image_{page_idx}_{img_counter}.png"
                        cropped.save(img_path)
                        result["images"].append({
                            "page": page_idx,
                            "path": str(img_path),
                            "bbox_pts": [
                                float(img["x0"]),
                                float(img["top"]),
                                float(img["x1"]),
                                float(img["bottom"]),
                            ],
                            "page_width_pts":  float(page.width),
                            "page_height_pts": float(page.height),
                            "width_px":  cropped.size[0],
                            "height_px": cropped.size[1],
                        })
                        img_counter += 1
                    except Exception as inner:
                        print(f"image extract failed page {page_idx}: {inner}", file=sys.stderr)

                for char in page.chars[:5000]:
                    fname = char.get("fontname")
                    if fname and fname not in seen_fonts:
                        seen_fonts.add(fname)
                    color = char.get("non_stroking_color")
                    if color is not None:
                        key = tuple(color) if isinstance(color, (list, tuple)) else (color,)
                        if key not in seen_colors:
                            seen_colors.add(key)
    except Exception as e:
        print(f"pdfplumber pass failed: {e}", file=sys.stderr)

    result["fonts"]  = sorted(seen_fonts)
    result["colors"] = [list(c) for c in seen_colors]
    result["ok"] = True
    print(json.dumps(result))

if __name__ == "__main__":
    if len(sys.argv) < 3:
        print(json.dumps({"ok": False, "error": "usage: extractor.py <pdf> <out_dir>"}))
        sys.exit(1)
    try:
        main(sys.argv[1], sys.argv[2])
    except Exception as e:
        print(json.dumps({"ok": False, "error": str(e), "traceback": traceback.format_exc()}))
        sys.exit(1)
"#;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct TemplateAssetRow {
    pub id: i64,
    pub workspace_id: i64,
    pub content_hash: String,
    pub abs_path: String,
    pub kind: String,
    pub display_name: String,
    pub width_px: Option<i64>,
    pub height_px: Option<i64>,
    pub size_bytes: Option<i64>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct TemplateAssetSourceRow {
    pub asset_id: i64,
    pub document_id: i64,
    pub page: i64,
    pub bbox_pts: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct TemplateExtractionRow {
    pub document_id: i64,
    pub status: String,
    pub page_count: i64,
    pub image_count: i64,
    pub manifest_json: Option<String>,
    pub error: Option<String>,
    pub extracted_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ExtractorResult {
    #[serde(default)]
    ok: bool,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    pages: Vec<ExtractedPage>,
    #[serde(default)]
    images: Vec<ExtractedImage>,
    #[serde(default)]
    fonts: Vec<String>,
    #[serde(default)]
    colors: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct ExtractedPage {
    index: i64,
    width_pts: f64,
    height_pts: f64,
    raster_path: String,
    raster_dpi: i64,
    #[serde(default)]
    raster_width_px: i64,
    #[serde(default)]
    raster_height_px: i64,
}

#[derive(Debug, Deserialize)]
struct ExtractedImage {
    page: i64,
    path: String,
    bbox_pts: [f64; 4],
    page_width_pts: f64,
    page_height_pts: f64,
    width_px: i64,
    height_px: i64,
}

/// Root storage dir for the asset library: `<app_data>/template_assets`.
fn library_root(app: &AppHandle) -> std::io::Result<PathBuf> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?
        .join("template_assets");
    std::fs::create_dir_all(&base)?;
    Ok(base)
}

/// Per-extraction scratch dir under the system temp root.
fn extraction_scratch_dir(document_id: i64) -> std::io::Result<PathBuf> {
    let base = std::env::temp_dir()
        .join("travis-template-extract")
        .join(document_id.to_string());
    if base.exists() {
        let _ = std::fs::remove_dir_all(&base);
    }
    std::fs::create_dir_all(&base)?;
    Ok(base)
}

/// Resolve a document's absolute path on disk + its display name.
async fn resolve_document(
    app: &AppHandle,
    pool: &SqlitePool,
    document_id: i64,
) -> anyhow::Result<(PathBuf, String, i64, String)> {
    let (rel, mime, ws_id, name): (String, String, i64, String) = sqlx::query_as(
        "SELECT relative_path, mime_type, workspace_id, display_name
         FROM document WHERE id = ?1",
    )
    .bind(document_id)
    .fetch_one(pool)
    .await?;
    let storage_root = crate::documents::storage::storage_root(
        &app.path()
            .app_data_dir()
            .map_err(|e| anyhow::anyhow!("app_data_dir: {e}"))?,
    )?;
    let abs = crate::documents::storage::absolute_path(&storage_root, Path::new(&rel));
    Ok((abs, mime, ws_id, name))
}

/// Schedule extraction for a document. Idempotent — pending/extracting/
/// ready rows return early; failed rows are retried.
pub async fn schedule_extraction(app: AppHandle, pool: SqlitePool, document_id: i64) {
    let existing: Option<(String,)> =
        sqlx::query_as("SELECT status FROM template_extraction WHERE document_id = ?1")
            .bind(document_id)
            .fetch_optional(&pool)
            .await
            .ok()
            .flatten();

    match existing.as_ref().map(|(s,)| s.as_str()) {
        Some("ready") | Some("extracting") | Some("pending") => return,
        _ => {}
    }

    let _ = sqlx::query(
        "INSERT INTO template_extraction(document_id, status)
         VALUES (?1, 'pending')
         ON CONFLICT(document_id) DO UPDATE SET status='pending', error=NULL",
    )
    .bind(document_id)
    .execute(&pool)
    .await;

    tauri::async_runtime::spawn(async move {
        let _ = run_extraction(&app, &pool, document_id).await;
    });
}

async fn run_extraction(
    app: &AppHandle,
    pool: &SqlitePool,
    document_id: i64,
) -> anyhow::Result<()> {
    let _ = sqlx::query("UPDATE template_extraction SET status='extracting' WHERE document_id=?1")
        .bind(document_id)
        .execute(pool)
        .await;

    let (pdf_path, mime, ws_id, doc_display_name) =
        match resolve_document(app, pool, document_id).await {
            Ok(t) => t,
            Err(e) => {
                fail(pool, document_id, format!("resolve path: {e}")).await;
                return Ok(());
            }
        };

    if !mime.contains("pdf") {
        fail(pool, document_id, format!("not a PDF (mime={mime}); skipping")).await;
        return Ok(());
    }
    if !pdf_path.exists() {
        fail(pool, document_id, format!("missing file: {}", pdf_path.display())).await;
        return Ok(());
    }

    let py_bin = match crate::python_runtime::resolve_python_bin(app) {
        Some(p) => p,
        None => {
            fail(pool, document_id, "bundled python not found".into()).await;
            return Ok(());
        }
    };

    let scratch = match extraction_scratch_dir(document_id) {
        Ok(p) => p,
        Err(e) => {
            fail(pool, document_id, format!("create scratch: {e}")).await;
            return Ok(());
        }
    };

    let script_path = scratch.join("_extractor.py");
    if let Err(e) = tokio::fs::write(&script_path, EXTRACTOR_PY).await {
        fail(pool, document_id, format!("write script: {e}")).await;
        return Ok(());
    }

    let mut cmd = tokio::process::Command::new(&py_bin);
    cmd.arg("-u")
        .arg(&script_path)
        .arg(&pdf_path)
        .arg(&scratch)
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

    let output = match cmd.output().await {
        Ok(o) => o,
        Err(e) => {
            fail(pool, document_id, format!("spawn python: {e}")).await;
            return Ok(());
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        fail(
            pool,
            document_id,
            format!(
                "extractor exited {:?}: {}",
                output.status.code(),
                if stderr.is_empty() { stdout.clone() } else { stderr.clone() }
            ),
        )
        .await;
        let _ = tokio::fs::remove_dir_all(&scratch).await;
        return Ok(());
    }

    let parsed: ExtractorResult = match serde_json::from_str(stdout.trim()) {
        Ok(p) => p,
        Err(e) => {
            fail(
                pool,
                document_id,
                format!("parse extractor result: {e}; stdout={stdout}"),
            )
            .await;
            let _ = tokio::fs::remove_dir_all(&scratch).await;
            return Ok(());
        }
    };
    if !parsed.ok {
        fail(
            pool,
            document_id,
            parsed.error.unwrap_or_else(|| "extractor returned ok=false".into()),
        )
        .await;
        let _ = tokio::fs::remove_dir_all(&scratch).await;
        return Ok(());
    }

    // Ingest into the global library.
    let library = match library_root(app) {
        Ok(p) => p,
        Err(e) => {
            fail(pool, document_id, format!("library root: {e}")).await;
            let _ = tokio::fs::remove_dir_all(&scratch).await;
            return Ok(());
        }
    };

    let mut manifest_assets: Vec<serde_json::Value> = Vec::new();
    let image_count = parsed.images.len() as i64;
    let page_count = parsed.pages.len() as i64;

    // -- Page renders. Kind = page_render. One asset per page.
    for page in &parsed.pages {
        let kind = "page_render";
        let label = format!("{} – page {} @ 300 DPI", doc_display_name, page.index + 1);
        if let Some(asset_id) = ingest_image(
            pool,
            ws_id,
            &library,
            document_id,
            Path::new(&page.raster_path),
            kind,
            &label,
            page.raster_width_px,
            page.raster_height_px,
            page.index,
            None,
        )
        .await
        {
            manifest_assets.push(serde_json::json!({
                "assetId": asset_id,
                "kind": kind,
                "displayName": label,
                "page": page.index,
                "widthPx":  page.raster_width_px,
                "heightPx": page.raster_height_px,
                "widthPts":  page.width_pts,
                "heightPts": page.height_pts,
            }));
        }
    }

    // -- Embedded images. Kind heuristically inferred from page position.
    for img in &parsed.images {
        let kind = infer_image_kind(img);
        let label = format!(
            "{} – {} (page {})",
            doc_display_name,
            display_label_for_kind(kind),
            img.page + 1
        );
        let bbox = serde_json::json!([
            img.bbox_pts[0],
            img.bbox_pts[1],
            img.bbox_pts[2],
            img.bbox_pts[3],
        ])
        .to_string();
        if let Some(asset_id) = ingest_image(
            pool,
            ws_id,
            &library,
            document_id,
            Path::new(&img.path),
            kind,
            &label,
            img.width_px,
            img.height_px,
            img.page,
            Some(bbox.clone()),
        )
        .await
        {
            manifest_assets.push(serde_json::json!({
                "assetId": asset_id,
                "kind": kind,
                "displayName": label,
                "page": img.page,
                "bboxPts": img.bbox_pts,
                "widthPx":  img.width_px,
                "heightPx": img.height_px,
                "pageWidthPts":  img.page_width_pts,
                "pageHeightPts": img.page_height_pts,
            }));
        }
    }

    // Per-doc manifest the LLM tool surfaces.
    let manifest = serde_json::json!({
        "documentId": document_id,
        "sourceName": doc_display_name,
        "pages": parsed.pages.iter().map(|p| serde_json::json!({
            "index": p.index,
            "widthPts":  p.width_pts,
            "heightPts": p.height_pts,
            "rasterDpi": p.raster_dpi,
        })).collect::<Vec<_>>(),
        "fonts":  parsed.fonts,
        "colors": parsed.colors,
        "assets": manifest_assets,
    });
    let manifest_text = manifest.to_string();

    let _ = sqlx::query(
        "UPDATE template_extraction SET
           manifest_json = ?2,
           image_count   = ?3,
           page_count    = ?4,
           status        = 'ready',
           error         = NULL,
           extracted_at  = datetime('now')
         WHERE document_id = ?1",
    )
    .bind(document_id)
    .bind(manifest_text)
    .bind(image_count)
    .bind(page_count)
    .execute(pool)
    .await;

    // Scratch was just per-call; we copied bytes into the library.
    let _ = tokio::fs::remove_dir_all(&scratch).await;

    tracing::info!(
        "template_assets: extracted doc {document_id} ({image_count} images, {page_count} pages)"
    );
    Ok(())
}

/// Hash + content-address + upsert one image into the global library.
/// Returns the asset row id, or None on failure.
async fn ingest_image(
    pool: &SqlitePool,
    workspace_id: i64,
    library: &Path,
    document_id: i64,
    src: &Path,
    kind: &str,
    display_name: &str,
    width_px: i64,
    height_px: i64,
    page: i64,
    bbox_pts: Option<String>,
) -> Option<i64> {
    let bytes = match tokio::fs::read(src).await {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!("ingest_image read {}: {e}", src.display());
            return None;
        }
    };
    let size = bytes.len() as i64;
    use sha2::{Digest, Sha256};
    let hash = {
        let mut h = Sha256::new();
        h.update(&bytes);
        let digest = h.finalize();
        let mut s = String::with_capacity(digest.len() * 2);
        for b in digest.iter() {
            s.push_str(&format!("{b:02x}"));
        }
        s
    };

    let prefix = &hash[..2];
    let dest_dir = library.join(prefix);
    if let Err(e) = tokio::fs::create_dir_all(&dest_dir).await {
        tracing::warn!("ingest_image mkdir {}: {e}", dest_dir.display());
        return None;
    }
    let dest = dest_dir.join(format!("{}.png", hash));
    if tokio::fs::metadata(&dest).await.is_err() {
        if let Err(e) = tokio::fs::write(&dest, &bytes).await {
            tracing::warn!("ingest_image write {}: {e}", dest.display());
            return None;
        }
    }

    // Upsert by content_hash so duplicate bytes share one row.
    let abs = dest.to_string_lossy().to_string();
    let row: Option<(i64,)> = sqlx::query_as(
        "INSERT INTO template_asset
           (workspace_id, content_hash, abs_path, kind, display_name,
            width_px, height_px, mime_type, size_bytes)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'image/png', ?8)
         ON CONFLICT(content_hash) DO UPDATE SET
           updated_at = datetime('now')
         RETURNING id",
    )
    .bind(workspace_id)
    .bind(&hash)
    .bind(&abs)
    .bind(kind)
    .bind(display_name)
    .bind(width_px)
    .bind(height_px)
    .bind(size)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    let asset_id = match row {
        Some((id,)) => id,
        None => {
            // ON CONFLICT path may not return on older sqlite — re-fetch.
            sqlx::query_as::<_, (i64,)>(
                "SELECT id FROM template_asset WHERE content_hash = ?1",
            )
            .bind(&hash)
            .fetch_one(pool)
            .await
            .ok()?
            .0
        }
    };

    // Link asset ↔ source doc (idempotent).
    let _ = sqlx::query(
        "INSERT INTO template_asset_source(asset_id, document_id, page, bbox_pts)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(asset_id, document_id, page) DO UPDATE SET
           bbox_pts = excluded.bbox_pts",
    )
    .bind(asset_id)
    .bind(document_id)
    .bind(page)
    .bind(bbox_pts)
    .execute(pool)
    .await;

    Some(asset_id)
}

/// Heuristic asset-kind inference from image position + dimensions.
fn infer_image_kind(img: &ExtractedImage) -> &'static str {
    let pw = img.page_width_pts.max(1.0);
    let ph = img.page_height_pts.max(1.0);
    let width_frac = (img.bbox_pts[2] - img.bbox_pts[0]) / pw;
    let height_frac = (img.bbox_pts[3] - img.bbox_pts[1]) / ph;
    let top_frac = img.bbox_pts[1] / ph;
    let bottom_frac = img.bbox_pts[3] / ph;
    let aspect = (img.width_px as f64) / (img.height_px.max(1) as f64);

    // Header banner: very wide, very thin, top of page.
    if width_frac >= 0.5 && height_frac <= 0.18 && top_frac < 0.25 {
        return "header_banner";
    }
    // Logo: small, top portion of page.
    if width_frac < 0.35 && height_frac < 0.18 && top_frac < 0.25 {
        return "logo";
    }
    // Signature: bottom of page, wide-ish thin element.
    if bottom_frac > 0.78 && height_frac < 0.15 && aspect > 1.5 {
        return "signature";
    }
    // Watermark: centered, very large but faint signals are hard to
    // detect from bbox alone — only flag dead-center massive images.
    if width_frac > 0.6 && height_frac > 0.5 {
        return "watermark";
    }
    "embedded_image"
}

fn display_label_for_kind(kind: &str) -> &'static str {
    match kind {
        "logo" => "logo",
        "header_banner" => "header banner",
        "signature" => "signature graphic",
        "watermark" => "watermark",
        "page_render" => "page render",
        _ => "embedded image",
    }
}

async fn fail(pool: &SqlitePool, document_id: i64, err: String) {
    tracing::warn!("template_assets: doc {document_id} failed: {err}");
    let _ = sqlx::query(
        "UPDATE template_extraction SET status='failed', error=?2 WHERE document_id=?1",
    )
    .bind(document_id)
    .bind(err)
    .execute(pool)
    .await;
}

/// Read the per-document extraction row for the LLM tool surface.
pub async fn get_extraction(
    pool: &SqlitePool,
    document_id: i64,
) -> anyhow::Result<Option<TemplateExtractionRow>> {
    let row: Option<TemplateExtractionRow> = sqlx::query_as(
        "SELECT document_id, status, page_count, image_count, manifest_json,
                error, extracted_at
         FROM template_extraction WHERE document_id = ?1",
    )
    .bind(document_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Library-wide asset search. `kind` filters by category; `query`
/// is a case-insensitive substring match against display_name; both
/// optional. Caller can scope to a single source doc via `document_id`.
pub async fn find_assets(
    pool: &SqlitePool,
    workspace_id: i64,
    kind: Option<&str>,
    query: Option<&str>,
    document_id: Option<i64>,
    limit: i64,
) -> anyhow::Result<Vec<TemplateAssetRow>> {
    let mut sql = String::from(
        "SELECT DISTINCT a.id, a.workspace_id, a.content_hash, a.abs_path,
                a.kind, a.display_name, a.width_px, a.height_px, a.size_bytes,
                a.created_at
         FROM template_asset a",
    );
    if document_id.is_some() {
        sql.push_str(" JOIN template_asset_source s ON s.asset_id = a.id");
    }
    sql.push_str(" WHERE a.workspace_id = ?1");
    if kind.is_some() {
        sql.push_str(" AND a.kind = ?2");
    }
    if query.is_some() {
        let bind = if kind.is_some() { "?3" } else { "?2" };
        sql.push_str(&format!(" AND lower(a.display_name) LIKE {bind}"));
    }
    if document_id.is_some() {
        let bind = match (kind.is_some(), query.is_some()) {
            (true, true) => "?4",
            (true, false) | (false, true) => "?3",
            (false, false) => "?2",
        };
        sql.push_str(&format!(" AND s.document_id = {bind}"));
    }
    sql.push_str(" ORDER BY a.created_at DESC LIMIT ?100");
    let limit_marker = format!(" LIMIT {}", limit.max(1).min(200));
    sql = sql.replace(" LIMIT ?100", &limit_marker);

    let mut q = sqlx::query_as::<_, TemplateAssetRow>(&sql).bind(workspace_id);
    if let Some(k) = kind {
        q = q.bind(k.to_string());
    }
    if let Some(qstr) = query {
        q = q.bind(format!("%{}%", qstr.to_lowercase()));
    }
    if let Some(d) = document_id {
        q = q.bind(d);
    }
    Ok(q.fetch_all(pool).await?)
}

/// Source docs for an asset — answers "where did this logo come from?"
pub async fn asset_sources(
    pool: &SqlitePool,
    asset_id: i64,
) -> anyhow::Result<Vec<TemplateAssetSourceRow>> {
    Ok(sqlx::query_as::<_, TemplateAssetSourceRow>(
        "SELECT asset_id, document_id, page, bbox_pts
         FROM template_asset_source WHERE asset_id = ?1
         ORDER BY document_id, page",
    )
    .bind(asset_id)
    .fetch_all(pool)
    .await?)
}

#[tauri::command]
pub async fn list_template_assets(
    state: State<'_, AppState>,
    document_id: i64,
) -> Result<Option<TemplateExtractionRow>, String> {
    get_extraction(&state.db.pool, document_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn find_template_assets(
    state: State<'_, AppState>,
    workspace_id: Option<i64>,
    kind: Option<String>,
    query: Option<String>,
    document_id: Option<i64>,
    limit: Option<i64>,
) -> Result<Vec<TemplateAssetRow>, String> {
    let ws = workspace_id.unwrap_or(1);
    find_assets(
        &state.db.pool,
        ws,
        kind.as_deref(),
        query.as_deref(),
        document_id,
        limit.unwrap_or(50),
    )
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn request_template_extraction(
    app: AppHandle,
    state: State<'_, AppState>,
    document_id: i64,
) -> Result<(), String> {
    schedule_extraction(app, state.db.pool.clone(), document_id).await;
    Ok(())
}
