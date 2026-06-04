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

// ===========================================================================
// propose_program_invoice_draft — multi-line program-delivery invoices.
// LTE_INVOICING_SPEC.md slice 5.
//
// Closes the loop. Taylor (or Travis on her behalf) says "draft this
// month's invoice for the PS 498 Math engagement" and the handler:
//   1. resolves the engagement (id or name)
//   2. picks the linked PO if one covers the period
//   3. for every engagement_module on the engagement, computes the
//      *remaining* billable qty (planned qty − already billed across
//      non-void invoices) — so partial monthly bills are first-class
//   4. builds the per-line date_list from coach_hours rows tagged with
//      that module in the period
//   5. inserts the invoice + invoice_line rows
//   6. returns a draft summary the LLM can show
//
// Validators (slice 2) still apply at draft→sent, so any catalog/price
// drift Taylor introduces while reviewing the draft surfaces there.
// ===========================================================================

pub struct ProposeProgramInvoiceDraftHandler;

#[async_trait::async_trait]
impl ActionHandler for ProposeProgramInvoiceDraftHandler {
    fn kind(&self) -> &'static str {
        "propose_program_invoice_draft"
    }

    async fn apply(
        &self,
        pool: &SqlitePool,
        app: &AppHandle,
        params_json: &str,
    ) -> anyhow::Result<Applied> {
        let state = app.state::<crate::AppState>();
        let workspace_id = state.workspace.read().await.active_id;
        apply_program(pool, workspace_id, params_json).await
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProgramParams {
    engagement_id: Option<i64>,
    engagement_name: Option<String>,
    period_start: String,
    period_end: String,
    purchase_order_id: Option<i64>,
    recipient: Option<String>,
}

#[derive(sqlx::FromRow)]
struct EngagementModuleRow {
    id: i64,
    qty: f64,
    agreed_price_cents: i64,
    catalog_name: String,
    catalog_list_price_cents: i64,
}

async fn apply_program(
    pool: &SqlitePool,
    workspace_id: i64,
    params_json: &str,
) -> anyhow::Result<Applied> {
    let p: ProgramParams = serde_json::from_str(params_json)?;

    if p.period_start > p.period_end {
        anyhow::bail!("periodStart must be on or before periodEnd");
    }

    // Resolve engagement (id wins; otherwise name lookup, otherwise bail).
    let (engagement_id, engagement_name, school_id) = match (p.engagement_id, &p.engagement_name) {
        (Some(id), _) => {
            let row: Option<(String, Option<i64>)> = sqlx::query_as(
                "SELECT name, school_id FROM engagement WHERE id = ?1 AND workspace_id = ?2",
            )
            .bind(id)
            .bind(workspace_id)
            .fetch_optional(pool)
            .await?;
            let (name, school) = row.ok_or_else(|| {
                anyhow::anyhow!("engagement {id} not found in this workspace")
            })?;
            (id, name, school)
        }
        (None, Some(name)) => {
            let trimmed = name.trim();
            if trimmed.is_empty() {
                anyhow::bail!("engagementName cannot be empty");
            }
            let like = format!("%{}%", trimmed.to_lowercase());
            let row: Option<(i64, String, Option<i64>)> = sqlx::query_as(
                "SELECT id, name, school_id FROM engagement
                 WHERE workspace_id = ?1 AND LOWER(name) LIKE ?2
                 ORDER BY updated_at DESC LIMIT 1",
            )
            .bind(workspace_id)
            .bind(like)
            .fetch_optional(pool)
            .await?;
            row.ok_or_else(|| {
                anyhow::anyhow!("no engagement matching \"{trimmed}\" in this workspace")
            })?
        }
        (None, None) => anyhow::bail!("engagementId or engagementName is required"),
    };

    // Pick PO: explicit > one covering the period for this engagement.
    let po_id: Option<i64> = if p.purchase_order_id.is_some() {
        p.purchase_order_id
    } else {
        sqlx::query_scalar(
            "SELECT id FROM purchase_order
             WHERE workspace_id = ?1
               AND engagement_id = ?2
               AND activity_start <= ?3
               AND activity_end >= ?4
             ORDER BY po_date DESC LIMIT 1",
        )
        .bind(workspace_id)
        .bind(engagement_id)
        .bind(&p.period_start)
        .bind(&p.period_end)
        .fetch_optional(pool)
        .await?
    };

    // Load engagement_module rows joined with catalog for names + list prices.
    let modules: Vec<EngagementModuleRow> = sqlx::query_as(
        "SELECT em.id AS id, em.qty AS qty, em.agreed_price_cents AS agreed_price_cents,
                cm.name AS catalog_name, cm.list_price_cents AS catalog_list_price_cents
         FROM engagement_module em
         JOIN catalog_module cm ON cm.id = em.module_id
         WHERE em.engagement_id = ?1
         ORDER BY em.id ASC",
    )
    .bind(engagement_id)
    .fetch_all(pool)
    .await?;

    if modules.is_empty() {
        anyhow::bail!(
            "engagement \"{engagement_name}\" has no scope items — add engagement_module rows before drafting"
        );
    }

    // Build lines: remaining_qty * unit_price per module, with the
    // date_list computed from coach_hours.
    struct DraftLine {
        module_id: i64,
        description: String,
        qty: f64,
        unit_price_cents: i64,
        subtotal_cents: i64,
        date_list: String,
    }
    let mut draft_lines: Vec<DraftLine> = Vec::new();

    for m in &modules {
        let unit_price = if m.agreed_price_cents > 0 {
            m.agreed_price_cents
        } else {
            m.catalog_list_price_cents
        };

        // How much of this module has been billed already?
        let already_billed: f64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(il.qty), 0)
             FROM invoice_line il
             JOIN invoice i ON i.id = il.invoice_id
             WHERE il.engagement_module_id = ?1
               AND i.status != 'void'",
        )
        .bind(m.id)
        .fetch_one(pool)
        .await
        .unwrap_or(0.0);

        let remaining = m.qty - already_billed;
        if remaining <= 0.0 {
            continue;
        }

        // Build date_list from coach_hours rows tagged with this module.
        let dates: Vec<String> = sqlx::query_scalar(
            "SELECT session_date FROM coach_hours
             WHERE engagement_id = ?1
               AND engagement_module_id = ?2
               AND session_date BETWEEN ?3 AND ?4
             ORDER BY session_date ASC",
        )
        .bind(engagement_id)
        .bind(m.id)
        .bind(&p.period_start)
        .bind(&p.period_end)
        .fetch_all(pool)
        .await
        .unwrap_or_default();
        let date_list = format_date_list(&dates);

        let subtotal = ((remaining * unit_price as f64).round()) as i64;
        draft_lines.push(DraftLine {
            module_id: m.id,
            description: m.catalog_name.to_uppercase(),
            qty: remaining,
            unit_price_cents: unit_price,
            subtotal_cents: subtotal,
            date_list,
        });
    }

    if draft_lines.is_empty() {
        anyhow::bail!(
            "every scope item on \"{engagement_name}\" is fully billed already — nothing left to invoice"
        );
    }

    let total_cents: i64 = draft_lines.iter().map(|l| l.subtotal_cents).sum();

    // Recipient: school name if we can resolve it, else a sensible default.
    let recipient = if let Some(custom) = p.recipient.as_ref() {
        let t = custom.trim();
        if !t.is_empty() {
            t.to_string()
        } else {
            resolve_recipient(pool, school_id).await
        }
    } else {
        resolve_recipient(pool, school_id).await
    };

    let number = next_invoice_number(pool).await?;
    let invoice_id: i64 = sqlx::query_scalar(
        "INSERT INTO invoice
            (workspace_id, number, recipient, coach_id, school_id, signing_sheet_id,
             period_start, period_end, hours_total, rate_cents, amount_cents,
             status, engagement_id, purchase_order_id)
         VALUES (?1, ?2, ?3, NULL, ?4, NULL, ?5, ?6, 0, 0, ?7, 'draft', ?8, ?9)
         RETURNING id",
    )
    .bind(workspace_id)
    .bind(&number)
    .bind(&recipient)
    .bind(school_id)
    .bind(&p.period_start)
    .bind(&p.period_end)
    .bind(total_cents)
    .bind(engagement_id)
    .bind(po_id)
    .fetch_one(pool)
    .await?;

    for (idx, line) in draft_lines.iter().enumerate() {
        sqlx::query(
            "INSERT INTO invoice_line
                (workspace_id, invoice_id, engagement_module_id, description,
                 qty, unit_price_cents, subtotal_cents, date_list, sort_order)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )
        .bind(workspace_id)
        .bind(invoice_id)
        .bind(line.module_id)
        .bind(&line.description)
        .bind(line.qty)
        .bind(line.unit_price_cents)
        .bind(line.subtotal_cents)
        .bind(&line.date_list)
        .bind(idx as i64)
        .execute(pool)
        .await?;
    }

    let lines_summary = draft_lines
        .iter()
        .map(|l| format!("  · {} qty {} × {} = {}", l.description, format_qty_msg(l.qty), fmt_cents(l.unit_price_cents), fmt_cents(l.subtotal_cents)))
        .collect::<Vec<_>>()
        .join("\n");

    Ok(Applied {
        message: format!(
            "Drafted program invoice {} for {} · {} → {} · {}\n{}",
            number,
            engagement_name,
            p.period_start,
            p.period_end,
            fmt_cents(total_cents),
            lines_summary,
        ),
        json: serde_json::json!({
            "invoiceId": invoice_id,
            "number": number,
            "engagementId": engagement_id,
            "purchaseOrderId": po_id,
            "amountCents": total_cents,
            "lineCount": draft_lines.len(),
        })
        .to_string(),
    })
}

