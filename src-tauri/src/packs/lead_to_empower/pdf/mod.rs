//! PDF invoice export.
//!
//! Two shapes:
//!
//! - **Legacy single-line** (after-school enrichment): one rate, one coach,
//!   line items materialised from `coach_hours`. Uses `UserProfile` for the
//!   FROM block. Kept intact for backwards compatibility.
//! - **Multi-line program-delivery** (LTE_INVOICING_SPEC §8.3): multiple
//!   catalog modules per invoice, each priced per `engagement_module`,
//!   line items materialised from `invoice_line`. Uses `company_profile`
//!   for the LTE-letterhead FROM block — parameterised branding so a
//!   sibling consultancy can swap the row and reuse the template.
//!
//! `export_invoice` picks the path automatically based on whether the
//! invoice has any `invoice_line` rows.

use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use printpdf::{BuiltinFont, IndirectFontRef, Mm, PdfDocument, PdfDocumentReference, PdfLayerReference};
use sqlx::SqlitePool;

use crate::db::UserProfile;
use crate::domain::{coach_hours, invoice};
use invoice::Invoice;

// --- Page geometry (A4 portrait) ---
const PAGE_W_MM: f32 = 210.0;
const PAGE_H_MM: f32 = 297.0;
const MARGIN_MM: f32 = 18.0;

// --- Money helper -----------------------------------------------------------

pub fn fmt_money(cents: i64) -> String {
    let neg = cents < 0;
    let abs = cents.unsigned_abs() as u128;
    let dollars = abs / 100;
    let frac = abs % 100;

    // Group thousands with commas.
    let dollars_str = {
        let raw = dollars.to_string();
        let bytes = raw.as_bytes();
        let mut out = String::with_capacity(raw.len() + raw.len() / 3);
        let len = bytes.len();
        for (i, b) in bytes.iter().enumerate() {
            if i > 0 && (len - i) % 3 == 0 {
                out.push(',');
            }
            out.push(*b as char);
        }
        out
    };

    if neg {
        format!("-${}.{:02}", dollars_str, frac)
    } else {
        format!("${}.{:02}", dollars_str, frac)
    }
}

// --- Lookup helpers ---------------------------------------------------------

async fn coach_name(pool: &SqlitePool, coach_id: Option<i64>) -> Result<Option<String>> {
    let Some(id) = coach_id else { return Ok(None) };
    let row: Option<(String,)> = sqlx::query_as("SELECT name FROM coach WHERE id = ?1")
        .bind(id)
        .fetch_optional(pool)
        .await
        .context("query coach name")?;
    Ok(row.map(|r| r.0))
}

async fn school_name(pool: &SqlitePool, school_id: Option<i64>) -> Result<Option<String>> {
    let Some(id) = school_id else { return Ok(None) };
    let row: Option<(String,)> = sqlx::query_as("SELECT name FROM school WHERE id = ?1")
        .bind(id)
        .fetch_optional(pool)
        .await
        .context("query school name")?;
    Ok(row.map(|r| r.0))
}

// --- Multi-line invoice (LTE letterhead) data loaders ----------------------

#[derive(sqlx::FromRow, Default)]
struct CompanyProfile {
    name: Option<String>,
    legal_name: Option<String>,
    address_line_1: Option<String>,
    address_line_2: Option<String>,
    city: Option<String>,
    state: Option<String>,
    zip: Option<String>,
    phone: Option<String>,
    email: Option<String>,
    website: Option<String>,
    ein: Option<String>,
    nyc_doe_vendor_number: Option<String>,
    default_contract_ref: Option<String>,
    tagline: Option<String>,
    default_invoice_signature_authority: Option<String>,
}

async fn load_company_profile(pool: &SqlitePool, workspace_id: i64) -> Result<CompanyProfile> {
    let row: Option<CompanyProfile> = sqlx::query_as(
        "SELECT name, legal_name, address_line_1, address_line_2, city, state, zip,
                phone, email, website, ein, nyc_doe_vendor_number, default_contract_ref,
                tagline, default_invoice_signature_authority
         FROM company_profile WHERE workspace_id = ?1 LIMIT 1",
    )
    .bind(workspace_id)
    .fetch_optional(pool)
    .await
    .context("load company_profile")?;
    Ok(row.unwrap_or_default())
}

