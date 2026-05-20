use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use super::{coach_hours, signing_sheet, DomainError};
use crate::behavioral;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Invoice {
    pub id: i64,
    pub workspace_id: i64,
    pub number: String,
    pub recipient: String,
    pub coach_id: Option<i64>,
    pub school_id: Option<i64>,
    pub signing_sheet_id: Option<i64>,
    pub period_start: String,
    pub period_end: String,
    pub hours_total: f64,
    pub rate_cents: i64,
    pub amount_cents: i64,
    pub status: String,
    pub issued_at: Option<String>,
    pub paid_at: Option<String>,
    pub notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    // Added in pack migration 0003_invoicing — nullable, present on
    // multi-line program-delivery invoices and used by Slice 2 validators.
    pub engagement_id: Option<i64>,
    pub purchase_order_id: Option<i64>,
    pub school_signed_at: Option<String>,
    pub school_signed_by_name: Option<String>,
    pub submitted_to_polaris_at: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceInput {
    pub id: Option<i64>,
    pub number: String,
    pub recipient: String,
    pub coach_id: Option<i64>,
    pub school_id: Option<i64>,
    pub signing_sheet_id: Option<i64>,
    pub period_start: String,
    pub period_end: String,
    pub hours_total: f64,
    pub rate_cents: i64,
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceFilter {
    pub status: Option<String>,
    pub coach_id: Option<i64>,
}

pub async fn list(
    pool: &SqlitePool,
    workspace_ids: &[i64],
    filter: InvoiceFilter,
) -> Result<Vec<Invoice>, DomainError> {
    let ws_start = 3usize;
    let ws_placeholders = (ws_start..ws_start + workspace_ids.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT id, workspace_id, number, recipient, coach_id, school_id, signing_sheet_id,
                period_start, period_end, hours_total, rate_cents, amount_cents,
                status, issued_at, paid_at, notes, created_at, updated_at,
                engagement_id, purchase_order_id, school_signed_at, school_signed_by_name,
                submitted_to_polaris_at
         FROM invoice
         WHERE (?1 IS NULL OR status = ?1)
           AND (?2 IS NULL OR coach_id = ?2)
           AND workspace_id IN ({ws_placeholders})
         ORDER BY period_end DESC, id DESC"
    );
    let mut q = sqlx::query_as::<_, Invoice>(&sql)
        .bind(&filter.status)
        .bind(filter.coach_id);
    for ws in workspace_ids {
        q = q.bind(ws);
    }
    Ok(q.fetch_all(pool).await?)
}

pub async fn upsert(
    pool: &SqlitePool,
    workspace_id: i64,
    input: InvoiceInput,
) -> Result<Invoice, DomainError> {
    let number = input.number.trim().to_string();
    let recipient = input.recipient.trim().to_string();
    if number.is_empty() {
        return Err(DomainError::invalid("invoice number is required"));
    }
    if recipient.is_empty() {
        return Err(DomainError::invalid("recipient is required"));
    }
    if input.period_start > input.period_end {
        return Err(DomainError::invalid("period_start must be on or before period_end"));
    }
    if input.hours_total < 0.0 {
        return Err(DomainError::invalid("hours_total cannot be negative"));
    }
    if input.rate_cents < 0 {
        return Err(DomainError::invalid("rate_cents cannot be negative"));
    }

    let amount_cents = (input.hours_total * input.rate_cents as f64).round() as i64;

    let was_new = input.id.is_none();
    let id = match input.id {
        Some(id) => {
            sqlx::query(
                "UPDATE invoice SET number=?1, recipient=?2, coach_id=?3, school_id=?4,
                    signing_sheet_id=?5, period_start=?6, period_end=?7, hours_total=?8,
                    rate_cents=?9, amount_cents=?10, notes=?11, updated_at=CURRENT_TIMESTAMP
                 WHERE id=?12",
            )
            .bind(&number)
            .bind(&recipient)
            .bind(input.coach_id)
            .bind(input.school_id)
            .bind(input.signing_sheet_id)
            .bind(&input.period_start)
            .bind(&input.period_end)
            .bind(input.hours_total)
            .bind(input.rate_cents)
            .bind(amount_cents)
            .bind(&input.notes)
            .bind(id)
            .execute(pool)
            .await?;
            id
        }
        None => sqlx::query(
            "INSERT INTO invoice (workspace_id, number, recipient, coach_id, school_id, signing_sheet_id,
                period_start, period_end, hours_total, rate_cents, amount_cents, notes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        )
        .bind(workspace_id)
        .bind(&number)
        .bind(&recipient)
        .bind(input.coach_id)
        .bind(input.school_id)
        .bind(input.signing_sheet_id)
        .bind(&input.period_start)
        .bind(&input.period_end)
        .bind(input.hours_total)
        .bind(input.rate_cents)
        .bind(amount_cents)
        .bind(&input.notes)
        .execute(pool)
        .await?
        .last_insert_rowid(),
    };

    let inv = fetch_one(pool, id).await?;

    // Spine sync — invoice number is the human-facing identifier.
    if let Err(e) = crate::spine::entity::upsert(
        pool,
        crate::spine::entity::UpsertParams {
            kind: "invoice",
            display_name: &inv.number,
            pack_slug: Some("lead-to-empower"),
            attributes_json: None,
            workspace_id: inv.workspace_id,
            pack_table_id: Some(inv.id),
        },
    )
    .await
    {
        tracing::warn!("spine entity sync (invoice): {e}");
    }

    if was_new {
        let attrs = serde_json::json!({
            "invoice_id": inv.id,
            "number": inv.number,
            "amount_cents": inv.amount_cents,
            "recipient": inv.recipient,
        })
        .to_string();
        if let Err(e) = crate::spine::event::record(
            pool,
            crate::spine::event::RecordParams {
                entity_id: None,
                kind: "invoice_drafted",
                pack_slug: Some("lead-to-empower"),
                occurred_at: None,
                attributes_json: Some(&attrs),
                workspace_id: inv.workspace_id,
            },
        )
        .await
        {
            tracing::warn!("spine event sync (invoice drafted): {e}");
        }
    }

    Ok(inv)
}

pub async fn fetch_one(pool: &SqlitePool, id: i64) -> Result<Invoice, DomainError> {
    let row = sqlx::query_as::<_, Invoice>(
        "SELECT id, workspace_id, number, recipient, coach_id, school_id, signing_sheet_id,
                period_start, period_end, hours_total, rate_cents, amount_cents,
                status, issued_at, paid_at, notes, created_at, updated_at,
                engagement_id, purchase_order_id, school_signed_at, school_signed_by_name,
                submitted_to_polaris_at
         FROM invoice WHERE id=?1",
    )
    .bind(id)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

pub async fn delete(pool: &SqlitePool, id: i64) -> Result<(), DomainError> {
    sqlx::query("DELETE FROM invoice WHERE id=?1").bind(id).execute(pool).await?;
    Ok(())
}

const VALID_STATUSES: &[&str] = &["draft", "sent", "paid", "void"];

pub async fn transition_status(
    pool: &SqlitePool,
    id: i64,
    new_status: &str,
) -> Result<Invoice, DomainError> {
    if !VALID_STATUSES.contains(&new_status) {
        return Err(DomainError::invalid(format!("unknown status: {new_status}")));
    }

    let invoice = fetch_one(pool, id).await?;

    if new_status == invoice.status {
        return Ok(invoice);
    }

    if new_status == "sent" {
        validate_for_send(pool, &invoice).await?;
    }

    let timestamp_clause = match new_status {
        "sent" => ", issued_at = COALESCE(issued_at, CURRENT_TIMESTAMP)",
        "paid" => ", paid_at = COALESCE(paid_at, CURRENT_TIMESTAMP)",
        _ => "",
    };

    let sql = format!(
        "UPDATE invoice SET status = ?1, updated_at = CURRENT_TIMESTAMP{timestamp_clause}
         WHERE id = ?2"
    );
    sqlx::query(&sql).bind(new_status).bind(id).execute(pool).await?;

    let event_kind = format!("invoice_{new_status}");
    let _ = behavioral::log_event(pool, &event_kind, Some("invoice"), Some(id), None).await;

    let updated = fetch_one(pool, id).await?;

    // Spine event — cross-pack activity timeline picks this up.
    let attrs = serde_json::json!({
        "invoice_id": updated.id,
        "number": updated.number,
        "new_status": new_status,
    })
    .to_string();
    if let Err(e) = crate::spine::event::record(
        pool,
        crate::spine::event::RecordParams {
            entity_id: None,
            kind: &event_kind,
            pack_slug: Some("lead-to-empower"),
            occurred_at: None,
            attributes_json: Some(&attrs),
            workspace_id: updated.workspace_id,
        },
    )
    .await
    {
        tracing::warn!("spine event sync (invoice transition): {e}");
    }

    Ok(updated)
}

async fn validate_for_send(pool: &SqlitePool, invoice: &Invoice) -> Result<(), DomainError> {
    // Program-delivery invoices carry invoice_line rows and bill multiple
    // catalog modules; their unit prices come from engagement_module, not
    // a single coach.rate_cents. Detect that shape first and dispatch to
    // the right validator set. Single-line/single-coach (after-school
    // enrichment, the legacy shape) stays on the original checks.
    let line_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM invoice_line WHERE invoice_id = ?1")
        .bind(invoice.id)
        .fetch_one(pool)
        .await?;

    if line_count > 0 {
        validate_lines_match_scope(pool, invoice.id).await?;
        validate_invoice_line_arithmetic(pool, invoice).await?;
        if let Some(po_id) = invoice.purchase_order_id {
            validate_within_po_window(pool, invoice, po_id).await?;
        }
        return Ok(());
    }

    // ----- legacy single-line / single-coach validation -------------------
    let coach_id = invoice
        .coach_id
        .ok_or_else(|| DomainError::invalid("invoice has no coach assigned"))?;

    let sheet = match invoice.signing_sheet_id {
        Some(sid) => {
            let s = sqlx::query_as::<_, signing_sheet::SigningSheet>(
                "SELECT id, coach_id, school_id, period_start, period_end, signed_at, signed_by,
                        pdf_path, notes, created_at, updated_at
                 FROM signing_sheet WHERE id=?1",
            )
            .bind(sid)
            .fetch_optional(pool)
            .await?;
            s.ok_or_else(|| DomainError::invalid("linked signing sheet not found"))?
        }
        None => signing_sheet::find_match(
            pool,
            invoice.workspace_id,
            coach_id,
            invoice.school_id,
            &invoice.period_start,
            &invoice.period_end,
        )
        .await?
        .ok_or_else(|| {
            DomainError::invalid(
                "no signing sheet covers this invoice's period — link one before sending",
            )
        })?,
    };

    if sheet.coach_id != coach_id {
        return Err(DomainError::invalid("signing sheet's coach does not match invoice"));
    }
    if let Some(school_id) = invoice.school_id {
        if sheet.school_id != school_id {
            return Err(DomainError::invalid("signing sheet's school does not match invoice"));
        }
    }
    if sheet.period_start > invoice.period_start || sheet.period_end < invoice.period_end {
        return Err(DomainError::invalid(
            "signing sheet does not fully cover invoice period",
        ));
    }
    if sheet.signed_at.is_none() {
        return Err(DomainError::invalid("signing sheet has not been signed"));
    }

    let logged = coach_hours::sum_in_period(
        pool,
        coach_id,
        invoice.school_id,
        &invoice.period_start,
        &invoice.period_end,
    )
    .await?;

    if (logged - invoice.hours_total).abs() > 0.01 {
        return Err(DomainError::invalid(format!(
            "invoice claims {:.2}h but only {:.2}h are logged in coach_hours for the period",
            invoice.hours_total, logged
        )));
    }

    // Legacy invoices may also link to a PO; same window rule applies.
    if let Some(po_id) = invoice.purchase_order_id {
        validate_within_po_window(pool, invoice, po_id).await?;
    }

    Ok(())
}

// ----- multi-line (program-delivery) validators ---------------------------
//
// These checks operationalise LTE_INVOICING_SPEC.md §6. Each one
// surfaces a specific, fix-shaped error message — the PS 498 sample
// would refuse with a clear "Leadership Coaching is $2,993 in the
// catalog, not $5,013.30" rather than a generic 400.

/// Every invoice_line's unit_price_cents must equal the engagement_module's
/// agreed_price_cents (or the catalog list price when agreed is 0). This
/// is the check that catches the PS 498 invoice's Leadership Coaching
/// row being billed at the Instructional Coaching total ($5,013.30
/// instead of $2,993).
async fn validate_lines_match_scope(pool: &SqlitePool, invoice_id: i64) -> Result<(), DomainError> {
    #[derive(sqlx::FromRow)]
    struct Row {
        description: String,
        line_unit_price: i64,
        agreed_price: i64,
        catalog_list_price: i64,
        catalog_name: String,
    }
    let rows = sqlx::query_as::<_, Row>(
        "SELECT il.description AS description,
                il.unit_price_cents AS line_unit_price,
                em.agreed_price_cents AS agreed_price,
                cm.list_price_cents AS catalog_list_price,
                cm.name AS catalog_name
         FROM invoice_line il
         JOIN engagement_module em ON em.id = il.engagement_module_id
         JOIN catalog_module cm ON cm.id = em.module_id
         WHERE il.invoice_id = ?1",
    )
    .bind(invoice_id)
    .fetch_all(pool)
    .await?;

    for r in rows {
        let expected = if r.agreed_price > 0 { r.agreed_price } else { r.catalog_list_price };
        if r.line_unit_price != expected {
            return Err(DomainError::invalid(format!(
                "{}: unit price is {} but the catalog/agreed price is {}. \
                 Looks like a copy from the wrong line — fix the line's unit price before sending.",
                r.description,
                fmt_cents(r.line_unit_price),
                fmt_cents(expected),
            )));
        }
        // Also sanity-check the description against the catalog so a
        // scope-item swap doesn't go undetected at the PDF stage.
        if !description_matches_catalog(&r.description, &r.catalog_name) {
            return Err(DomainError::invalid(format!(
                "Line description \"{}\" doesn't match the catalog module \"{}\" — \
                 was the wrong scope item linked?",
                r.description, r.catalog_name
            )));
        }
    }
    Ok(())
}

/// Each invoice_line's subtotal must equal qty × unit_price (rounded to
/// cents), and the invoice header's amount_cents must equal the sum of
/// all line subtotals. Catches the PS 498 case where qty 2 × $2,993
/// should have been $5,986 but the row claimed $5,013.30.
async fn validate_invoice_line_arithmetic(pool: &SqlitePool, invoice: &Invoice) -> Result<(), DomainError> {
    #[derive(sqlx::FromRow)]
    struct Row {
        description: String,
        qty: f64,
        unit_price_cents: i64,
        subtotal_cents: i64,
    }
    let lines = sqlx::query_as::<_, Row>(
        "SELECT description, qty, unit_price_cents, subtotal_cents
         FROM invoice_line WHERE invoice_id = ?1",
    )
    .bind(invoice.id)
    .fetch_all(pool)
    .await?;

    let mut total: i64 = 0;
    for l in &lines {
        let expected = (l.qty * l.unit_price_cents as f64).round() as i64;
        if (l.subtotal_cents - expected).abs() > 0 {
            return Err(DomainError::invalid(format!(
                "{}: qty {} × {} = {}, but the line subtotal is {}. The math on the invoice doesn't agree with itself.",
                l.description,
                l.qty,
                fmt_cents(l.unit_price_cents),
                fmt_cents(expected),
                fmt_cents(l.subtotal_cents),
            )));
        }
        total += l.subtotal_cents;
    }

    if invoice.amount_cents != total {
        return Err(DomainError::invalid(format!(
            "Invoice total is {} but the line subtotals add to {}. Recompute the header total before sending.",
            fmt_cents(invoice.amount_cents),
            fmt_cents(total),
        )));
    }

    Ok(())
}

/// Invoice period must fall inside the linked PO's activity window.
/// Catches "billing for work outside what the PO authorized" — one of
/// the failure modes implied by Jacob's memory-driven tracking.
async fn validate_within_po_window(pool: &SqlitePool, invoice: &Invoice, po_id: i64) -> Result<(), DomainError> {
    #[derive(sqlx::FromRow)]
    struct PoWindow {
        po_number: String,
        activity_start: String,
        activity_end: String,
    }
    let po: Option<PoWindow> = sqlx::query_as(
        "SELECT po_number, activity_start, activity_end FROM purchase_order WHERE id = ?1",
    )
    .bind(po_id)
    .fetch_optional(pool)
    .await?;

    let Some(po) = po else {
        return Err(DomainError::invalid(format!("linked purchase order #{po_id} not found")));
    };

    if invoice.period_start < po.activity_start || invoice.period_end > po.activity_end {
        return Err(DomainError::invalid(format!(
            "Invoice covers {}..{} but PO {} only authorizes {}..{}. Move the work outside the window onto another PO before sending.",
            invoice.period_start,
            invoice.period_end,
            po.po_number,
            po.activity_start,
            po.activity_end,
        )));
    }
    Ok(())
}

/// Display-format cents like "$2,993.00". Matches the convention in
/// `pdf::fmt_money` so error messages and PDF text line up visually.
fn fmt_cents(cents: i64) -> String {
    let neg = cents < 0;
    let abs = cents.unsigned_abs() as u128;
    let dollars = abs / 100;
    let frac = abs % 100;
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

/// Lightly-normalised string comparison so "DATA COACHING" matches
/// "Data Coaching" matches "Data Coaching Module". Catalog names in
/// migration 0001 include the "Module" suffix; invoice descriptions
/// usually drop it. Compare on lowercased prefix-or-substring.
fn description_matches_catalog(line_desc: &str, catalog_name: &str) -> bool {
    let l = line_desc.trim().to_ascii_lowercase();
    let c = catalog_name.trim().to_ascii_lowercase();
    if l == c {
        return true;
    }
    let c_stripped = c.trim_end_matches(" module").trim();
    if l == c_stripped {
        return true;
    }
    l.contains(c_stripped) || c_stripped.contains(&l)
}
