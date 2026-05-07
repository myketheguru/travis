use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use super::DomainError;
use crate::behavioral;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct CoachHours {
    pub id: i64,
    pub coach_id: i64,
    pub school_id: i64,
    pub session_date: String,
    pub hours: f64,
    pub description: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoachHoursInput {
    pub id: Option<i64>,
    pub coach_id: i64,
    pub school_id: i64,
    pub session_date: String,
    pub hours: f64,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CoachHoursFilter {
    pub coach_id: Option<i64>,
    pub school_id: Option<i64>,
    pub period_start: Option<String>,
    pub period_end: Option<String>,
}

pub async fn list(
    pool: &SqlitePool,
    filter: CoachHoursFilter,
) -> Result<Vec<CoachHours>, DomainError> {
    let rows = sqlx::query_as::<_, CoachHours>(
        "SELECT id, coach_id, school_id, session_date, hours, description, created_at, updated_at
         FROM coach_hours
         WHERE (?1 IS NULL OR coach_id = ?1)
           AND (?2 IS NULL OR school_id = ?2)
           AND (?3 IS NULL OR session_date >= ?3)
           AND (?4 IS NULL OR session_date <= ?4)
         ORDER BY session_date DESC, id DESC",
    )
    .bind(filter.coach_id)
    .bind(filter.school_id)
    .bind(&filter.period_start)
    .bind(&filter.period_end)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn upsert(
    pool: &SqlitePool,
    input: CoachHoursInput,
) -> Result<CoachHours, DomainError> {
    if input.hours <= 0.0 {
        return Err(DomainError::invalid("hours must be positive"));
    }
    if input.session_date.trim().is_empty() {
        return Err(DomainError::invalid("session_date is required (YYYY-MM-DD)"));
    }

    let id = match input.id {
        Some(id) => {
            sqlx::query(
                "UPDATE coach_hours SET coach_id=?1, school_id=?2, session_date=?3, hours=?4,
                    description=?5, updated_at=CURRENT_TIMESTAMP WHERE id=?6",
            )
            .bind(input.coach_id)
            .bind(input.school_id)
            .bind(&input.session_date)
            .bind(input.hours)
            .bind(&input.description)
            .bind(id)
            .execute(pool)
            .await?;
            id
        }
        None => sqlx::query(
            "INSERT INTO coach_hours (coach_id, school_id, session_date, hours, description)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind(input.coach_id)
        .bind(input.school_id)
        .bind(&input.session_date)
        .bind(input.hours)
        .bind(&input.description)
        .execute(pool)
        .await?
        .last_insert_rowid(),
    };

    let row = sqlx::query_as::<_, CoachHours>(
        "SELECT id, coach_id, school_id, session_date, hours, description, created_at, updated_at
         FROM coach_hours WHERE id=?1",
    )
    .bind(id)
    .fetch_one(pool)
    .await?;

    let _ = behavioral::log_event(pool, "coach_hours_logged", Some("coach_hours"), Some(id), None).await;

    // Spine event — surfaces the entry in cross-pack activity timelines.
    let attrs = serde_json::json!({
        "coach_hours_id": row.id,
        "coach_id": row.coach_id,
        "school_id": row.school_id,
        "session_date": row.session_date,
        "hours": row.hours,
    })
    .to_string();
    if let Err(e) = crate::spine::event::record(
        pool,
        crate::spine::event::RecordParams {
            entity_id: None,
            kind: "coach_hours_logged",
            pack_slug: Some("lead-to-empower"),
            occurred_at: None,
            attributes_json: Some(&attrs),
        },
    )
    .await
    {
        tracing::warn!("spine event sync (coach_hours): {e}");
    }

    Ok(row)
}

pub async fn delete(pool: &SqlitePool, id: i64) -> Result<(), DomainError> {
    sqlx::query("DELETE FROM coach_hours WHERE id=?1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn sum_in_period(
    pool: &SqlitePool,
    coach_id: i64,
    school_id: Option<i64>,
    period_start: &str,
    period_end: &str,
) -> Result<f64, DomainError> {
    let (total,): (Option<f64>,) = sqlx::query_as(
        "SELECT SUM(hours) FROM coach_hours
         WHERE coach_id = ?1
           AND (?2 IS NULL OR school_id = ?2)
           AND session_date >= ?3
           AND session_date <= ?4",
    )
    .bind(coach_id)
    .bind(school_id)
    .bind(period_start)
    .bind(period_end)
    .fetch_one(pool)
    .await?;
    Ok(total.unwrap_or(0.0))
}