#[derive(sqlx::FromRow)]
struct InvoiceLineRow {
    description: String,
    qty: f64,
    unit_price_cents: i64,
    subtotal_cents: i64,
    date_list: Option<String>,
}

async fn load_invoice_lines(pool: &SqlitePool, invoice_id: i64) -> Result<Vec<InvoiceLineRow>> {
    let rows: Vec<InvoiceLineRow> = sqlx::query_as(
        "SELECT description, qty, unit_price_cents, subtotal_cents, date_list
         FROM invoice_line
         WHERE invoice_id = ?1
         ORDER BY sort_order ASC, id ASC",
    )
    .bind(invoice_id)
    .fetch_all(pool)
    .await
    .context("load invoice_line rows")?;
    Ok(rows)
}

async fn load_po_number(pool: &SqlitePool, po_id: Option<i64>) -> Result<Option<String>> {
    let Some(id) = po_id else { return Ok(None) };
    let row: Option<(String,)> =
        sqlx::query_as("SELECT po_number FROM purchase_order WHERE id = ?1")
            .bind(id)
            .fetch_optional(pool)
            .await
            .context("load po_number")?;
    Ok(row.map(|r| r.0))
}

async fn invoice_line_count(pool: &SqlitePool, invoice_id: i64) -> Result<i64> {
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM invoice_line WHERE invoice_id = ?1",
    )
    .bind(invoice_id)
    .fetch_one(pool)
    .await
    .context("count invoice_line rows")?;
    Ok(n)
}

// --- Drawing primitives -----------------------------------------------------

/// Cursor that tracks "current Y from top of page in mm" so callers can write
/// in a top-down narrative without juggling printpdf's bottom-origin coords.
struct Layout {
    layer: PdfLayerReference,
    bold: IndirectFontRef,
    regular: IndirectFontRef,
    y_from_top_mm: f32,
}

impl Layout {
    fn y_pdf(&self) -> Mm {
        // printpdf origin is bottom-left.
        Mm(PAGE_H_MM - self.y_from_top_mm)
    }

    fn advance(&mut self, mm: f32) {
        self.y_from_top_mm += mm;
    }

    fn text(&self, s: &str, size: f32, x_mm: f32, font: &IndirectFontRef) {
        self.layer
            .use_text(s, size, Mm(x_mm), self.y_pdf(), font);
    }

    fn text_regular(&self, s: &str, size: f32, x_mm: f32) {
        self.text(s, size, x_mm, &self.regular);
    }

    fn text_bold(&self, s: &str, size: f32, x_mm: f32) {
        self.text(s, size, x_mm, &self.bold);
    }
}

// --- Public entry point -----------------------------------------------------

