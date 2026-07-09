//! v0.28.22 — pack-scheduled signals.
//!
//! Every proactive tick, we scan each default-on pack for time-sensitive
//! signals: contact birthdays in the next 7 days, bills due within 3
//! days, subscriptions renewing within 3 days, overdue follow-ups. The
//! findings get appended to the state summary the proactive-nudge LLM
//! sees so it can reference them by name in a nudge (or stay silent —
//! quiet is still the default).
//!
//! No new subsystem — this rides on the existing proactive tick loop
//! which already gates on schedule, throttle, and health.

use chrono::{Datelike, NaiveDate};
use sqlx::{Row, SqlitePool};

/// One time-sensitive finding surfaced from a pack table.
#[derive(Debug, Clone)]
pub struct PackSignal {
    pub kind: &'static str,        // birthday | bill_due | sub_renewal | overdue_followup
    pub subject: String,           // "Sarah Chen" or "ConEd" — for nudge reference-by-name
    pub detail: String,            // "in 3 days (Jul 12)"
    pub urgency: u8,               // 0 lowest, 3 highest — used to pick top 5
}

/// Collect fresh signals across default-on packs. Silently drops errors
/// per-pack so a bad row can't break the whole tick.
pub async fn scan(pool: &SqlitePool) -> Vec<PackSignal> {
    let today = chrono::Local::now().date_naive();
    let mut out = Vec::new();

    if table_exists(pool, "contact").await {
        out.extend(scan_birthdays(pool, today).await.unwrap_or_default());
    }
    if table_exists(pool, "bill").await {
        out.extend(scan_bills_due(pool, today).await.unwrap_or_default());
    }
    if table_exists(pool, "subscription").await {
        out.extend(scan_subs_renewal(pool, today).await.unwrap_or_default());
    }
    if table_exists(pool, "followup").await {
        out.extend(scan_overdue_followups(pool, today).await.unwrap_or_default());
    }

    // Highest urgency first, cap at 5 so the LLM prompt doesn't balloon.
    out.sort_by(|a, b| b.urgency.cmp(&a.urgency));
    out.truncate(5);
    out
}

pub fn format_for_prompt(signals: &[PackSignal]) -> String {
    if signals.is_empty() {
        return String::new();
    }
    let mut s = String::from("PACK SIGNALS (time-sensitive rows the user should probably see):\n");
    for sig in signals {
        s.push_str(&format!("- {} · {} · {}\n", sig.kind, sig.subject, sig.detail));
    }
    s.push_str("\nIf one of these deserves a nudge, mention it by name. Otherwise stay silent — these are hints, not commands.\n");
    s
}

async fn table_exists(pool: &SqlitePool, name: &str) -> bool {
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
    )
    .bind(name)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);
    row.map(|(n,)| n > 0).unwrap_or(false)
}

// --- birthdays ---------------------------------------------------------

async fn scan_birthdays(pool: &SqlitePool, today: NaiveDate) -> anyhow::Result<Vec<PackSignal>> {
    let rows = sqlx::query("SELECT display_name, birthday FROM contact WHERE birthday IS NOT NULL")
        .fetch_all(pool)
        .await?;
    let mut out = Vec::new();
    for r in rows {
        let name: String = r.try_get("display_name").unwrap_or_default();
        let raw: String = match r.try_get("birthday") {
            Ok(v) => v,
            Err(_) => continue,
        };
        if let Some((month, day)) = parse_birthday_md(&raw) {
            let days = days_until_next(today, month, day);
            if days <= 7 {
                let when = format_upcoming(today, month, day, days);
                let urgency = if days == 0 { 3 } else if days <= 2 { 2 } else { 1 };
                out.push(PackSignal {
                    kind: "birthday",
                    subject: name,
                    detail: when,
                    urgency,
                });
            }
        }
    }
    Ok(out)
}

/// Accepts YYYY-MM-DD, MM-DD, --MM-DD. Returns (month, day) 1-indexed.
fn parse_birthday_md(raw: &str) -> Option<(u32, u32)> {
    let s = raw.trim();
    if let Ok(dt) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Some((dt.month(), dt.day()));
    }
    let s = s.trim_start_matches("--");
    if s.len() >= 5 {
        let month: u32 = s.get(0..2).and_then(|x| x.parse().ok())?;
        let day: u32 = s.get(3..5).and_then(|x| x.parse().ok())?;
        if (1..=12).contains(&month) && (1..=31).contains(&day) {
            return Some((month, day));
        }
    }
    None
}

fn days_until_next(today: NaiveDate, month: u32, day: u32) -> i64 {
    let this_year = NaiveDate::from_ymd_opt(today.year(), month, day);
    let next_year = NaiveDate::from_ymd_opt(today.year() + 1, month, day);
    let target = match (this_year, next_year) {
        (Some(t), _) if t >= today => t,
        (_, Some(n)) => n,
        _ => return i64::MAX,
    };
    (target - today).num_days()
}