async fn resolve_recipient(pool: &SqlitePool, school_id: Option<i64>) -> String {
    if let Some(sid) = school_id {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT name FROM school WHERE id = ?1")
                .bind(sid)
                .fetch_optional(pool)
                .await
                .unwrap_or(None);
        if let Some((name,)) = row {
            return name;
        }
    }
    "NYC Department of Finance".to_string()
}

/// Build the LTE invoice date list — `"Jan: 29 Feb: 24 Mar: 6, 18 Apr: 17, 24"`.
/// Grouped by month abbreviation, days comma-separated within a month, no year
/// (Jacob's preference per the transcript). Dates are ISO `YYYY-MM-DD`.
fn format_date_list(dates: &[String]) -> String {
    if dates.is_empty() {
        return String::new();
    }
    // Stable ordering already done in SQL; just group by month.
    let mut by_month: Vec<(String, Vec<String>)> = Vec::new();
    for d in dates {
        if d.len() < 10 {
            continue;
        }
        let month_num = &d[5..7];
        let day = &d[8..10];
        let label = month_label(month_num).to_string();
        let day_str = day.trim_start_matches('0').to_string();
        match by_month.iter_mut().find(|(m, _)| *m == label) {
            Some((_, days)) => days.push(day_str),
            None => by_month.push((label, vec![day_str])),
        }
    }
    by_month
        .into_iter()
        .map(|(m, days)| format!("{m}: {}", days.join(", ")))
        .collect::<Vec<_>>()
        .join(" ")
}

