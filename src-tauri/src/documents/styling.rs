//! Visual styling analysis — v0.14.0 Slice 4.
//!
//! Sends a document's PDF bytes to a vision-capable LLM (Claude) with
//! a prompt that asks for structured styling features: header colors,
//! fonts, table layouts, signature placement, margin estimates. The
//! result is cached on `document.styling_json` so subsequent code
//! generations against the same sample template don't re-pay the
//! vision call.
//!
//! Reuses the `extract_pdf` provider method from v0.12 (which already
//! handles Claude's native PDF document block) — only the prompt
//! differs.

use std::path::Path;

use sqlx::SqlitePool;

use crate::llm;
use crate::secrets;

use super::db as docs_db;
use super::storage;

/// The vision prompt that asks Claude to return structured styling.
/// Matches the schema in the v0.14 spec — the LLM-written Python in
/// `run_python` reads this JSON to drive reportlab generation.
const STYLING_PROMPT: &str = "\
You are looking at a sample document. Analyze its visual styling and \
return ONLY valid JSON matching this shape. Use null when something is \
not visible or doesn't apply.\n\
\n\
{\n\
  \"header_color\": \"#RRGGBB\",            // background colour of the table header row\n\
  \"header_text_color\": \"#RRGGBB\",       // text colour inside the header row\n\
  \"body_font_family\": string?,           // best guess: 'Arial', 'Helvetica', 'Times New Roman', etc.\n\
  \"body_font_size_estimate\": number?,    // approx point size of body text (8-14 typical)\n\
  \"table_header_color\": \"#RRGGBB\"?,     // distinct from header_color when there's a separate body header\n\
  \"table_alt_row_color\": \"#RRGGBB\"?,    // zebra-stripe colour if present, null otherwise\n\
  \"border_color\": \"#RRGGBB\"?,           // grid line colour (e.g. '#000000' for black, '#CCCCCC' for grey)\n\
  \"border_weight_estimate\": number?,     // approx point width\n\
  \"font_weight_for_header\": \"bold\" | \"normal\" | null,\n\
  \"signature_column_present\": boolean,\n\
  \"signature_stroke_type\": \"diagonal\" | \"horizontal\" | \"none\" | null,\n\
  \"key_layout_features\": [string],       // 3-6 short observations like 'portrait letter', 'tight 0.3in margins', '7-column table', 'totals row spans 5 columns'\n\
  \"column_widths_relative\": [number]?,    // table column widths as proportions summing to ~1.0\n\
  \"brand_logo_position\": \"top_left\" | \"top_right\" | \"top_center\" | \"none\" | null,\n\
  \"page_orientation\": \"portrait\" | \"landscape\",\n\
  \"page_size\": \"letter\" | \"legal\" | \"A4\" | \"other\" | null,\n\
  \"approximate_margins_inches\": { \"top\": number, \"bottom\": number, \"left\": number, \"right\": number }?,\n\
  \"distinctive_styling_notes\": [string]   // up to 3 short notes a developer would need to faithfully reproduce this layout in reportlab\n\
}\n\
\n\
Be precise on the colours — sample them from the actual page rather than guessing brand colours. \
If you can see only a portion of the document clearly, infer the rest conservatively and note your \
uncertainty in distinctive_styling_notes.";

/// Analyse the styling of a document. Reads bytes from managed
/// storage, sends to the LLM's vision-PDF surface, parses + caches
/// the result.
pub async fn analyze_styling(
    pool: &SqlitePool,
    http: reqwest::Client,
    storage_root: &Path,
    document_id: i64,
    force: bool,
) -> anyhow::Result<serde_json::Value> {
    let doc = docs_db::get(pool, document_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("document {document_id} not found"))?;

    // Cache hit?
    if !force {
        if let Some(json) = doc.extracted_json.as_deref() {
            // The styling_json column is what we want — but the Document
            // struct doesn't expose it. Query directly.
            let _ = json; // silence unused
        }
        let cached: Option<(Option<String>,)> =
            sqlx::query_as("SELECT styling_json FROM document WHERE id = ?1")
                .bind(document_id)
                .fetch_optional(pool)
                .await?;
        if let Some((Some(json),)) = cached {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&json) {
                return Ok(v);
            }
        }
    }

    let abs = storage::absolute_path(storage_root, Path::new(&doc.relative_path));
    let bytes = tokio::fs::read(&abs).await?;
    if bytes.len() > 30 * 1024 * 1024 {
        anyhow::bail!(
            "PDF is {}MB; styling analysis cap is 30MB",
            bytes.len() / 1_048_576
        );
    }

    // Load provider via user_profile
    let row: Option<(String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT llm_provider, ollama_url, model FROM user_profile WHERE id = 1",
    )
    .fetch_optional(pool)
    .await?;
    let (llm_provider, ollama_url, model) = row
        .ok_or_else(|| anyhow::anyhow!("user_profile row missing"))?;
    let api_key = secrets::get_api_key(&llm_provider);
    let provider = llm::build(
        &llm_provider,
        api_key.as_deref(),
        ollama_url.as_deref(),
        model.as_deref(),
        http,
    )?;

    let raw = provider
        .extract_pdf(&bytes, STYLING_PROMPT, Some(1500))
        .await?;
    let trimmed = raw.trim();
    let payload = trimmed
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    let value: serde_json::Value = serde_json::from_str(payload).map_err(|e| {
        anyhow::anyhow!("could not parse styling JSON: {e} (got: {trimmed})")
    })?;

    // Cache
    let json_str = serde_json::to_string(&value)?;
    let _ = sqlx::query(
        "UPDATE document
         SET styling_json = ?1, styling_analyzed_at = CURRENT_TIMESTAMP
         WHERE id = ?2",
    )
    .bind(&json_str)
    .bind(document_id)
    .execute(pool)
    .await;

    Ok(value)
}
