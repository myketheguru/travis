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