fn format_upcoming(_today: NaiveDate, month: u32, day: u32, days: i64) -> String {
    let when = if days == 0 {
        "today".to_string()
    } else if days == 1 {
        "tomorrow".to_string()
    } else {
        format!("in {days} days")
    };
    format!("{} ({:02}-{:02})", when, month, day)
}

// --- bills -------------------------------------------------------------

async fn scan_bills_due(pool: &SqlitePool, today: NaiveDate) -> anyhow::Result<Vec<PackSignal>> {
    let rows = sqlx::query(
        "SELECT name, amount_cents, next_due_at, autopay FROM bill
         WHERE next_due_at IS NOT NULL",
    )
    .fetch_all(pool)
    .await?;
    let mut out = Vec::new();
    for r in rows {
        let name: String = r.try_get("name").unwrap_or_default();
        let due: String = match r.try_get("next_due_at") {
            Ok(v) => v,
            Err(_) => continue,
        };
        let amount: Option<i64> = r.try_get("amount_cents").ok();
        let autopay: i64 = r.try_get("autopay").unwrap_or(0);
        let Some(due_date) = parse_date_prefix(&due) else { continue };
        let days = (due_date - today).num_days();
        if !(-1..=3).contains(&days) {
            continue;
        }
        let amt = amount
            .map(|c| format!("${:.2}", c as f64 / 100.0))
            .unwrap_or_default();
        let ap = if autopay == 1 { " · autopay" } else { "" };
        let when = if days == 0 {
            "due today".to_string()
        } else if days < 0 {
            format!("{} days late", -days)
        } else {
            format!("due in {days} days")
        };
        let detail = if amt.is_empty() {
            format!("{when}{ap}")
        } else {
            format!("{amt} · {when}{ap}")
        };
        let urgency = if days < 0 { 3 } else if days == 0 { 3 } else if days == 1 { 2 } else { 1 };
        out.push(PackSignal { kind: "bill_due", subject: name, detail, urgency });
    }
    Ok(out)
}

fn parse_date_prefix(s: &str) -> Option<NaiveDate> {
    let head = s.get(0..10)?;
    NaiveDate::parse_from_str(head, "%Y-%m-%d").ok()
}

// --- subscription renewals --------------------------------------------

async fn scan_subs_renewal(pool: &SqlitePool, today: NaiveDate) -> anyhow::Result<Vec<PackSignal>> {
    let rows = sqlx::query(
        "SELECT name, amount_cents, next_renewal_at FROM subscription
         WHERE status = 'active' AND next_renewal_at IS NOT NULL",
    )
    .fetch_all(pool)
    .await?;
    let mut out = Vec::new();
    for r in rows {
        let name: String = r.try_get("name").unwrap_or_default();
        let renews: String = match r.try_get("next_renewal_at") {
            Ok(v) => v,
            Err(_) => continue,
        };
        let amount: Option<i64> = r.try_get("amount_cents").ok();
        let Some(dt) = parse_date_prefix(&renews) else { continue };
        let days = (dt - today).num_days();
        if !(0..=3).contains(&days) {
            continue;
        }
        let amt = amount
            .map(|c| format!("${:.2}", c as f64 / 100.0))
            .unwrap_or_default();
        let when = if days == 0 { "renews today".to_string() } else { format!("renews in {days} days") };
        let detail = if amt.is_empty() { when } else { format!("{amt} · {when}") };
        let urgency = if days == 0 { 2 } else { 1 };
        out.push(PackSignal { kind: "sub_renewal", subject: name, detail, urgency });
    }
    Ok(out)
}

// --- overdue follow-ups -----------------------------------------------

async fn scan_overdue_followups(pool: &SqlitePool, today: NaiveDate) -> anyhow::Result<Vec<PackSignal>> {
    let today_iso = today.format("%Y-%m-%d").to_string();
    let rows = sqlx::query(
        "SELECT title, person, due_by FROM followup
         WHERE status = 'open' AND due_by IS NOT NULL AND due_by < ?1
         ORDER BY due_by ASC LIMIT 10",
    )
    .bind(&today_iso)
    .fetch_all(pool)
    .await?;
    let mut out = Vec::new();
    for r in rows {
        let title: String = r.try_get("title").unwrap_or_default();
        let person: Option<String> = r.try_get("person").ok();
        let due: String = r.try_get("due_by").unwrap_or_default();
        let days_late = parse_date_prefix(&due)
            .map(|d| (today - d).num_days())
            .unwrap_or(0);
        let subject = match person {
            Some(p) if !p.is_empty() => format!("{title} ({p})"),
            _ => title,
        };
        let detail = format!("{} days past due ({due})", days_late);
        let urgency = if days_late >= 7 { 3 } else if days_late >= 3 { 2 } else { 1 };
        out.push(PackSignal { kind: "overdue_followup", subject, detail, urgency });
    }
    Ok(out)
}
