use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use super::{coach_hours, signing_sheet, DomainError};
use crate::behavioral;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Invoice {
    pub id: i64,
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

pub async fn list(pool: &SqlitePool, filter: InvoiceFilter) -> Result<Vec<Invoice>, DomainError> {
    let rows = sqlx::query_as::<_, Invoice>(
        "SELECT id, number, recipient, coach_id, school_id, signing_sheet_id,
                period_start, period_end, hours_total, rate_cents, amount_cents,
                status, issued_at, paid_at, notes, created_at, updated_at
         FROM invoice
         WHERE (?1 IS NULL OR status = ?1)
           AND (?2 IS NULL OR coach_id = ?2)
         ORDER BY period_end DESC, id DESC",
    )
    .bind(&filter.status)
    .bind(filter.coach_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn upsert(pool: &SqlitePool, input: InvoiceInput) -> Result<Invoice, DomainError> {
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
            "INSERT INTO invoice (number, recipient, coach_id, school_id, signing_sheet_id,
                period_start, period_end, hours_total, rate_cents, amount_cents, notes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
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
        .execute(pool)
        .await?
        .last_insert_rowid(),
    };

    fetch_one(pool, id).await
}

pub async fn fetch_one(pool: &SqlitePool, id: i64) -> Result<Invoice, DomainError> {
    let row = sqlx::query_as::<_, Invoice>(
        "SELECT id, number, recipient, coach_id, school_id, signing_sheet_id,
                period_start, period_end, hours_total, rate_cents, amount_cents,
                status, issued_at, paid_at, notes, created_at, updated_at
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

    let kind = format!("invoice_{new_status}");
    let _ = behavioral::log_event(pool, &kind, Some("invoice"), Some(id), None).await;

    fetch_one(pool, id).await
}

async fn validate_for_send(pool: &SqlitePool, invoice: &Invoice) -> Result<(), DomainError> {
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

    Ok(())
}