pub async fn export_invoice(
    pool: &SqlitePool,
    invoice_id: i64,
    dest_path: &Path,
    profile: &UserProfile,
) -> Result<PathBuf> {
    // Fetch invoice.
    let inv = invoice::fetch_one(pool, invoice_id)
        .await
        .map_err(|e| anyhow!("fetch invoice {invoice_id}: {e}"))?;

    // Program-delivery invoices (invoice_line rows present) get the new
    // multi-line LTE-letterhead renderer; legacy single-line/single-coach
    // invoices (after-school enrichment) keep the original layout below.
    let n_lines = invoice_line_count(pool, invoice_id).await?;
    if n_lines > 0 {
        return render_multi_line_invoice(pool, &inv, dest_path).await;
    }

    // Fetch line items (coach_hours within period).
    let filter = coach_hours::CoachHoursFilter {
        coach_id: inv.coach_id,
        school_id: inv.school_id,
        period_start: Some(inv.period_start.clone()),
        period_end: Some(inv.period_end.clone()),
    };
    // Scope to the invoice's workspace — coach_hours rows must come
     // from the same world the invoice lives in.
     let mut items = coach_hours::list(pool, &[inv.workspace_id], filter)
        .await
        .map_err(|e| anyhow!("list coach_hours: {e}"))?;
    // Render chronologically (list() returns DESC).
    items.sort_by(|a, b| a.session_date.cmp(&b.session_date));

    let coach = coach_name(pool, inv.coach_id).await?;
    let school = school_name(pool, inv.school_id).await?;

    // Footer timestamp from SQLite (await must happen before non-Send doc exists).
    let footer_ts: String = sqlx::query_scalar("SELECT strftime('%Y-%m-%d %H:%M UTC', 'now')")
        .fetch_one(pool)
        .await
        .unwrap_or_else(|_| "now".to_string());

    // From this point on: NO awaits. printpdf's PdfDocumentReference is Rc<RefCell<...>>
    // which is !Send, so anything holding it cannot span an await — Tauri requires
    // command futures to be Send.

    // Build the document.
    let (doc, page1, layer1) =
        PdfDocument::new("Invoice", Mm(PAGE_W_MM), Mm(PAGE_H_MM), "Layer 1");
    let layer = doc.get_page(page1).get_layer(layer1);
    let regular: IndirectFontRef = doc
        .add_builtin_font(BuiltinFont::Helvetica)
        .map_err(|e| anyhow!("add builtin Helvetica: {e}"))?;
    let bold: IndirectFontRef = doc
        .add_builtin_font(BuiltinFont::HelveticaBold)
        .map_err(|e| anyhow!("add builtin Helvetica-Bold: {e}"))?;

    let mut layout = Layout {
        layer,
        bold,
        regular,
        y_from_top_mm: MARGIN_MM,
    };

    // -------- Header: top-left wordmark, top-right meta --------
    layout.text_bold("[ LOGO ]", 11.0, MARGIN_MM);
    layout.advance(8.0);
    layout.text_bold("INVOICE", 26.0, MARGIN_MM);

    // Meta block on the right (use absolute Y so we don't disturb layout cursor).
    let right_x = PAGE_W_MM - MARGIN_MM - 70.0;
    let mut meta_y = MARGIN_MM + 4.0;
    let issued = inv
        .issued_at
        .clone()
        .unwrap_or_else(|| inv.created_at.clone());
    let period_str = format!("{} -> {}", inv.period_start, inv.period_end);
    let meta_rows: [(&str, &str); 4] = [
        ("Number:", inv.number.as_str()),
        ("Issued:", issued.as_str()),
        ("Period:", period_str.as_str()),
        ("Status:", inv.status.as_str()),
    ];
    for (k, v) in meta_rows.iter() {
        layout
            .layer
            .use_text(*k, 10.0, Mm(right_x), Mm(PAGE_H_MM - meta_y), &layout.bold);
        layout.layer.use_text(
            *v,
            10.0,
            Mm(right_x + 28.0),
            Mm(PAGE_H_MM - meta_y),
            &layout.regular,
        );
        meta_y += 5.5;
    }

    // Make sure we sit below whichever block is taller.
    layout.advance(14.0);
    if layout.y_from_top_mm < meta_y + 4.0 {
        layout.y_from_top_mm = meta_y + 4.0;
    }

    // -------- Bill to / From blocks --------
    let bill_to_x = MARGIN_MM;
    let from_x = MARGIN_MM + 95.0;
    let block_top = layout.y_from_top_mm;

    layout.text_bold("BILL TO", 9.0, bill_to_x);
    layout.text_bold("FROM", 9.0, from_x);
    layout.advance(5.5);

    layout.text_regular(&inv.recipient, 11.0, bill_to_x);
    layout.text_regular(&profile.org, 11.0, from_x);
    layout.advance(5.0);

    if let Some(s) = &school {
        layout.text_regular(&format!("School: {s}"), 9.5, bill_to_x);
    }
    layout.text_regular(&profile.role, 9.5, from_x);
    layout.advance(4.5);

    if let Some(c) = &coach {
        layout.text_regular(&format!("Coach: {c}"), 9.5, bill_to_x);
    }
    layout.text_regular(&profile.name, 9.5, from_x);
    layout.advance(8.0);

    // Pad in case the longer side dictates.
    if layout.y_from_top_mm < block_top + 26.0 {
        layout.y_from_top_mm = block_top + 26.0;
    }

    // -------- Line items table --------
    // Columns: Date | Description | Hours | Rate | Line Total
    let col_date = MARGIN_MM;
    let col_desc = MARGIN_MM + 24.0;
    let col_hours = MARGIN_MM + 110.0;
    let col_rate = MARGIN_MM + 132.0;
    let col_total = MARGIN_MM + 158.0;

    layout.text_bold("DATE", 9.0, col_date);
    layout.text_bold("DESCRIPTION", 9.0, col_desc);
    layout.text_bold("HOURS", 9.0, col_hours);
    layout.text_bold("RATE", 9.0, col_rate);
    layout.text_bold("AMOUNT", 9.0, col_total);
    layout.advance(5.5);

    // A subtle ruler row (drawn as text underscore — keeps us off the shapes API
    // which differs across printpdf 0.7 minors).
    layout.text_regular(
        "________________________________________________________________________________________",
        7.0,
        col_date,
    );
    layout.advance(3.0);

    let rate_cents = inv.rate_cents;

    if items.is_empty() {
        // Single synthetic row from the invoice header.
        layout.text_regular(&inv.period_start, 9.5, col_date);
        layout.text_regular(
            &truncate(&format!("Coaching services {}", coach.as_deref().unwrap_or("")), 60),
            9.5,
            col_desc,
        );
        layout.text_regular(&format!("{:.2}", inv.hours_total), 9.5, col_hours);
        layout.text_regular(&fmt_money(rate_cents), 9.5, col_rate);
        layout.text_regular(&fmt_money(inv.amount_cents), 9.5, col_total);
        layout.advance(5.0);
    } else {
        for it in &items {
            let line_total =
                ((it.hours * rate_cents as f64).round()) as i64;
            let desc = it
                .description
                .clone()
                .unwrap_or_else(|| "Coaching session".to_string());
            layout.text_regular(&it.session_date, 9.5, col_date);
            layout.text_regular(&truncate(&desc, 60), 9.5, col_desc);
            layout.text_regular(&format!("{:.2}", it.hours), 9.5, col_hours);
            layout.text_regular(&fmt_money(rate_cents), 9.5, col_rate);
            layout.text_regular(&fmt_money(line_total), 9.5, col_total);
            layout.advance(5.0);

            // Stop drawing rows if we'd overflow the page; the rest still
            // contributes to the total summary below.
            if layout.y_from_top_mm > PAGE_H_MM - 60.0 {
                layout.text_regular("(additional rows truncated)", 8.0, col_desc);
                layout.advance(5.0);
                break;
            }
        }
    }

    // Total bar.
    layout.advance(3.0);
    layout.text_regular(
        "________________________________________________________________________________________",
        7.0,
        col_date,
    );
    layout.advance(5.0);

    layout.text_bold("TOTAL", 11.0, col_hours - 16.0);
    layout.text_regular(&format!("{:.2} h", inv.hours_total), 10.0, col_hours);
    layout.text_regular(&fmt_money(rate_cents), 10.0, col_rate);
    layout.text_bold(&fmt_money(inv.amount_cents), 11.0, col_total);
    layout.advance(10.0);

    // Notes.
    if let Some(notes) = &inv.notes {
        if !notes.trim().is_empty() {
            layout.text_bold("NOTES", 9.0, MARGIN_MM);
            layout.advance(4.5);
            for line in wrap(notes, 95) {
                layout.text_regular(&line, 9.0, MARGIN_MM);
                layout.advance(4.2);
                if layout.y_from_top_mm > PAGE_H_MM - 25.0 {
                    break;
                }
            }
        }
    }

    // Footer (absolute position near bottom).
    let footer = format!("Generated by Travis on {}", footer_ts);
    layout.layer.use_text(
        footer,
        8.0,
        Mm(MARGIN_MM),
        Mm(MARGIN_MM * 0.55),
        &layout.regular,
    );

    // -------- Persist --------
    if let Some(parent) = dest_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create parent dir {}", parent.display()))?;
        }
    }
    write_doc(doc, dest_path)?;
    Ok(dest_path.to_path_buf())
}