fn month_label(num: &str) -> &'static str {
    match num {
        "01" => "Jan",
        "02" => "Feb",
        "03" => "Mar",
        "04" => "Apr",
        "05" => "May",
        "06" => "Jun",
        "07" => "Jul",
        "08" => "Aug",
        "09" => "Sep",
        "10" => "Oct",
        "11" => "Nov",
        "12" => "Dec",
        _ => "—",
    }
}

fn format_qty_msg(qty: f64) -> String {
    if (qty - qty.round()).abs() < 0.001 {
        format!("{}", qty as i64)
    } else {
        format!("{qty}")
    }
}

// ===========================================================================
// record_coach_hours — chat-first sign-in row creation.
// Resolves coach (silent create if new — coaches are observational),
// school (silent), engagement (must exist — billable relationship),
// optional engagement_module (the scope item this hour served — drives
// per-module date_list on invoices).
// ===========================================================================

pub struct RecordCoachHoursHandler;

#[async_trait::async_trait]
impl ActionHandler for RecordCoachHoursHandler {
    fn kind(&self) -> &'static str {
        "lte_record_coach_hours"
    }
    async fn apply(
        &self,
        pool: &SqlitePool,
        app: &AppHandle,
        params_json: &str,
    ) -> anyhow::Result<Applied> {
        let state = app.state::<crate::AppState>();
        let workspace_id = state.workspace.read().await.active_id;
        apply_record_coach_hours(pool, workspace_id, params_json).await
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CoachHoursParams {
    coach_name: Option<String>,
    coach_id: Option<i64>,
    school_name: Option<String>,
    school_id: Option<i64>,
    engagement_id: Option<i64>,
    engagement_name: Option<String>,
    /// Module name OR id; pinned to a specific engagement_module so the
    /// hours roll up to the right invoice line. Optional — untagged
    /// rows still record but won't show under any invoice_line.
    module_name: Option<String>,
    engagement_module_id: Option<i64>,
    session_date: String,
    hours: f64,
    description: Option<String>,
}

async fn apply_record_coach_hours(
    pool: &SqlitePool,
    workspace_id: i64,
    params_json: &str,
) -> anyhow::Result<Applied> {
    let p: CoachHoursParams = serde_json::from_str(params_json)?;
    if p.session_date.trim().is_empty() {
        anyhow::bail!("sessionDate is required (YYYY-MM-DD)");
    }
    if p.hours <= 0.0 {
        anyhow::bail!("hours must be positive");
    }

    let coach_id = if let Some(id) = p.coach_id {
        id
    } else if let Some(name) = p.coach_name.as_deref() {
        resolve_or_create_coach(pool, workspace_id, name).await?.0
    } else {
        anyhow::bail!("coachName or coachId is required");
    };

    let school_id = if let Some(id) = p.school_id {
        Some(id)
    } else if let Some(name) = p.school_name.as_deref() {
        Some(resolve_or_create_school(pool, workspace_id, name).await?.0)
    } else {
        None
    };

    let engagement_id = if let Some(id) = p.engagement_id {
        Some(id)
    } else if let Some(name) = p.engagement_name.as_deref() {
        let like = format!("%{}%", name.to_lowercase());
        let id: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM engagement
             WHERE workspace_id = ?1 AND LOWER(name) LIKE ?2
             ORDER BY updated_at DESC LIMIT 1",
        )
        .bind(workspace_id)
        .bind(like)
        .fetch_optional(pool)
        .await?;
        if id.is_none() {
            anyhow::bail!(
                "no engagement matching \"{name}\" — create one first via lte_create_engagement"
            );
        }
        id
    } else {
        None
    };

    // Resolve engagement_module if a module hint was given.
    let em_id = if let Some(id) = p.engagement_module_id {
        Some(id)
    } else if let (Some(eng_id), Some(mname)) = (engagement_id, p.module_name.as_deref()) {
        let like = format!("%{}%", mname.to_lowercase());
        sqlx::query_scalar(
            "SELECT em.id FROM engagement_module em
             JOIN catalog_module cm ON cm.id = em.module_id
             WHERE em.engagement_id = ?1
               AND LOWER(cm.name) LIKE ?2
             ORDER BY em.id ASC LIMIT 1",
        )
        .bind(eng_id)
        .bind(like)
        .fetch_optional(pool)
        .await?
    } else {
        None
    };

    let row_id: i64 = sqlx::query_scalar(
        "INSERT INTO coach_hours
            (workspace_id, coach_id, school_id, session_date, hours, description,
             engagement_id, engagement_module_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) RETURNING id",
    )
    .bind(workspace_id)
    .bind(coach_id)
    .bind(school_id)
    .bind(&p.session_date)
    .bind(p.hours)
    .bind(p.description.as_deref())
    .bind(engagement_id)
    .bind(em_id)
    .fetch_one(pool)
    .await?;

    Ok(Applied {
        message: format!(
            "Logged {} h on {} for coach #{coach_id}{}{}.",
            p.hours,
            p.session_date,
            engagement_id.map(|e| format!(" (engagement #{e})")).unwrap_or_default(),
            em_id.map(|m| format!(" tagged to scope item #{m}")).unwrap_or_default(),
        ),
        json: serde_json::json!({
            "coachHoursId": row_id,
            "coachId": coach_id,
            "engagementId": engagement_id,
            "engagementModuleId": em_id,
            "hours": p.hours,
            "sessionDate": p.session_date,
        })
        .to_string(),
    })
}

