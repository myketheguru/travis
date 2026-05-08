//! Action handlers contributed by the Lead to Empower pack.
//!
//! Currently: `propose_invoice_draft` — drafts an NYC DoF-shaped
//! invoice from a coach + period. Moved from core's `actions.rs` in
//! step 8 of the pack refactor.

use serde::Deserialize;
use sqlx::SqlitePool;
use tauri::{AppHandle, Manager};

use crate::actions::{ActionHandler, Applied};
use crate::domain::coach_hours;
use crate::domain::invoice::{self, InvoiceInput};

pub struct ProposeInvoiceDraftHandler;

#[async_trait::async_trait]
impl ActionHandler for ProposeInvoiceDraftHandler {
    fn kind(&self) -> &'static str {
        "propose_invoice_draft"
    }

    async fn apply(
        &self,
        pool: &SqlitePool,
        app: &AppHandle,
        params_json: &str,
    ) -> anyhow::Result<Applied> {
        let state = app.state::<crate::AppState>();
        let workspace_id = state.workspace.read().await.active_id;
        apply(pool, workspace_id, params_json).await
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Params {
    coach_name: String,
    school_name: Option<String>,
    period_start: String,
    period_end: String,
    hours_total: Option<f64>,
    rate_cents: Option<i64>,
    recipient: Option<String>,
}

async fn resolve_or_create_coach(
    pool: &SqlitePool,
    workspace_id: i64,
    name: &str,
) -> anyhow::Result<(i64, String)> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        anyhow::bail!("coach name required");
    }
    let existing: Option<(i64, String)> = sqlx::query_as(
        "SELECT id, name FROM coach
         WHERE workspace_id = ?1
           AND LOWER(TRIM(name)) = LOWER(TRIM(?2))
         ORDER BY id ASC LIMIT 1",
    )
    .bind(workspace_id)
    .bind(trimmed)
    .fetch_optional(pool)
    .await?;
    if let Some((id, name)) = existing {
        return Ok((id, name));
    }
    let id = sqlx::query("INSERT INTO coach (workspace_id, name) VALUES (?1, ?2)")
        .bind(workspace_id)
        .bind(trimmed)
        .execute(pool)
        .await?
        .last_insert_rowid();
    Ok((id, trimmed.to_string()))
}

async fn resolve_or_create_school(
    pool: &SqlitePool,
    workspace_id: i64,
    name: &str,
) -> anyhow::Result<(i64, String)> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        anyhow::bail!("school name required");
    }
    let existing: Option<(i64, String)> = sqlx::query_as(
        "SELECT id, name FROM school
         WHERE workspace_id = ?1
           AND LOWER(TRIM(name)) = LOWER(TRIM(?2))
         ORDER BY id ASC LIMIT 1",
    )
    .bind(workspace_id)
    .bind(trimmed)
    .fetch_optional(pool)
    .await?;
    if let Some((id, name)) = existing {
        return Ok((id, name));
    }
    let id = sqlx::query("INSERT INTO school (workspace_id, name) VALUES (?1, ?2)")
        .bind(workspace_id)
        .bind(trimmed)
        .execute(pool)
        .await?
        .last_insert_rowid();
    Ok((id, trimmed.to_string()))
}

async fn next_invoice_number(pool: &SqlitePool) -> anyhow::Result<String> {
    let year = chrono::Utc::now().format("%Y").to_string();
    let prefix = format!("L2E-{year}-");
    let pattern = format!("{prefix}%");
    let (count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM invoice WHERE number LIKE ?1")
            .bind(&pattern)
            .fetch_one(pool)
            .await?;
    Ok(format!("{prefix}{:04}", count + 1))
}

fn fmt_cents(cents: i64) -> String {
    let neg = cents < 0;
    let abs = cents.unsigned_abs() as u128;
    let dollars = abs / 100;
    let frac = abs % 100;
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
    if neg {
        format!("-${out}.{frac:02}")
    } else {
        format!("${out}.{frac:02}")
    }
}

async fn apply(
    pool: &SqlitePool,
    workspace_id: i64,
    params_json: &str,
) -> anyhow::Result<Applied> {
    let p: Params = serde_json::from_str(params_json)?;
    let (coach_id, coach_name) = resolve_or_create_coach(pool, workspace_id, &p.coach_name).await?;

    let school_pair = if let Some(s) = p.school_name.as_ref() {
        Some(resolve_or_create_school(pool, workspace_id, s).await?)
    } else {
        None
    };
    let school_id = school_pair.as_ref().map(|(id, _)| *id);

    if p.period_start > p.period_end {
        anyhow::bail!("periodStart must be on or before periodEnd");
    }

    let hours_total = match p.hours_total {
        Some(h) if h >= 0.0 => h,
        _ => coach_hours::sum_in_period(pool, coach_id, school_id, &p.period_start, &p.period_end)
            .await
            .map_err(|e| anyhow::anyhow!("sum hours: {e}"))?,
    };

    let rate_cents = match p.rate_cents {
        Some(r) if r >= 0 => r,
        _ => {
            let row: Option<(Option<i64>,)> =
                sqlx::query_as("SELECT rate_cents FROM coach WHERE id = ?1")
                    .bind(coach_id)
                    .fetch_optional(pool)
                    .await?;
            row.and_then(|r| r.0).unwrap_or(0)
        }
    };

    let recipient = p
        .recipient
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "NYC Department of Finance".to_string());

    let number = next_invoice_number(pool).await?;
    let inv = invoice::upsert(
        pool,
        workspace_id,
        InvoiceInput {
            id: None,
            number: number.clone(),
            recipient,
            coach_id: Some(coach_id),
            school_id,
            signing_sheet_id: None,
            period_start: p.period_start.clone(),
            period_end: p.period_end.clone(),
            hours_total,
            rate_cents,
            notes: None,
        },
    )
    .await
    .map_err(|e| anyhow::anyhow!("create invoice: {e}"))?;

    Ok(Applied {
        message: format!(
            "Drafted invoice {} for Coach {} · {} → {} · {} h · {}",
            inv.number,
            coach_name,
            inv.period_start,
            inv.period_end,
            inv.hours_total,
            fmt_cents(inv.amount_cents)
        ),
        json: serde_json::json!({
            "invoiceId": inv.id,
            "number": inv.number,
            "amountCents": inv.amount_cents,
        })
        .to_string(),
    })
}
