//! Observer loop for proactive nudges (BRAIN.md capability #5).
//!
//! Scans the graph for state transitions and anomalies that might
//! warrant a proactive nudge, returning structured observations the
//! existing proactive thread surfaces to the LLM as candidate reasons.
//!
//! Cheap by construction: indexed lookups on existing event /
//! entity / relation tables. Runs as part of the same tick the
//! proactive thread already does — no new schedule.

use serde::Serialize;
use sqlx::SqlitePool;

const SPIKE_RECENT_DAYS: i64 = 1;
const SPIKE_DORMANT_DAYS: i64 = 14;
const SPIKE_MIN_MENTIONS: i64 = 3;

/// One thing Travis noticed that the proactive thread can choose to
/// nudge about (or stay silent on).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Observation {
    pub kind: ObservationKind,
    /// One-line natural-language summary. Already names specific
    /// entities — no generic phrasing.
    pub summary: String,
    /// Optional entity id the observation centres on. Lets the LLM
    /// know which entity to reference.
    #[serde(default)]
    pub entity_id: Option<i64>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ObservationKind {
    /// A dormant entity (no mentions in 14d) suddenly active —
    /// 3+ mentions in 24h.
    MentionSpike,
    /// A signing sheet got signed and there are uninvoiced hours on
    /// the matching engagement / coach.
    SheetSignedReady,
    /// An invoice has been in draft past its period_end + 7 days.
    StaleInvoiceDraft,
}

/// Scan once and return up to 6 ranked observations. Empty when
/// the graph is quiet.
pub async fn scan(pool: &SqlitePool, workspace_ids: &[i64]) -> Vec<Observation> {
    if workspace_ids.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<Observation> = Vec::new();
    if let Ok(mut spikes) = mention_spikes(pool, workspace_ids).await {
        out.append(&mut spikes);
    }
    if let Ok(mut sheets) = signed_sheets_ready_to_invoice(pool, workspace_ids).await {
        out.append(&mut sheets);
    }
    if let Ok(mut stale) = stale_invoice_drafts(pool, workspace_ids).await {
        out.append(&mut stale);
    }
    out.truncate(6);
    out
}

/// Entities that resumed activity after a quiet stretch.
async fn mention_spikes(
    pool: &SqlitePool,
    workspace_ids: &[i64],
) -> anyhow::Result<Vec<Observation>> {
    let placeholders = (1..=workspace_ids.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "WITH recent AS (
           SELECT entity_id, COUNT(*) AS n
           FROM event
           WHERE kind = 'mentioned'
             AND datetime(occurred_at) >= datetime('now', '-{SPIKE_RECENT_DAYS} day')
           GROUP BY entity_id
         ),
         prior AS (
           SELECT entity_id, COUNT(*) AS n
           FROM event
           WHERE kind = 'mentioned'
             AND datetime(occurred_at) < datetime('now', '-{SPIKE_RECENT_DAYS} day')
             AND datetime(occurred_at) >= datetime('now', '-{SPIKE_DORMANT_DAYS} day')
           GROUP BY entity_id
         )
         SELECT e.id, e.display_name, e.kind, recent.n AS recent_n
         FROM entity e
         JOIN recent ON recent.entity_id = e.id
         LEFT JOIN prior ON prior.entity_id = e.id
         WHERE e.archived_at IS NULL
           AND e.workspace_id IN ({placeholders})
           AND recent.n >= {SPIKE_MIN_MENTIONS}
           AND COALESCE(prior.n, 0) = 0
         ORDER BY recent.n DESC
         LIMIT 3"
    );
    let mut q = sqlx::query_as::<_, (i64, String, String, i64)>(&sql);
    for ws in workspace_ids {
        q = q.bind(*ws);
    }
    let rows = q.fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .map(|(id, name, kind, n)| Observation {
            kind: ObservationKind::MentionSpike,
            summary: format!(
                "{name} ({kind}) has come up {n} times in the last 24h after {SPIKE_DORMANT_DAYS} quiet days"
            ),
            entity_id: Some(id),
        })
        .collect())
}

/// Signing sheets that just got signed and have uninvoiced hours on
/// the same coach for the period.
async fn signed_sheets_ready_to_invoice(
    pool: &SqlitePool,
    workspace_ids: &[i64],
) -> anyhow::Result<Vec<Observation>> {
    let placeholders = (1..=workspace_ids.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT ss.id, c.name AS coach_name, s.name AS school_name,
                ss.period_start, ss.period_end
         FROM signing_sheet ss
         LEFT JOIN coach c ON c.id = ss.coach_id
         LEFT JOIN school s ON s.id = ss.school_id
         WHERE ss.signed_at IS NOT NULL
           AND datetime(ss.signed_at) >= datetime('now', '-3 day')
           AND ss.workspace_id IN ({placeholders})
           AND NOT EXISTS (
             SELECT 1 FROM invoice i
             WHERE i.signing_sheet_id = ss.id
               AND i.status != 'void'
           )
         LIMIT 3"
    );
    let mut q = sqlx::query_as::<_, (i64, Option<String>, Option<String>, String, String)>(&sql);
    for ws in workspace_ids {
        q = q.bind(*ws);
    }
    let rows = match q.fetch_all(pool).await {
        Ok(r) => r,
        Err(_) => return Ok(Vec::new()), // signing_sheet table may not exist if pack disabled
    };
    Ok(rows
        .into_iter()
        .map(|(id, coach, school, ps, pe)| {
            let who = coach.unwrap_or_else(|| "(unassigned coach)".to_string());
            let where_ = school.unwrap_or_else(|| "—".to_string());
            Observation {
                kind: ObservationKind::SheetSignedReady,
                summary: format!(
                    "{who}'s sign-in sheet for {where_} ({ps}..{pe}) is signed but not yet invoiced"
                ),
                entity_id: Some(id),
            }
        })
        .collect())
}

/// Invoices stuck in draft past their period_end + 7 days.
async fn stale_invoice_drafts(
    pool: &SqlitePool,
    workspace_ids: &[i64],
) -> anyhow::Result<Vec<Observation>> {
    let placeholders = (1..=workspace_ids.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT id, number, period_end
         FROM invoice
         WHERE status = 'draft'
           AND date(period_end) <= date('now', '-7 day')
           AND workspace_id IN ({placeholders})
         ORDER BY period_end ASC
         LIMIT 3"
    );
    let mut q = sqlx::query_as::<_, (i64, String, String)>(&sql);
    for ws in workspace_ids {
        q = q.bind(*ws);
    }
    let rows = match q.fetch_all(pool).await {
        Ok(r) => r,
        Err(_) => return Ok(Vec::new()),
    };
    Ok(rows
        .into_iter()
        .map(|(id, number, period_end)| Observation {
            kind: ObservationKind::StaleInvoiceDraft,
            summary: format!(
                "Invoice {number} has been in draft since the period ended ({period_end})"
            ),
            entity_id: Some(id),
        })
        .collect())
}

/// Format observations as a block for the proactive nudge LLM
/// prompt. Empty when no observations.
pub fn format_for_prompt(obs: &[Observation]) -> String {
    if obs.is_empty() {
        return String::new();
    }
    let mut s = String::from("THINGS TRAVIS NOTICED (candidate reasons for a nudge — pick at most one if any is worth interrupting for; otherwise stay silent):\n");
    for o in obs {
        s.push_str(&format!("- {}\n", o.summary));
    }
    s
}
