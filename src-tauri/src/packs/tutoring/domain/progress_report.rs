use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use super::DomainError;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ProgressReport {
    pub id: i64,
    pub workspace_id: i64,
    pub student_id: i64,
    pub period_start: String,
    pub period_end: String,
    pub content: String,
    pub sent_at: Option<String>,
    pub sent_to: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressReportInput {
    pub id: Option<i64>,
    pub student_id: i64,
    pub period_start: String,
    pub period_end: String,
    pub content: String,
    pub sent_at: Option<String>,
    pub sent_to: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProgressReportFilter {
    pub student_id: Option<i64>,
    pub sent: Option<bool>,
}

pub async fn list(
    pool: &SqlitePool,
    workspace_ids: &[i64],
    filter: ProgressReportFilter,
) -> Result<Vec<ProgressReport>, DomainError> {
    let ws_start = 3usize;
    let ws_placeholders = (ws_start..ws_start + workspace_ids.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT id, workspace_id, student_id, period_start, period_end, content, sent_at, sent_to,
                created_at, updated_at
         FROM progress_report
         WHERE (?1 IS NULL OR student_id = ?1)
           AND (?2 IS NULL OR (sent_at IS NOT NULL) = ?2)
           AND workspace_id IN ({ws_placeholders})
         ORDER BY period_end DESC, id DESC"
    );
    let mut q = sqlx::query_as::<_, ProgressReport>(&sql)
        .bind(filter.student_id)
        .bind(filter.sent);
    for ws in workspace_ids {
        q = q.bind(ws);
    }
    Ok(q.fetch_all(pool).await?)
}

pub async fn upsert(
    pool: &SqlitePool,
    workspace_id: i64,
    input: ProgressReportInput,
) -> Result<ProgressReport, DomainError> {
    if input.period_start > input.period_end {
        return Err(DomainError::invalid("period_start must be on or before period_end"));
    }
    if input.content.trim().is_empty() {
        return Err(DomainError::invalid("content is required"));
    }

    let id = match input.id {
        Some(id) => {
            sqlx::query(
                "UPDATE progress_report SET student_id=?1, period_start=?2, period_end=?3,
                     content=?4, sent_at=?5, sent_to=?6, updated_at=CURRENT_TIMESTAMP
                 WHERE id=?7",
            )
            .bind(input.student_id)
            .bind(&input.period_start)
            .bind(&input.period_end)
            .bind(&input.content)
            .bind(&input.sent_at)
            .bind(&input.sent_to)
            .bind(id)
            .execute(pool)
            .await?;
            id
        }
        None => sqlx::query(
            "INSERT INTO progress_report (workspace_id, student_id, period_start, period_end, content,
                sent_at, sent_to)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )
        .bind(workspace_id)
        .bind(input.student_id)
        .bind(&input.period_start)
        .bind(&input.period_end)
        .bind(&input.content)
        .bind(&input.sent_at)
        .bind(&input.sent_to)
        .execute(pool)
        .await?
        .last_insert_rowid(),
    };

    let row = sqlx::query_as::<_, ProgressReport>(
        "SELECT id, workspace_id, student_id, period_start, period_end, content, sent_at, sent_to,
                created_at, updated_at
         FROM progress_report WHERE id=?1",
    )
    .bind(id)
    .fetch_one(pool)
    .await?;

    // Spine event for activity timeline.
    let kind = if row.sent_at.is_some() {
        "progress_report_sent"
    } else {
        "progress_report_drafted"
    };
    let attrs = serde_json::json!({
        "report_id": row.id,
        "student_id": row.student_id,
        "period_start": row.period_start,
        "period_end": row.period_end,
    })
    .to_string();
    if let Err(e) = crate::spine::event::record(
        pool,
        crate::spine::event::RecordParams {
            entity_id: None,
            kind,
            pack_slug: Some("tutoring"),
            occurred_at: None,
            attributes_json: Some(&attrs),
            workspace_id: row.workspace_id,
        },
    )
    .await
    {
        tracing::warn!("spine event sync (progress_report): {e}");
    }

    Ok(row)
}

pub async fn delete(pool: &SqlitePool, id: i64) -> Result<(), DomainError> {
    sqlx::query("DELETE FROM progress_report WHERE id=?1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}
