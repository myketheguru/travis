//! PDF invoice export.
//!
//! Renders an invoice (header, bill-to / from blocks, line-item table, total,
//! footer) to a single A4 page using printpdf 0.7's built-in Helvetica font
//! (no external font file required).

use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use printpdf::{BuiltinFont, IndirectFontRef, Mm, PdfDocument, PdfDocumentReference, PdfLayerReference};
use sqlx::SqlitePool;

use crate::db::UserProfile;
use crate::domain::{coach_hours, invoice};

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

    // Fetch line items (coach_hours within period).
    let filter = coach_hours::CoachHoursFilter {
        coach_id: inv.coach_id,
        school_id: inv.school_id,
        period_start: Some(inv.period_start.clone()),
        period_end: Some(inv.period_end.clone()),
    };
    let mut items = coach_hours::list(pool, filter)
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
