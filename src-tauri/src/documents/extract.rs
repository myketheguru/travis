//! Document extraction — PDF → structured data.
//!
//! Two-stage pipeline:
//!   1. **Text layer** — `pdf-extract` pulls embedded text from
//!      digitally-generated PDFs (POs out of Polaris, Travis-generated
//!      LTE letterhead invoices, most contracts). Pure Rust, no native
//!      deps, ~50KB. Covers the dominant case for Taylor's workflow.
//!   2. **Vision fallback** — for scanned PDFs (signed sheets that came
//!      back from a fax/scanner) we'd render the page and send the
//!      image to a vision-capable LLM. NOT YET WIRED in this slice;
//!      tracked in WORKFLOWS_BACKLOG.md. For now the extractor flags
//!      these as `extraction_error: "text layer empty; vision not yet
//!      wired"` so the document is still ingested + linkable, just
//!      lacking auto-extraction.
//!
//! Once we have raw text, an LLM call structures it into a
//! kind-specific JSON shape — the shape lives next to each document
//! kind so packs can define their own.
//!
//! See [[feedback-docs-first]] and [[feedback-workflow-led]] for context.

use std::path::Path;

use crate::llm;
use crate::secrets;

use super::db::{self, IngestStatus};
use super::storage;

/// Extract text from a PDF on disk. Returns `Ok(None)` when the file
/// has no text layer (scanned image PDF) — the caller decides whether
/// to fall back to vision or mark the document as needing manual
/// labelling. Errors for unreadable / corrupt files.
pub fn extract_text(path: &Path) -> anyhow::Result<Option<String>> {
    let bytes = std::fs::read(path)?;
    let text = pdf_extract::extract_text_from_mem(&bytes)
        .map_err(|e| anyhow::anyhow!("pdf-extract failed: {e}"))?;
    let cleaned = text.trim();
    if cleaned.is_empty() {
        Ok(None)
    } else {
        Ok(Some(cleaned.to_string()))
    }
}

/// Run extraction on one document by id. Reads the file from managed
/// storage, picks the right extraction prompt for its `kind`, calls
/// the LLM in JSON mode, and updates the document row with the
/// resulting structured JSON.
///
/// Side-effect-free on caller; persists results to the DB. The Tauri
/// command + the fire-and-forget background task on ingest both go
/// through here.
pub async fn run_extraction(
    pool: &sqlx::SqlitePool,
    http: reqwest::Client,
    storage_root: &Path,
    document_id: i64,
) -> anyhow::Result<()> {
    let doc = db::get(pool, document_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("document {document_id} not found"))?;

    // Already done? Skip — caller can force a re-extract via a
    // dedicated path later if needed.
    if doc.ingest_status == "extracted" || doc.ingest_status == "skipped" {
        return Ok(());
    }

    // Generated-by-Travis PDFs don't need extraction — we generated
    // them from structured data already. Mark skipped and move on.
    if doc.source == "generated_by_travis" {
        db::set_extracted(pool, document_id, IngestStatus::Skipped, None, None).await?;
        return Ok(());
    }

    let abs = storage::absolute_path(storage_root, Path::new(&doc.relative_path));
    let prompt = prompt_for_kind(&doc.kind);

    // Stage 1: text-layer extraction (cheap, on-device).
    // Stage 2: when text layer is empty AND the provider supports
    // document vision (Claude does, OpenAI/Ollama don't), send the
    // PDF bytes directly. No PDFium / OCR pipeline needed.
    let json_value = match extract_text(&abs) {
        Ok(Some(text)) => {
            match run_llm_extraction(pool, http.clone(), &text, prompt).await {
                Ok(v) => v,
                Err(e) => {
                    db::set_extracted(
                        pool,
                        document_id,
                        IngestStatus::Failed,
                        None,
                        Some(&format!("llm extraction failed: {e}")),
                    )
                    .await?;
                    return Err(e);
                }
            }
        }
        Ok(None) => {
            // No text layer — likely a scanned PDF. Fall back to
            // provider-side vision document input.
            match run_vision_extraction(pool, http, &abs, prompt).await {
                Ok(v) => v,
                Err(e) => {
                    db::set_extracted(
                        pool,
                        document_id,
                        IngestStatus::Failed,
                        None,
                        Some(&format!("vision extraction failed: {e}")),
                    )
                    .await?;
                    return Ok(());
                }
            }
        }
        Err(e) => {
            db::set_extracted(
                pool,
                document_id,
                IngestStatus::Failed,
                None,
                Some(&format!("pdf read failed: {e}")),
            )
            .await?;
            return Err(e);
        }
    };

    let json_str = serde_json::to_string(&json_value)?;
    db::set_extracted(
        pool,
        document_id,
        IngestStatus::Extracted,
        Some(&json_str),
        None,
    )
    .await?;

    Ok(())
}

