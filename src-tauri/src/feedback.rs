use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tauri::State;

use crate::AppState;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct AppFeedback {
    pub id: i64,
    pub capability: String,
    pub context: Option<String>,
    pub source_kind: Option<String>,
    pub source_id: Option<i64>,
    pub addressed_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppFeedbackInput {
    pub capability: String,
    pub context: Option<String>,
    pub source_kind: Option<String>,
    pub source_id: Option<i64>,
}

/// A recurring capability gap that hasn't been advocated for
/// recently — candidate for a self-advocacy surface (BRAIN.md
/// capability #6).
#[derive(Debug, Clone)]
pub struct RecurringGap {
    pub capability: String,
    pub hit_count: i64,
    pub latest_context: Option<String>,
}

const MIN_HITS: i64 = 3;
const RECENT_DAYS: i64 = 14;
const COOLDOWN_DAYS: i64 = 7;

/// Return up to `limit` capabilities that:
///   - have ≥ [`MIN_HITS`] rows in the last [`RECENT_DAYS`] days
///   - have NOT been surfaced within [`COOLDOWN_DAYS`] (any row's
///     last_advocacy_surfaced_at)
///   - are not yet marked addressed
/// Ranked by hit_count DESC then most-recent activity.
pub async fn recurring_unaddressed_gaps(
    pool: &SqlitePool,
    limit: i64,
) -> Vec<RecurringGap> {
    let sql = format!(
        "SELECT capability,
                COUNT(*) AS hit_count,
                (SELECT context FROM app_feedback x
                  WHERE x.capability = app_feedback.capability
                  ORDER BY x.created_at DESC LIMIT 1) AS latest_context
         FROM app_feedback
         WHERE addressed_at IS NULL
           AND datetime(created_at) >= datetime('now', '-{RECENT_DAYS} day')
         GROUP BY capability
         HAVING COUNT(*) >= {MIN_HITS}
            AND COALESCE(
              MAX(last_advocacy_surfaced_at),
              datetime('now', '-{cool} day', '-1 day')
            ) <= datetime('now', '-{cool} day')
         ORDER BY hit_count DESC, MAX(created_at) DESC
         LIMIT ?1",
        cool = COOLDOWN_DAYS,
    );
    let rows: Vec<(String, i64, Option<String>)> = sqlx::query_as(&sql)
        .bind(limit)
        .fetch_all(pool)
        .await
        .unwrap_or_default();
    rows.into_iter()
        .map(|(capability, hit_count, latest_context)| RecurringGap {
            capability,
            hit_count,
            latest_context,
        })
        .collect()
}

/// Stamp every active row for the given capability so the cooldown
/// holds. Called immediately after Travis surfaces an advocacy ask.
pub async fn mark_advocacy_surfaced(
    pool: &SqlitePool,
    capability: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE app_feedback
         SET last_advocacy_surfaced_at = CURRENT_TIMESTAMP
         WHERE capability = ?1 AND addressed_at IS NULL",
    )
    .bind(capability)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn record(pool: &SqlitePool, input: &AppFeedbackInput) -> Result<AppFeedback, sqlx::Error> {
    let cap = input.capability.trim();
    if cap.is_empty() {
        return Err(sqlx::Error::Protocol("capability is required".into()));
    }
    let id = sqlx::query(
        "INSERT INTO app_feedback (capability, context, source_kind, source_id)
         VALUES (?1, ?2, ?3, ?4)",
    )
    .bind(cap)
    .bind(&input.context)
    .bind(&input.source_kind)
    .bind(input.source_id)
    .execute(pool)
    .await?
    .last_insert_rowid();

    sqlx::query_as::<_, AppFeedback>(
        "SELECT id, capability, context, source_kind, source_id, addressed_at, created_at
         FROM app_feedback WHERE id = ?1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FeedbackFilter {
    pub addressed: Option<bool>,
}

#[tauri::command]
pub async fn list_feedback(
    state: State<'_, AppState>,
    filter: Option<FeedbackFilter>,
) -> Result<Vec<AppFeedback>, String> {
    let f = filter.unwrap_or_default();
    let rows = sqlx::query_as::<_, AppFeedback>(
        "SELECT id, capability, context, source_kind, source_id, addressed_at, created_at
         FROM app_feedback
         WHERE (?1 IS NULL OR (addressed_at IS NOT NULL) = ?1)
         ORDER BY created_at DESC LIMIT 200",
    )
    .bind(f.addressed)
    .fetch_all(&state.db.pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(rows)
}

#[tauri::command]
pub async fn ack_feedback(
    state: State<'_, AppState>,
    id: i64,
) -> Result<(), String> {
    sqlx::query(
        "UPDATE app_feedback SET addressed_at = CURRENT_TIMESTAMP WHERE id = ?1",
    )
    .bind(id)
    .execute(&state.db.pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn delete_feedback(
    state: State<'_, AppState>,
    id: i64,
) -> Result<(), String> {
    sqlx::query("DELETE FROM app_feedback WHERE id = ?1")
        .bind(id)
        .execute(&state.db.pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}
