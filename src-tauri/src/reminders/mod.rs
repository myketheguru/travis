pub mod scheduler;

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Reminder {
    pub id: i64,
    pub workspace_id: i64,
    pub text: String,
    pub kind: String,
    pub remind_at: Option<String>,
    pub fired_at: Option<String>,
    pub dismissed_at: Option<String>,
    pub source: String,
    pub link_kind: Option<String>,
    pub link_id: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReminderInput {
    pub id: Option<i64>,
    pub text: String,
    pub kind: Option<String>,
    pub remind_at: Option<String>,
    pub source: Option<String>,
    pub link_kind: Option<String>,
    pub link_id: Option<i64>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ReminderFilter {
    pub fired: Option<bool>,
    pub dismissed: Option<bool>,
    pub kind: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ReminderError {
    #[error("validation: {0}")]
    Validation(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

impl ReminderError {
    pub fn invalid<S: Into<String>>(s: S) -> Self {
        ReminderError::Validation(s.into())
    }
}

pub async fn list(
    pool: &SqlitePool,
    workspace_ids: &[i64],
    filter: ReminderFilter,
) -> Result<Vec<Reminder>, ReminderError> {
    let ws_start = 4usize;
    let ws_placeholders = (ws_start..ws_start + workspace_ids.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT id, workspace_id, text, kind, remind_at, fired_at, dismissed_at, source,
                link_kind, link_id, created_at, updated_at
         FROM reminder
         WHERE (?1 IS NULL OR (fired_at IS NOT NULL) = ?1)
           AND (?2 IS NULL OR (dismissed_at IS NOT NULL) = ?2)
           AND (?3 IS NULL OR kind = ?3)
           AND workspace_id IN ({ws_placeholders})
         ORDER BY
           CASE WHEN dismissed_at IS NULL AND fired_at IS NULL THEN 0
                WHEN dismissed_at IS NULL AND fired_at IS NOT NULL THEN 1
                ELSE 2 END,
           COALESCE(remind_at, created_at) ASC,
           id DESC"
    );
    let mut q = sqlx::query_as::<_, Reminder>(&sql)
        .bind(filter.fired)
        .bind(filter.dismissed)
        .bind(&filter.kind);
    for ws in workspace_ids {
        q = q.bind(ws);
    }
    Ok(q.fetch_all(pool).await?)
}

pub async fn upsert(
    pool: &SqlitePool,
    workspace_id: i64,
    input: ReminderInput,
) -> Result<Reminder, ReminderError> {
    let text = input.text.trim().to_string();
    if text.is_empty() {
        return Err(ReminderError::invalid("text is required"));
    }
    let kind = input.kind.unwrap_or_else(|| "time".into());
    if !["time", "context", "behavioral"].contains(&kind.as_str()) {
        return Err(ReminderError::invalid(format!("unknown reminder kind: {kind}")));
    }
    if input.link_kind.is_some() != input.link_id.is_some() {
        return Err(ReminderError::invalid(
            "link_kind and link_id must be set together",
        ));
    }
    if kind == "time" && input.remind_at.as_deref().map(str::trim).unwrap_or("").is_empty() {
        return Err(ReminderError::invalid(
            "remind_at is required for time-based reminders",
        ));
    }

    let source = input.source.unwrap_or_else(|| "manual".into());

    let id = match input.id {
        Some(id) => {
            sqlx::query(
                "UPDATE reminder SET text=?1, kind=?2, remind_at=?3, source=?4,
                     link_kind=?5, link_id=?6, updated_at=CURRENT_TIMESTAMP
                 WHERE id=?7",
            )
            .bind(&text)
            .bind(&kind)
            .bind(&input.remind_at)
            .bind(&source)
            .bind(&input.link_kind)
            .bind(input.link_id)
            .bind(id)
            .execute(pool)
            .await?;
            id
        }
        None => sqlx::query(
            "INSERT INTO reminder (workspace_id, text, kind, remind_at, source, link_kind, link_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )
        .bind(workspace_id)
        .bind(&text)
        .bind(&kind)
        .bind(&input.remind_at)
        .bind(&source)
        .bind(&input.link_kind)
        .bind(input.link_id)
        .execute(pool)
        .await?
        .last_insert_rowid(),
    };

    fetch_one(pool, id).await
}

pub async fn fetch_one(pool: &SqlitePool, id: i64) -> Result<Reminder, ReminderError> {
    let row = sqlx::query_as::<_, Reminder>(
        "SELECT id, workspace_id, text, kind, remind_at, fired_at, dismissed_at, source,
                link_kind, link_id, created_at, updated_at
         FROM reminder WHERE id=?1",
    )
    .bind(id)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

pub async fn dismiss(pool: &SqlitePool, id: i64) -> Result<Reminder, ReminderError> {
    sqlx::query(
        "UPDATE reminder SET dismissed_at = COALESCE(dismissed_at, CURRENT_TIMESTAMP),
             updated_at = CURRENT_TIMESTAMP
         WHERE id = ?1",
    )
    .bind(id)
    .execute(pool)
    .await?;
    fetch_one(pool, id).await
}

pub async fn delete(pool: &SqlitePool, id: i64) -> Result<(), ReminderError> {
    sqlx::query("DELETE FROM reminder WHERE id=?1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Returns time-based reminders that are due (remind_at <= now) and not yet
/// fired or dismissed.
///
/// NB: the column list must include `workspace_id` because the
/// `Reminder` struct carries it (Phase 2). Missed in the original
/// scoping pass — the scheduler logged "no column found for name:
/// workspace_id" every 30 seconds until this was fixed.
pub async fn due_now(pool: &SqlitePool, now: &str) -> Result<Vec<Reminder>, ReminderError> {
    let rows = sqlx::query_as::<_, Reminder>(
        "SELECT id, workspace_id, text, kind, remind_at, fired_at, dismissed_at, source,
                link_kind, link_id, created_at, updated_at
         FROM reminder
         WHERE kind = 'time'
           AND fired_at IS NULL
           AND dismissed_at IS NULL
           AND remind_at IS NOT NULL
           AND remind_at <= ?1
         ORDER BY remind_at ASC, id ASC",
    )
    .bind(now)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn mark_fired(pool: &SqlitePool, id: i64, when: &str) -> Result<(), ReminderError> {
    sqlx::query(
        "UPDATE reminder SET fired_at = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
    )
    .bind(when)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}