/// Per-document-kind extraction prompts. These currently live in core
/// because Taylor is our only user and her doc kinds are LTE-shaped.
/// As other packs onboard, this should move behind a PackHandle method
/// (mirror of `workflows()`). Tracked in WORKFLOWS_BACKLOG.md.
fn prompt_for_kind(kind: &str) -> &'static str {
    match kind {
        "po" | "purchase_order" => PO_PROMPT,
        "wo" | "work_order" => WO_PROMPT,
        "signed_sheet" | "sign_in_sheet" => SHEET_PROMPT,
        "invoice" => INVOICE_PROMPT,
        "contract" => CONTRACT_PROMPT,
        _ => GENERIC_PROMPT,
    }
}

const PO_PROMPT: &str = "\
You are reading a Purchase Order (PO) PDF, typically issued by an NYC public school \
to a vendor like Lead to Empower. Extract structured fields and return ONLY valid JSON \
matching this shape:
{
  \"po_number\": string,        // The PO identifier, e.g. \"WR260363316\"
  \"school_name\": string?,     // Issuing school / school name as written
  \"vendor_name\": string?,     // Vendor / payee name
  \"date_issued\": string?,     // ISO date (YYYY-MM-DD), best guess
  \"period_start\": string?,    // ISO date — earliest covered service date if stated
  \"period_end\": string?,      // ISO date — latest covered service date if stated
  \"line_items\": [             // One row per priced line on the PO
    {
      \"description\": string,
      \"quantity\": number,
      \"unit_price_cents\": integer,   // List price per unit, in cents
      \"total_cents\": integer         // quantity × unit_price, in cents
    }
  ],
  \"total_cents\": integer,     // Grand total in cents
  \"notes\": string?            // Anything else worth recording
}
Be conservative with guesses — leave optional fields null if unsure. Convert all dollar \
amounts to integer cents. If multiple values for the same field appear, pick the one in \
the PO header / totals section.";

const WO_PROMPT: &str = "\
You are reading a Work Order (WO) PDF, typically issued alongside an LTE-shape PO. \
Extract structured fields and return ONLY valid JSON matching this shape:
{
  \"wo_number\": string?,
  \"po_number\": string?,       // The PO this WO covers
  \"school_name\": string?,
  \"vendor_name\": string?,
  \"date_issued\": string?,     // ISO date
  \"period_start\": string?,
  \"period_end\": string?,
  \"description\": string?,     // Plain-text description of work
  \"line_items\": [
    { \"description\": string, \"quantity\": number, \"unit\": string? }
  ],
  \"notes\": string?
}
Leave fields null when not present. Convert dates to ISO format.";

const SHEET_PROMPT: &str = "\
You are reading a signed sign-in / time sheet PDF used to document hours \
delivered by a contractor at a school. Extract structured fields and return ONLY \
valid JSON matching this shape:
{
  \"coach_name\": string?,      // Contractor / facilitator name
  \"school_name\": string?,
  \"period_start\": string?,    // ISO date — earliest entry
  \"period_end\": string?,      // ISO date — latest entry
  \"total_hours\": number?,     // Sum of hours across all rows, if calculable
  \"entries\": [                // One row per delivery date
    {
      \"date\": string,        // ISO date
      \"start_time\": string?,  // \"HH:MM\" 24h if available
      \"end_time\": string?,
      \"hours\": number?
    }
  ],
  \"signer_name\": string?,     // Who signed the sheet (typically principal)
  \"signer_role\": string?,     // e.g. \"principal\"
  \"signed_date\": string?,     // ISO date the sheet was signed
  \"notes\": string?
}
If the sheet appears unsigned, set signer_name and signed_date to null.";