// ===========================================================================
// create_work_order — auto-totals from engagement_module rows.
// ===========================================================================

pub struct CreateWorkOrderHandler;

#[async_trait::async_trait]
impl ActionHandler for CreateWorkOrderHandler {
    fn kind(&self) -> &'static str {
        "lte_create_work_order"
    }
    async fn apply(
        &self,
        pool: &SqlitePool,
        app: &AppHandle,
        params_json: &str,
    ) -> anyhow::Result<Applied> {
        let state = app.state::<crate::AppState>();
        let workspace_id = state.workspace.read().await.active_id;
        apply_create_work_order(pool, workspace_id, params_json).await
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkOrderParams {
    engagement_id: Option<i64>,
    engagement_name: Option<String>,
    date_issued: Option<String>,
    vendor_signed_at: Option<String>,
    vendor_signed_by_name: Option<String>,
    school_signed_at: Option<String>,
    school_signed_by_name: Option<String>,
    notes: Option<String>,
}

async fn apply_create_work_order(
    pool: &SqlitePool,
    workspace_id: i64,
    params_json: &str,
) -> anyhow::Result<Applied> {
    let p: WorkOrderParams = serde_json::from_str(params_json)?;

    let (eng_id, eng_name, contract_ref): (i64, String, Option<String>) = if let Some(id) = p.engagement_id
    {
        let row: Option<(String, Option<String>)> = sqlx::query_as(
            "SELECT name, contract_ref FROM engagement WHERE id = ?1 AND workspace_id = ?2",
        )
        .bind(id)
        .bind(workspace_id)
        .fetch_optional(pool)
        .await?;
        let (n, cr) = row.ok_or_else(|| anyhow::anyhow!("engagement #{id} not found"))?;
        (id, n, cr)
    } else if let Some(name) = p.engagement_name.as_deref() {
        let like = format!("%{}%", name.to_lowercase());
        let row: Option<(i64, String, Option<String>)> = sqlx::query_as(
            "SELECT id, name, contract_ref FROM engagement
             WHERE workspace_id = ?1 AND LOWER(name) LIKE ?2
             ORDER BY updated_at DESC LIMIT 1",
        )
        .bind(workspace_id)
        .bind(like)
        .fetch_optional(pool)
        .await?;
        row.ok_or_else(|| anyhow::anyhow!("no engagement matching \"{name}\""))?
    } else {
        anyhow::bail!("engagementId or engagementName is required");
    };

    let total_cents: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(em.qty * CASE WHEN em.agreed_price_cents > 0
                                            THEN em.agreed_price_cents
                                            ELSE cm.list_price_cents END), 0)
         FROM engagement_module em
         JOIN catalog_module cm ON cm.id = em.module_id
         WHERE em.engagement_id = ?1",
    )
    .bind(eng_id)
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    let date_issued = p
        .date_issued
        .clone()
        .unwrap_or_else(|| chrono::Utc::now().format("%Y-%m-%d").to_string());

    let id: i64 = sqlx::query_scalar(
        "INSERT INTO work_order
            (workspace_id, engagement_id, contract_ref, date_issued,
             vendor_signed_at, vendor_signed_by_name,
             school_signed_at, school_signed_by_name,
             total_cents, notes)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10) RETURNING id",
    )
    .bind(workspace_id)
    .bind(eng_id)
    .bind(contract_ref.as_deref())
    .bind(&date_issued)
    .bind(p.vendor_signed_at.as_deref())
    .bind(p.vendor_signed_by_name.as_deref())
    .bind(p.school_signed_at.as_deref())
    .bind(p.school_signed_by_name.as_deref())
    .bind(total_cents)
    .bind(p.notes.as_deref())
    .fetch_one(pool)
    .await?;

    Ok(Applied {
        message: format!(
            "Created work order #{id} for \"{eng_name}\" dated {date_issued}. Scope totals {}.",
            fmt_cents(total_cents)
        ),
        json: serde_json::json!({
            "workOrderId": id,
            "engagementId": eng_id,
            "dateIssued": date_issued,
            "totalCents": total_cents,
        })
        .to_string(),
    })
}

// ===========================================================================
// create_purchase_order — DOE-issued, received-by-LTE.
// Optionally links to the work_order that triggered it. activity_start /
// activity_end define the billable window; the invoice-within-PO-window
// validator (slice 2) checks against these.
// ===========================================================================

pub struct CreatePurchaseOrderHandler;