fn write_doc(doc: PdfDocumentReference, dest: &Path) -> Result<()> {
    let file = File::create(dest)
        .with_context(|| format!("create pdf file {}", dest.display()))?;
    let mut bw = BufWriter::new(file);
    doc.save(&mut bw)
        .map_err(|e| anyhow!("save pdf: {e}"))?;
    Ok(())
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(n.saturating_sub(3)).collect();
        out.push_str("...");
        out
    }
}

// ---------------------------------------------------------------------------
// Multi-line program-delivery invoice renderer (LTE_INVOICING_SPEC §8.3).
//
// Layout follows the LTE2064981 sample Taylor sent: teal title row,
// FROM/TO blocks, line-item table with embedded date-list per row,
// total row, PO# stamp, principal signature block. Branding comes from
// `company_profile` so a sibling consultancy swaps the row and reuses
// the template — no hardcoded "LEAD TO EMPOWER" strings.
// ---------------------------------------------------------------------------

async fn render_multi_line_invoice(
    pool: &SqlitePool,
    inv: &Invoice,
    dest_path: &Path,
) -> Result<PathBuf> {
    // ----- await-phase data loads -----
    let cp = load_company_profile(pool, inv.workspace_id).await?;
    let lines = load_invoice_lines(pool, inv.id).await?;
    let school = school_name(pool, inv.school_id).await?;
    let po_number = load_po_number(pool, inv.purchase_order_id).await?;
    let footer_ts: String = sqlx::query_scalar("SELECT strftime('%Y-%m-%d %H:%M UTC', 'now')")
        .fetch_one(pool)
        .await
        .unwrap_or_else(|_| "now".to_string());

    // No awaits past this point — printpdf's doc handle is !Send.
    let (doc, page1, layer1) =
        PdfDocument::new("Invoice", Mm(PAGE_W_MM), Mm(PAGE_H_MM), "Layer 1");
    let layer = doc.get_page(page1).get_layer(layer1);
    let regular: IndirectFontRef = doc
        .add_builtin_font(BuiltinFont::Helvetica)
        .map_err(|e| anyhow!("add Helvetica: {e}"))?;
    let bold: IndirectFontRef = doc
        .add_builtin_font(BuiltinFont::HelveticaBold)
        .map_err(|e| anyhow!("add Helvetica-Bold: {e}"))?;

    let mut layout = Layout {
        layer,
        bold,
        regular,
        y_from_top_mm: MARGIN_MM,
    };

    // ----- Header: company wordmark + tagline (LTE branding) -----
    let company_label = cp.name.as_deref().unwrap_or("Lead to Empower").to_uppercase();
    layout.text_bold(&company_label, 22.0, MARGIN_MM);
    layout.advance(8.0);
    if let Some(tag) = cp.tagline.as_deref() {
        layout.text_regular(tag, 9.0, MARGIN_MM);
        layout.advance(5.0);
    }
    // Company address strip
    let addr_strip = compose_address_strip(&cp);
    if !addr_strip.is_empty() {
        layout.text_regular(&addr_strip, 8.5, MARGIN_MM);
        layout.advance(4.5);
    }
    if let Some(web) = cp.website.as_deref() {
        layout.text_regular(web, 8.5, MARGIN_MM);
        layout.advance(8.0);
    } else {
        layout.advance(4.0);
    }

    // "INVOICE" title on the right + invoice #
    let right_x = PAGE_W_MM - MARGIN_MM - 55.0;
    let mut meta_y = MARGIN_MM + 4.0;
    layout
        .layer
        .use_text("INVOICE", 24.0, Mm(right_x), Mm(PAGE_H_MM - meta_y), &layout.bold);
    meta_y += 9.0;
    layout
        .layer
        .use_text("Invoice #:", 9.5, Mm(right_x), Mm(PAGE_H_MM - meta_y), &layout.bold);
    layout.layer.use_text(
        inv.number.as_str(),
        9.5,
        Mm(right_x + 22.0),
        Mm(PAGE_H_MM - meta_y),
        &layout.regular,
    );

    // ----- FROM block (company) -----
    let from_top = layout.y_from_top_mm.max(meta_y + 8.0);
    layout.y_from_top_mm = from_top;
    layout.text_bold("From:", 10.0, MARGIN_MM);
    layout.advance(5.0);
    layout.text_bold(cp.name.as_deref().unwrap_or(""), 11.0, MARGIN_MM);
    layout.advance(4.5);
    if let Some(t) = cp.tagline.as_deref() {
        layout.text_regular(t, 8.5, MARGIN_MM);
        layout.advance(4.5);
    }
    for line in compose_company_block(&cp) {
        layout.text_regular(&line, 9.0, MARGIN_MM);
        layout.advance(4.2);
    }
    let from_bottom = layout.y_from_top_mm;

    // ----- TO block (school recipient) — placed below FROM -----
    layout.advance(6.0);
    layout.text_bold("To:", 10.0, MARGIN_MM);
    layout.advance(5.0);
    let recipient_label = school.as_deref().unwrap_or(inv.recipient.as_str());
    layout.text_regular(recipient_label, 10.5, MARGIN_MM);
    layout.advance(8.0);
    let _ = from_bottom; // reserved for a future side-by-side layout

    // ----- Line items table -----
    let col_unit = MARGIN_MM;
    let col_qty = MARGIN_MM + 18.0;
    let col_desc = MARGIN_MM + 32.0;
    let col_price = MARGIN_MM + 130.0;
    let col_total = MARGIN_MM + 162.0;

    layout.text_bold("UNIT", 9.0, col_unit);
    layout.text_bold("QTY", 9.0, col_qty);
    layout.text_bold("DESCRIPTION", 9.0, col_desc);
    layout.text_bold("UNIT PRICE", 9.0, col_price);
    layout.text_bold("TOTAL", 9.0, col_total);
    layout.advance(5.5);
    layout.text_regular(
        "________________________________________________________________________________________",
        7.0,
        col_unit,
    );
    layout.advance(3.5);

    let mut running_total: i64 = 0;
    for line in &lines {
        layout.text_regular("1", 9.5, col_unit);
        layout.text_regular(&format_qty(line.qty), 9.5, col_qty);
        layout.text_regular(&truncate(&line.description, 50), 9.5, col_desc);
        layout.text_regular(&fmt_money(line.unit_price_cents), 9.5, col_price);
        layout.text_regular(&fmt_money(line.subtotal_cents), 9.5, col_total);
        layout.advance(4.8);

        // Date list (e.g. "Jan: 29 Feb: 24 Mar: 6, 18 Apr: 17, 24")
        // wraps under the description column in smaller text.
        if let Some(dl) = line.date_list.as_deref() {
            if !dl.trim().is_empty() {
                for wrapped in wrap(dl, 60) {
                    layout.text_regular(&wrapped, 8.0, col_desc);
                    layout.advance(3.8);
                }
            }
        }
        layout.advance(2.0);
        running_total += line.subtotal_cents;

        if layout.y_from_top_mm > PAGE_H_MM - 60.0 {
            layout.text_regular("(additional rows truncated)", 8.0, col_desc);
            layout.advance(5.0);
            break;
        }
    }

    // ----- Total row -----
    layout.advance(2.0);
    layout.text_regular(
        "________________________________________________________________________________________",
        7.0,
        col_unit,
    );
    layout.advance(5.0);
    layout.text_bold("TOTAL", 11.0, col_price);
    let total_display = if running_total > 0 { running_total } else { inv.amount_cents };
    layout.text_bold(&fmt_money(total_display), 11.0, col_total);
    layout.advance(10.0);

    // ----- Footer instructions (verbatim from LTE template) -----
    layout.text_regular("1. Please send two copies of your invoice.", 8.5, MARGIN_MM);
    layout.advance(4.0);
    layout.text_regular(
        "2. Enter this order in accordance with the prices, terms, and specifications listed above.",
        8.5,
        MARGIN_MM,
    );
    layout.advance(10.0);

    // ----- PO stamp (bottom-left) + signature block (bottom-right) -----
    let sig_y = (PAGE_H_MM - MARGIN_MM - 25.0).max(layout.y_from_top_mm + 8.0);
    let sig_row_y = Mm(PAGE_H_MM - sig_y);
    if let Some(po) = po_number.as_deref() {
        layout
            .layer
            .use_text(po, 10.0, Mm(MARGIN_MM), sig_row_y, &layout.bold);
    }
    // Authorized by ___ Date ___ on the right
    let auth_x = PAGE_W_MM - MARGIN_MM - 75.0;
    layout.layer.use_text(
        "Authorized by",
        9.5,
        Mm(auth_x),
        sig_row_y,
        &layout.regular,
    );
    layout.layer.use_text(
        "Date",
        9.5,
        Mm(auth_x + 50.0),
        sig_row_y,
        &layout.regular,
    );
    // Pre-fill the signature authority + signed date if we have them.
    let sig_name_y = Mm(PAGE_H_MM - sig_y + 5.0);
    if let Some(name) = inv
        .school_signed_by_name
        .as_deref()
        .or(cp.default_invoice_signature_authority.as_deref())
    {
        layout
            .layer
            .use_text(name, 9.0, Mm(auth_x), sig_name_y, &layout.bold);
    }
    if let Some(d) = inv.school_signed_at.as_deref() {
        layout
            .layer
            .use_text(d, 9.0, Mm(auth_x + 50.0), sig_name_y, &layout.bold);
    }

    // ----- Generated-by footer -----
    let footer = format!("Generated by Travis on {}", footer_ts);
    layout.layer.use_text(
        footer,
        7.5,
        Mm(MARGIN_MM),
        Mm(MARGIN_MM * 0.55),
        &layout.regular,
    );

    // Persist
    if let Some(parent) = dest_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create parent dir {}", parent.display()))?;
        }
    }
    write_doc(doc, dest_path)?;
    Ok(dest_path.to_path_buf())
}