const INVOICE_PROMPT: &str = "\
You are reading an invoice PDF (likely LTE-generated, NYC DoF style). \
Extract structured fields and return ONLY valid JSON matching this shape:
{
  \"invoice_number\": string?,  // e.g. \"LTE2064981\"
  \"vendor_name\": string?,
  \"recipient_name\": string?,  // Bill-to (school name or DoF)
  \"date_issued\": string?,     // ISO date
  \"period_start\": string?,
  \"period_end\": string?,
  \"po_number\": string?,       // Referenced PO
  \"line_items\": [
    {
      \"description\": string,
      \"quantity\": number,
      \"unit_price_cents\": integer,
      \"total_cents\": integer
    }
  ],
  \"subtotal_cents\": integer?,
  \"total_cents\": integer?,
  \"notes\": string?
}";

const CONTRACT_PROMPT: &str = "\
You are reading a contract or master agreement PDF. Extract structured fields \
and return ONLY valid JSON matching this shape:
{
  \"contract_id\": string?,     // Reference number / agreement id
  \"vendor_name\": string?,
  \"counterparty_name\": string?,  // School, district, etc.
  \"effective_date\": string?,    // ISO date
  \"expiration_date\": string?,   // ISO date
  \"total_cents\": integer?,      // Headline contract value if stated
  \"scope_summary\": string?,     // 1-2 sentence summary
  \"notes\": string?
}";

const GENERIC_PROMPT: &str = "\
You are reading a document of unspecified kind. Return ONLY valid JSON with this shape:
{
  \"title\": string?,           // Doc title, header, or filename-sized identifier
  \"summary\": string?,         // 1-3 sentence summary of what this document is
  \"key_fields\": [             // Up to 10 (label, value) pairs worth surfacing
    { \"label\": string, \"value\": string }
  ]
}";

/// Build the LLM call: load user's profile to pick the provider /
/// model, send the kind-specific prompt as system + the extracted text
/// as user, parse JSON.
async fn run_llm_extraction(
    pool: &sqlx::SqlitePool,
    http: reqwest::Client,
    text: &str,
    prompt: &str,
) -> anyhow::Result<serde_json::Value> {
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

    // Cap input text so we don't blow the model's context with a
    // monster PDF. Real LTE docs are 1-3 pages; 16k chars is comfortable.
    let truncated = if text.len() > 16_000 {
        &text[..16_000]
    } else {
        text
    };

    let resp = provider
        .chat(
            vec![llm::Message::user(truncated.to_string())],
            llm::ChatOptions {
                system: Some(prompt.to_string()),
                max_tokens: Some(2_000),
                temperature: Some(0.0),
                cache_system: true,
                json_mode: true,
            },
        )
        .await?;

    // Strip optional code-fence wrappers some providers like to add.
    let raw = resp.content.trim();
    let payload = raw
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    let value: serde_json::Value = serde_json::from_str(payload)
        .map_err(|e| anyhow::anyhow!("could not parse extraction JSON: {e} (got: {raw})"))?;
    Ok(value)
}

/// Vision-fallback extraction for scanned PDFs with no text layer.
/// Sends the raw PDF bytes to the configured LLM provider. Claude
/// supports native document input; other providers return a clear
/// error. The caller surfaces this as `extraction_error` on the
/// document row.
async fn run_vision_extraction(
    pool: &sqlx::SqlitePool,
    http: reqwest::Client,
    pdf_path: &Path,
    prompt: &str,
) -> anyhow::Result<serde_json::Value> {
    let bytes = tokio::fs::read(pdf_path).await?;
    // Anthropic's PDF document block currently caps at ~32MB. Files
    // bigger than that need page-splitting, which we punt for now —
    // Taylor's real docs are 1–3 pages and well under this.
    const MAX_BYTES: usize = 30 * 1024 * 1024;
    if bytes.len() > MAX_BYTES {
        anyhow::bail!(
            "PDF is {}MB; vision-extraction cap is {}MB. Try splitting it.",
            bytes.len() / 1_048_576,
            MAX_BYTES / 1_048_576,
        );
    }

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

    let raw = provider.extract_pdf(&bytes, prompt, Some(2_000)).await?;
    let trimmed = raw.trim();
    let payload = trimmed
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    let value: serde_json::Value = serde_json::from_str(payload).map_err(|e| {
        anyhow::anyhow!("could not parse vision-extracted JSON: {e} (got: {trimmed})")
    })?;
    Ok(value)
}