#[async_trait::async_trait]
impl ActionHandler for CreatePurchaseOrderHandler {
    fn kind(&self) -> &'static str {
        "lte_create_purchase_order"
    }
    async fn apply(
        &self,
        pool: &SqlitePool,
        app: &AppHandle,
        params_json: &str,
    ) -> anyhow::Result<Applied> {
        let state = app.state::<crate::AppState>();
        let workspace_id = state.workspace.read().await.active_id;
        apply_create_purchase_order(pool, workspace_id, params_json).await
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PoParams {
    po_number: String,
    suffix: Option<String>,
    tracking_number: Option<String>,
    engagement_id: Option<i64>,
    engagement_name: Option<String>,
    work_order_id: Option<i64>,
    po_date: Option<String>,
    activity_start: String,
    activity_end: String,
    deliver_to_attention: Option<String>,
    deliver_to_address: Option<String>,
    authorized_by: Option<String>,
    authorized_at: Option<String>,
    total_cents: Option<i64>,
    notes: Option<String>,
}

async fn apply_create_purchase_order(
    pool: &SqlitePool,
    workspace_id: i64,
    params_json: &str,
) -> anyhow::Result<Applied> {
    let p: PoParams = serde_json::from_str(params_json)?;
    let po_number = p.po_number.trim().to_string();
    if po_number.is_empty() {
        anyhow::bail!("poNumber is required");
    }
    if p.activity_start > p.activity_end {
        anyhow::bail!("activityStart must be on or before activityEnd");
    }

    let eng_id: i64 = if let Some(id) = p.engagement_id {
        id
    } else if let Some(name) = p.engagement_name.as_deref() {
        let like = format!("%{}%", name.to_lowercase());
        sqlx::query_scalar(
            "SELECT id FROM engagement
             WHERE workspace_id = ?1 AND LOWER(name) LIKE ?2
             ORDER BY updated_at DESC LIMIT 1",
        )
        .bind(workspace_id)
        .bind(like)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| anyhow::anyhow!("no engagement matching \"{name}\""))?
    } else {
        anyhow::bail!("engagementId or engagementName is required");
    };

    // Default total from engagement_module if not supplied.
    let total = if let Some(t) = p.total_cents {
        t
    } else {
        sqlx::query_scalar(
            "SELECT COALESCE(SUM(em.qty * CASE WHEN em.agreed_price_cents > 0
                                                THEN em.agreed_price_cents
                                                ELSE cm.list_price_cents END), 0)
             FROM engagement_module em
             JOIN catalog_module cm ON cm.id = em.module_id
             WHERE em.engagement_id = ?1",
        )
        .bind(eng_id)
        .fetch_one(pool)
        .await
        .unwrap_or(0)
    };

    let id: i64 = sqlx::query_scalar(
        "INSERT INTO purchase_order
            (workspace_id, engagement_id, work_order_id, po_number, suffix, tracking_number,
             po_date, activity_start, activity_end,
             deliver_to_attention, deliver_to_address,
             authorized_by, authorized_at, total_cents, notes)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15) RETURNING id",
    )
    .bind(workspace_id)
    .bind(eng_id)
    .bind(p.work_order_id)
    .bind(&po_number)
    .bind(p.suffix.as_deref().unwrap_or("01"))
    .bind(p.tracking_number.as_deref())
    .bind(p.po_date.as_deref())
    .bind(&p.activity_start)
    .bind(&p.activity_end)
    .bind(p.deliver_to_attention.as_deref())
    .bind(p.deliver_to_address.as_deref())
    .bind(p.authorized_by.as_deref())
    .bind(p.authorized_at.as_deref())
    .bind(total)
    .bind(p.notes.as_deref())
    .fetch_one(pool)
    .await?;

    Ok(Applied {
        message: format!(
            "Recorded purchase order {} (#{id}), activity {}..{}, total {}.",
            po_number,
            p.activity_start,
            p.activity_end,
            fmt_cents(total),
        ),
        json: serde_json::json!({
            "purchaseOrderId": id,
            "poNumber": po_number,
            "engagementId": eng_id,
            "totalCents": total,
        })
        .to_string(),
    })
}

// ===========================================================================
// create_contract — confirmation-card action for new master agreements.
// Schools are observational (silent create via lte_find_or_create_school).
// Contracts commit to a relationship, so they go through the proposal/
// confirmation flow even when Travis has all the data.
// ===========================================================================

pub struct CreateContractHandler;

