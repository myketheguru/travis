use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use super::DomainError;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: i64,
    pub tutor_id: i64,
    pub student_id: i64,
    pub subject: Option<String>,
    pub session_date: String,
    pub duration_minutes: i64,
    pub status: String,
    pub notes: Option<String>,
    pub homework: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInput {
    pub id: Option<i64>,
    pub tutor_id: i64,
    pub student_id: i64,
    pub subject: Option<String>,
    pub session_date: String,
    pub duration_minutes: i64,
    pub status: Option<String>,
    pub notes: Option<String>,
    pub homework: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SessionFilter {
    pub tutor_id: Option<i64>,
    pub student_id: Option<i64>,
    pub status: Option<String>,
    pub period_start: Option<String>,
    pub period_end: Option<String>,
}

pub async fn list(pool: &SqlitePool, filter: SessionFilter) -> Result<Vec<Session>, DomainError> {
    let rows = sqlx::query_as::<_, Session>(
        "SELECT id, tutor_id, student_id, subject, session_date, duration_minutes,
                status, notes, homework, created_at, updated_at
         FROM session
         WHERE (?1 IS NULL OR tutor_id = ?1)
           AND (?2 IS NULL OR student_id = ?2)
           AND (?3 IS NULL OR status = ?3)
           AND (?4 IS NULL OR session_date >= ?4)
           AND (?5 IS NULL OR session_date <= ?5)
         ORDER BY session_date DESC, id DESC",
    )
    .bind(filter.tutor_id)
    .bind(filter.student_id)
    .bind(&filter.status)
    .bind(&filter.period_start)
    .bind(&filter.period_end)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn upsert(pool: &SqlitePool, input: SessionInput) -> Result<Session, DomainError> {
    if input.duration_minutes <= 0 {
        return Err(DomainError::invalid("duration_minutes must be positive"));
    }
    if input.session_date.trim().is_empty() {
        return Err(DomainError::invalid("session_date is required (YYYY-MM-DD)"));
    }
    let status = input.status.unwrap_or_else(|| "completed".into());
    if !["scheduled", "completed", "cancelled", "no_show"].contains(&status.as_str()) {
        return Err(DomainError::invalid(format!("unknown session status: {status}")));
    }

    let id = match input.id {
        Some(id) => {
            sqlx::query(
                "UPDATE session SET tutor_id=?1, student_id=?2, subject=?3, session_date=?4,
                     duration_minutes=?5, status=?6, notes=?7, homework=?8,
                     updated_at=CURRENT_TIMESTAMP WHERE id=?9",
            )
            .bind(input.tutor_id)
            .bind(input.student_id)
            .bind(&input.subject)
            .bind(&input.session_date)
            .bind(input.duration_minutes)
            .bind(&status)
            .bind(&input.notes)
            .bind(&input.homework)
            .bind(id)
            .execute(pool)
            .await?;
            id
        }
        None => sqlx::query(
            "INSERT INTO session (tutor_id, student_id, subject, session_date,
                duration_minutes, status, notes, homework)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )
        .bind(input.tutor_id)
        .bind(input.student_id)
        .bind(&input.subject)
        .bind(&input.session_date)
        .bind(input.duration_minutes)
        .bind(&status)
        .bind(&input.notes)
        .bind(&input.homework)
        .execute(pool)
        .await?
        .last_insert_rowid(),
    };

    let row = sqlx::query_as::<_, Session>(
        "SELECT id, tutor_id, student_id, subject, session_date, duration_minutes,
                status, notes, homework, created_at, updated_at
         FROM session WHERE id=?1",
    )
    .bind(id)
    .fetch_one(pool)
    .await?;

    // Spine event — surfaces the session in cross-pack activity timelines.
    let event_kind = match row.status.as_str() {
        "completed" => "session_completed",
        "scheduled" => "session_scheduled",
        "cancelled" => "session_cancelled",
        "no_show"   => "session_no_show",
        _           => "session_logged",
    };
    let attrs = serde_json::json!({
        "session_id": row.id,
        "tutor_id": row.tutor_id,
        "student_id": row.student_id,
        "session_date": row.session_date,
        "duration_minutes": row.duration_minutes,
        "subject": row.subject,
    })
    .to_string();
    if let Err(e) = crate::spine::event::record(
        pool,
        crate::spine::event::RecordParams {
            entity_id: None,
            kind: event_kind,
            pack_slug: Some("tutoring"),
            occurred_at: None,
            attributes_json: Some(&attrs),
        },
    )
    .await
    {
        tracing::warn!("spine event sync (session): {e}");
    }

    Ok(row)
}

pub async fn delete(pool: &SqlitePool, id: i64) -> Result<(), DomainError> {
    sqlx::query("DELETE FROM session WHERE id=?1").bind(id).execute(pool).await?;
    Ok(())
}

/// Total billable minutes for a tutor (or tutor+student) across a period.
/// Counts only `completed` sessions — scheduled / cancelled / no_show
/// don't bill.
pub async fn billable_minutes_in_period(
    pool: &SqlitePool,
    tutor_id: i64,
    student_id: Option<i64>,
    period_start: &str,
    period_end: &str,
) -> Result<i64, DomainError> {
    let (total,): (Option<i64>,) = sqlx::query_as(
        "SELECT SUM(duration_minutes) FROM session
         WHERE tutor_id = ?1
           AND (?2 IS NULL OR student_id = ?2)
           AND status = 'completed'
           AND session_date >= ?3
           AND session_date <= ?4",
    )
    .bind(tutor_id)
    .bind(student_id)
    .bind(period_start)
    .bind(period_end)
    .fetch_one(pool)
    .await?;
    Ok(total.unwrap_or(0))
}
