use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use super::DomainError;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct SigningSheet {
    pub id: i64,
    pub workspace_id: i64,
    pub coach_id: i64,
    pub school_id: i64,
    pub period_start: String,
    pub period_end: String,
    pub signed_at: Option<String>,
    pub signed_by: Option<String>,
    pub pdf_path: Option<String>,
    pub notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SigningSheetInput {
    pub id: Option<i64>,
    pub coach_id: i64,
    pub school_id: i64,
    pub period_start: String,
    pub period_end: String,
    pub signed_at: Option<String>,
    pub signed_by: Option<String>,
    pub pdf_path: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SigningSheetFilter {
    pub coach_id: Option<i64>,
    pub school_id: Option<i64>,
    pub signed: Option<bool>,
}

pub async fn list(
    pool: &SqlitePool,
    workspace_ids: &[i64],
    filter: SigningSheetFilter,
) -> Result<Vec<SigningSheet>, DomainError> {
    let ws_start = 4usize;
    let ws_placeholders = (ws_start..ws_start + workspace_ids.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT id, workspace_id, coach_id, school_id, period_start, period_end, signed_at, signed_by,
                pdf_path, notes, created_at, updated_at
         FROM signing_sheet
         WHERE (?1 IS NULL OR coach_id = ?1)
           AND (?2 IS NULL OR school_id = ?2)
           AND (?3 IS NULL OR (signed_at IS NOT NULL) = ?3)
           AND workspace_id IN ({ws_placeholders})
         ORDER BY period_end DESC, id DESC"
    );
    let mut q = sqlx::query_as::<_, SigningSheet>(&sql)
        .bind(filter.coach_id)
        .bind(filter.school_id)
        .bind(filter.signed);
    for ws in workspace_ids {
        q = q.bind(ws);
    }
    Ok(q.fetch_all(pool).await?)
}

pub async fn upsert(
    pool: &SqlitePool,
    workspace_id: i64,
    input: SigningSheetInput,
) -> Result<SigningSheet, DomainError> {
    if input.period_start > input.period_end {
        return Err(DomainError::invalid("period_start must be on or before period_end"));
    }

    let id = match input.id {
        Some(id) => {
            sqlx::query(
                "UPDATE signing_sheet SET coach_id=?1, school_id=?2, period_start=?3,
                     period_end=?4, signed_at=?5, signed_by=?6, pdf_path=?7, notes=?8,
                     updated_at=CURRENT_TIMESTAMP WHERE id=?9",
            )
            .bind(input.coach_id)
            .bind(input.school_id)
            .bind(&input.period_start)
            .bind(&input.period_end)
            .bind(&input.signed_at)
            .bind(&input.signed_by)
            .bind(&input.pdf_path)
            .bind(&input.notes)
            .bind(id)
            .execute(pool)
            .await?;
            id
        }
        None => sqlx::query(
            "INSERT INTO signing_sheet (workspace_id, coach_id, school_id, period_start, period_end,
                signed_at, signed_by, pdf_path, notes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )
        .bind(workspace_id)
        .bind(input.coach_id)
        .bind(input.school_id)
        .bind(&input.period_start)
        .bind(&input.period_end)
        .bind(&input.signed_at)
        .bind(&input.signed_by)
        .bind(&input.pdf_path)
        .bind(&input.notes)
        .execute(pool)
        .await?
        .last_insert_rowid(),
    };

    let row = sqlx::query_as::<_, SigningSheet>(
        "SELECT id, workspace_id, coach_id, school_id, period_start, period_end, signed_at, signed_by,
                pdf_path, notes, created_at, updated_at
         FROM signing_sheet WHERE id=?1",
    )
    .bind(id)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

pub async fn delete(pool: &SqlitePool, id: i64) -> Result<(), DomainError> {
    sqlx::query("DELETE FROM signing_sheet WHERE id=?1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn find_match(
    pool: &SqlitePool,
    workspace_id: i64,
    coach_id: i64,
    school_id: Option<i64>,
    period_start: &str,
    period_end: &str,
) -> Result<Option<SigningSheet>, DomainError> {
    let row = sqlx::query_as::<_, SigningSheet>(
        "SELECT id, workspace_id, coach_id, school_id, period_start, period_end, signed_at, signed_by,
                pdf_path, notes, created_at, updated_at
         FROM signing_sheet
         WHERE workspace_id = ?1
           AND coach_id = ?2
           AND (?3 IS NULL OR school_id = ?3)
           AND period_start <= ?4
           AND period_end >= ?5
         ORDER BY period_end DESC LIMIT 1",
    )
    .bind(workspace_id)
    .bind(coach_id)
    .bind(school_id)
    .bind(period_start)
    .bind(period_end)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}