#[async_trait::async_trait]
impl ActionHandler for CreateContractHandler {
    fn kind(&self) -> &'static str {
        "lte_create_contract"
    }
    async fn apply(
        &self,
        pool: &SqlitePool,
        app: &AppHandle,
        params_json: &str,
    ) -> anyhow::Result<Applied> {
        let state = app.state::<crate::AppState>();
        let workspace_id = state.workspace.read().await.active_id;
        apply_create_contract(pool, workspace_id, params_json).await
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateContractParams {
    /// External ref (e.g. "QR179CF"). Required. Uppercased + trimmed on save.
    contract_ref: String,
    name: Option<String>,
    counterparty: Option<String>,
    parent_solicitation: Option<String>,
    term_start: Option<String>,
    term_end: Option<String>,
    ceiling_cents: Option<i64>,
    status: Option<String>,
    notes: Option<String>,
}

async fn apply_create_contract(
    pool: &SqlitePool,
    workspace_id: i64,
    params_json: &str,
) -> anyhow::Result<Applied> {
    let p: CreateContractParams = serde_json::from_str(params_json)?;
    let r = p.contract_ref.trim().to_uppercase();
    if r.is_empty() {
        anyhow::bail!("contractRef is required");
    }

    // Idempotent: if a contract with this ref already exists in this
    // workspace, return it instead of erroring out. Lets Travis re-issue
    // the same action safely on a retry.
    let existing: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM contract WHERE workspace_id = ?1 AND ref = ?2",
    )
    .bind(workspace_id)
    .bind(&r)
    .fetch_optional(pool)
    .await?;

    if let Some(id) = existing {
        return Ok(Applied {
            message: format!("Contract {r} already exists (#{id}). No change."),
            json: serde_json::json!({ "contractId": id, "ref": r, "wasCreated": false })
                .to_string(),
        });
    }

    let status = p.status.as_deref().unwrap_or("active");
    let name = p.name.as_deref().unwrap_or(&r);
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO contract
            (workspace_id, ref, name, counterparty, parent_solicitation,
             term_start, term_end, ceiling_cents, status, notes)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10) RETURNING id",
    )
    .bind(workspace_id)
    .bind(&r)
    .bind(name)
    .bind(p.counterparty.as_deref())
    .bind(p.parent_solicitation.as_deref())
    .bind(p.term_start.as_deref())
    .bind(p.term_end.as_deref())
    .bind(p.ceiling_cents.unwrap_or(0))
    .bind(status)
    .bind(p.notes.as_deref())
    .fetch_one(pool)
    .await?;

    Ok(Applied {
        message: format!(
            "Created contract {r} (#{id}, {status}){}{}",
            p.counterparty
                .as_deref()
                .map(|c| format!(" with {c}"))
                .unwrap_or_default(),
            p.term_end
                .as_deref()
                .map(|t| format!(", ends {t}"))
                .unwrap_or_default(),
        ),
        json: serde_json::json!({
            "contractId": id,
            "ref": r,
            "status": status,
            "wasCreated": true,
        })
        .to_string(),
    })
}

// ===========================================================================
// create_engagement — confirmation-card action for new engagements.
// Resolves school + contract first (creating school silently if missing,
// resolving contract by ref), then inserts the engagement linking both.
// ===========================================================================

pub struct CreateEngagementHandler;

#[async_trait::async_trait]
impl ActionHandler for CreateEngagementHandler {
    fn kind(&self) -> &'static str {
        "lte_create_engagement"
    }
    async fn apply(
        &self,
        pool: &SqlitePool,
        app: &AppHandle,
        params_json: &str,
    ) -> anyhow::Result<Applied> {
        let state = app.state::<crate::AppState>();
        let workspace_id = state.workspace.read().await.active_id;
        apply_create_engagement(pool, workspace_id, params_json).await
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateEngagementParams {
    /// School name OR id. If name and no match, school is created silently.
    school_name: Option<String>,
    school_id: Option<i64>,
    /// Contract ref OR id. Optional — engagement can exist without a
    /// contract link initially (Travis will surface that as a gap).
    contract_ref: Option<String>,
    contract_id: Option<i64>,
    /// Engagement display name. Defaults to "<School> — <School Year>"
    /// when omitted.
    name: Option<String>,
    school_year: Option<String>,
    stage: Option<String>,
    summary: Option<String>,
}

async fn apply_create_engagement(
    pool: &SqlitePool,
    workspace_id: i64,
    params_json: &str,
) -> anyhow::Result<Applied> {
    let p: CreateEngagementParams = serde_json::from_str(params_json)?;

    // ----- resolve school -----
    let (school_id, school_name) = if let Some(id) = p.school_id {
        let name: String =
            sqlx::query_scalar("SELECT name FROM school WHERE id = ?1 AND workspace_id = ?2")
                .bind(id)
                .bind(workspace_id)
                .fetch_optional(pool)
                .await?
                .ok_or_else(|| anyhow::anyhow!("school #{id} not found in workspace"))?;
        (Some(id), name)
    } else if let Some(name) = p.school_name.as_deref() {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            anyhow::bail!("schoolName cannot be empty");
        }
        let existing: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM school WHERE workspace_id = ?1
             AND LOWER(TRIM(name)) = LOWER(TRIM(?2)) LIMIT 1",
        )
        .bind(workspace_id)
        .bind(trimmed)
        .fetch_optional(pool)
        .await?;
        let id = if let Some(id) = existing {
            id
        } else {
            sqlx::query_scalar(
                "INSERT INTO school (workspace_id, name) VALUES (?1, ?2) RETURNING id",
            )
            .bind(workspace_id)
            .bind(trimmed)
            .fetch_one(pool)
            .await?
        };
        (Some(id), trimmed.to_string())
    } else {
        anyhow::bail!("schoolName or schoolId is required");
    };

    // ----- resolve contract (optional) -----
    let (contract_id, contract_ref_resolved) = if let Some(id) = p.contract_id {
        let r: Option<String> =
            sqlx::query_scalar("SELECT ref FROM contract WHERE id = ?1 AND workspace_id = ?2")
                .bind(id)
                .bind(workspace_id)
                .fetch_optional(pool)
                .await?;
        (Some(id), r)
    } else if let Some(r) = p.contract_ref.as_deref() {
        let trimmed = r.trim().to_uppercase();
        let existing: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM contract WHERE workspace_id = ?1 AND ref = ?2",
        )
        .bind(workspace_id)
        .bind(&trimmed)
        .fetch_optional(pool)
        .await?;
        if existing.is_none() {
            anyhow::bail!(
                "contract {trimmed} not found — create it first via lte_create_contract"
            );
        }
        (existing, Some(trimmed))
    } else {
        (None, None)
    };

    let school_year = p.school_year.as_deref().unwrap_or("");
    let display_name = p.name.clone().unwrap_or_else(|| {
        if !school_year.is_empty() {
            format!("{school_name} — {school_year}")
        } else {
            school_name.clone()
        }
    });
    let stage = p.stage.as_deref().unwrap_or("assessment");

    let id: i64 = sqlx::query_scalar(
        "INSERT INTO engagement
            (workspace_id, name, school_id, stage, contract_ref, school_year, contract_id, summary)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) RETURNING id",
    )
    .bind(workspace_id)
    .bind(&display_name)
    .bind(school_id)
    .bind(stage)
    .bind(contract_ref_resolved.as_deref())
    .bind(if school_year.is_empty() {
        None
    } else {
        Some(school_year)
    })
    .bind(contract_id)
    .bind(p.summary.as_deref())
    .fetch_one(pool)
    .await?;

    let contract_note = contract_ref_resolved
        .as_deref()
        .map(|r| format!(", under contract {r}"))
        .unwrap_or_else(|| " (no contract linked yet)".to_string());

    Ok(Applied {
        message: format!(
            "Created engagement \"{display_name}\" (#{id}, stage {stage}) at {school_name}{contract_note}."
        ),
        json: serde_json::json!({
            "engagementId": id,
            "schoolId": school_id,
            "contractId": contract_id,
            "name": display_name,
            "stage": stage,
        })
        .to_string(),
    })
}