fn compose_address_strip(cp: &CompanyProfile) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(a) = cp.address_line_1.as_deref() {
        parts.push(a.to_string());
    }
    if let Some(city) = cp.city.as_deref() {
        let mut tail = city.to_string();
        if let Some(s) = cp.state.as_deref() {
            tail.push_str(", ");
            tail.push_str(s);
        }
        if let Some(z) = cp.zip.as_deref() {
            tail.push(' ');
            tail.push_str(z);
        }
        parts.push(tail);
    }
    parts.join("  ")
}

fn compose_company_block(cp: &CompanyProfile) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Some(a) = cp.address_line_1.as_deref() {
        out.push(a.to_string());
    }
    if let Some(a2) = cp.address_line_2.as_deref() {
        if !a2.is_empty() {
            out.push(a2.to_string());
        }
    }
    let mut citystate = String::new();
    if let Some(c) = cp.city.as_deref() {
        citystate.push_str(c);
    }
    if let Some(s) = cp.state.as_deref() {
        if !citystate.is_empty() {
            citystate.push_str(", ");
        }
        citystate.push_str(s);
    }
    if let Some(z) = cp.zip.as_deref() {
        if !citystate.is_empty() {
            citystate.push(' ');
        }
        citystate.push_str(z);
    }
    if !citystate.is_empty() {
        out.push(citystate);
    }
    if let Some(p) = cp.phone.as_deref() {
        out.push(format!("Phone: {p}"));
    }
    if let Some(v) = cp.nyc_doe_vendor_number.as_deref() {
        out.push(format!("VENDOR # {v}"));
    }
    if let Some(c) = cp.default_contract_ref.as_deref() {
        out.push(format!("Contract #: {c}"));
    }
    if let Some(e) = cp.ein.as_deref() {
        out.push(format!("EIN {e}"));
    }
    let _ = cp.email.as_deref();
    let _ = cp.legal_name.as_deref();
    out
}

fn format_qty(qty: f64) -> String {
    if (qty - qty.round()).abs() < 0.001 {
        format!("{}", qty as i64)
    } else {
        format!("{qty}")
    }
}

fn wrap(s: &str, width: usize) -> Vec<String> {
    let mut out = Vec::new();
    for paragraph in s.split('\n') {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            if line.is_empty() {
                line.push_str(word);
            } else if line.len() + 1 + word.len() <= width {
                line.push(' ');
                line.push_str(word);
            } else {
                out.push(std::mem::take(&mut line));
                line.push_str(word);
            }
        }
        out.push(line);
    }
    out
}