// ===========================================================================
// DeriveSignInSheet — Path A of the master-sheet → sign-in-sheet flow.
//
// Taylor's master coach-hours spreadsheet (Google Sheet → CSV/XLSX export)
// has been ingested as a document and extracted into structured rows by
// the documents pipeline. This action filters those rows down to one
// engagement (school + period), upserts the matching rows into
// `coach_hours`, and renders the printable sign-in sheet PDF via the
// existing renderer. Closes the loop on [[feedback-docs-first]] for the
// sign-in-sheet half of Taylor's monthly ritual.
// ===========================================================================

pub struct DeriveSignInSheetHandler;

#[async_trait::async_trait]
impl ActionHandler for DeriveSignInSheetHandler {
    fn kind(&self) -> &'static str {
        "lte_derive_sign_in_sheet"
    }

    async fn apply(
        &self,
        pool: &SqlitePool,
        app: &AppHandle,
        params_json: &str,
    ) -> anyhow::Result<Applied> {
        let state = app.state::<crate::AppState>();
        let workspace_id = state.workspace.read().await.active_id;
        apply_derive_sign_in_sheet(pool, app, workspace_id, params_json).await
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeriveParams {
    /// The document id of the master coach-hours spreadsheet.
    source_document_id: i64,
    /// The engagement to derive the sheet for.
    engagement_id: i64,
    /// ISO date string YYYY-MM-DD.
    period_start: String,
    /// ISO date string YYYY-MM-DD.
    period_end: String,
}

async fn apply_derive_sign_in_sheet(
    pool: &SqlitePool,
    app: &AppHandle,
    workspace_id: i64,
    params_json: &str,
) -> anyhow::Result<Applied> {
    let p: DeriveParams = serde_json::from_str(params_json)?;

    // 1. Load source doc + extracted JSON.
    let doc = crate::documents::db::get(pool, p.source_document_id)
        .await?
        .ok_or_else(|| {
            anyhow::anyhow!(
                "master sheet document {} not found",
                p.source_document_id
            )
        })?;
    if doc.workspace_id != workspace_id {
        anyhow::bail!(
            "master sheet document {} is in a different workspace",
            p.source_document_id
        );
    }
    let extracted: serde_json::Value = doc
        .extracted_json
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .map_err(|e| anyhow::anyhow!("malformed extracted JSON on master sheet: {e}"))?
        .ok_or_else(|| {
            anyhow::anyhow!(
                "master sheet document {} hasn't been extracted yet — try re-running \
                 extraction with kind='coach_hours_master'",
                p.source_document_id
            )
        })?;
    let rows = extracted
        .get("rows")
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "master sheet document {} extracted JSON has no 'rows' array — was \
                 it labelled as kind 'coach_hours_master'?",
                p.source_document_id
            )
        })?;

    // 2. Load the engagement + school.
    let row: Option<(String, Option<i64>)> = sqlx::query_as(
        "SELECT name, school_id FROM engagement WHERE id = ?1 AND workspace_id = ?2",
    )
    .bind(p.engagement_id)
    .bind(workspace_id)
    .fetch_optional(pool)
    .await?;
    let (engagement_name, school_id) = row.ok_or_else(|| {
        anyhow::anyhow!(
            "engagement {} not found in active workspace",
            p.engagement_id
        )
    })?;
    let school_id = school_id.ok_or_else(|| {
        anyhow::anyhow!("engagement {} has no school linked", p.engagement_id)
    })?;
    let school_name: String = sqlx::query_scalar("SELECT name FROM school WHERE id = ?1")
        .bind(school_id)
        .fetch_one(pool)
        .await?;
    let school_lower = school_name.to_lowercase();

    // 3. Filter rows: school name match (fuzzy) AND date in period AND
    //    has a coach + hours.
    let mut matched: Vec<(String, String, f64, Option<String>)> = Vec::new();
    let mut skipped_school = 0i64;
    let mut skipped_period = 0i64;
    let mut skipped_incomplete = 0i64;

    for row in rows {
        let row_school = row
            .get("school_name")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_lowercase());
        let row_date = row
            .get("date")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string());
        let row_coach = row
            .get("coach_name")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string());
        let row_hours = row.get("hours").and_then(|v| v.as_f64());
        let row_notes = row
            .get("notes")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let school_matches = match row_school.as_deref() {
            Some(s) if !s.is_empty() => {
                s.contains(&school_lower) || school_lower.contains(s)
            }
            _ => false,
        };
        if !school_matches {
            skipped_school += 1;
            continue;
        }

        let date_in_period = match row_date.as_deref() {
            Some(d) if d.len() >= 10 => {
                d >= p.period_start.as_str() && d <= p.period_end.as_str()
            }
            _ => false,
        };
        if !date_in_period {
            skipped_period += 1;
            continue;
        }

        match (row_coach, row_hours, row_date) {
            (Some(c), Some(h), Some(d)) if !c.is_empty() && h > 0.0 => {
                matched.push((c, d, h, row_notes));
            }
            _ => skipped_incomplete += 1,
        }
    }

    if matched.is_empty() {
        anyhow::bail!(
            "No matching rows in the master sheet for {school_name} between {} and {}. \
             Scanned {} rows total — {} wrong school, {} out of period, {} missing coach/hours/date.",
            p.period_start,
            p.period_end,
            rows.len(),
            skipped_school,
            skipped_period,
            skipped_incomplete
        );
    }

    // 4. Upsert into coach_hours. Dedup by (coach, school, date).
    let mut inserted = 0i64;
    let mut updated = 0i64;
    for (coach_name, date, hours, notes) in &matched {
        let (coach_id, _) = resolve_or_create_coach(pool, workspace_id, coach_name).await?;

        let existing_id: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM coach_hours
             WHERE coach_id = ?1 AND school_id = ?2 AND session_date = ?3
             LIMIT 1",
        )
        .bind(coach_id)
        .bind(school_id)
        .bind(date)
        .fetch_optional(pool)
        .await?;

        if let Some(id) = existing_id {
            sqlx::query(
                "UPDATE coach_hours
                 SET hours = ?1, description = ?2, engagement_id = ?3,
                     updated_at = CURRENT_TIMESTAMP
                 WHERE id = ?4",
            )
            .bind(hours)
            .bind(notes.as_deref())
            .bind(p.engagement_id)
            .bind(id)
            .execute(pool)
            .await?;
            updated += 1;
        } else {
            sqlx::query(
                "INSERT INTO coach_hours
                    (coach_id, school_id, session_date, hours, description, engagement_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )
            .bind(coach_id)
            .bind(school_id)
            .bind(date)
            .bind(hours)
            .bind(notes.as_deref())
            .bind(p.engagement_id)
            .execute(pool)
            .await?;
            inserted += 1;
        }
    }

    // 5. Render the sign-in sheet PDF.
    let downloads = app
        .path()
        .download_dir()
        .or_else(|_| app.path().app_data_dir())
        .map_err(|e| anyhow::anyhow!("resolve downloads dir: {e}"))?;
    std::fs::create_dir_all(&downloads)
        .map_err(|e| anyhow::anyhow!("create downloads dir: {e}"))?;
    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let dest = downloads.join(format!(
        "lte-signin-eng{}-{}.pdf",
        p.engagement_id, stamp
    ));

    let saved = super::pdf::render_sign_in_sheet(
        pool,
        p.engagement_id,
        &p.period_start,
        &p.period_end,
        &dest,
    )
    .await
    .map_err(|e| anyhow::anyhow!("render sign-in sheet: {e}"))?;

    // 6. Register the generated PDF as a document so it round-trips.
    let state = app.state::<crate::AppState>();
    let generated_doc_id = match crate::documents::cmd::register_generated_document(
        app,
        state.inner(),
        &saved,
        "signed_sheet",
        Some(&format!(
            "Sign-in Sheet · {} · {} to {} (derived from doc#{})",
            engagement_name, p.period_start, p.period_end, p.source_document_id
        )),
        None,
        None,
    )
    .await
    {
        Ok(d) => Some(d.id),
        Err(e) => {
            tracing::warn!("could not register derived sign-in PDF: {e}");
            None
        }
    };

    let matched_count = matched.len();
    let dropped = skipped_school + skipped_period + skipped_incomplete;

    Ok(Applied {
        message: format!(
            "Derived sign-in sheet for \"{engagement_name}\" at {school_name}, \
             {} to {} — {matched_count} matching row{} from the master sheet \
             ({} new, {} updated, {} skipped). PDF saved to {}.",
            p.period_start,
            p.period_end,
            if matched_count == 1 { "" } else { "s" },
            inserted,
            updated,
            dropped,
            saved.display(),
        ),
        json: serde_json::json!({
            "engagementId": p.engagement_id,
            "schoolId": school_id,
            "sourceDocumentId": p.source_document_id,
            "generatedDocumentId": generated_doc_id,
            "matchedRows": matched_count,
            "insertedRows": inserted,
            "updatedRows": updated,
            "skippedRows": dropped,
            "periodStart": p.period_start,
            "periodEnd": p.period_end,
            "outputPath": saved.to_string_lossy(),
        })
        .to_string(),
    })
}
