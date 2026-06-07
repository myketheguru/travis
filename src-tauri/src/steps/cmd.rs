//! Tauri commands for retrieving persisted steps.

use sqlx::SqlitePool;
use tauri::State;

use super::model::StepRow;
use crate::AppState;

/// List all persisted steps for a conversation, in started_at order.
/// Used by the chat UI when restoring conversation history.
#[tauri::command]
pub async fn list_steps(
    state: State<'_, AppState>,
    conversation_id: i64,
) -> Result<Vec<StepRow>, String> {
    list_steps_inner(&state.db.pool, conversation_id)
        .await
        .map_err(|e| e.to_string())
}

async fn list_steps_inner(
    pool: &SqlitePool,
    conversation_id: i64,
) -> anyhow::Result<Vec<StepRow>> {
    let rows = sqlx::query_as::<_, StepRow>(
        "SELECT id, conversation_id, parent_step_id, kind, name, detail,
                status, summary, notes_json, started_at, completed_at, duration_ms
         FROM step
         WHERE conversation_id = ?1
         ORDER BY started_at ASC",
    )
    .bind(conversation_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Mark any still-running steps as cancelled — startup cleanup.
/// Called from lib.rs::setup to recover from crashes or app
/// kills mid-execution.
pub async fn mark_orphans_cancelled(pool: &SqlitePool) -> anyhow::Result<u64> {
    let r = sqlx::query(
        "UPDATE step
         SET status = 'cancelled',
             completed_at = CURRENT_TIMESTAMP,
             summary = COALESCE(summary, 'cancelled — app restarted')
         WHERE status = 'running'",
    )
    .execute(pool)
    .await?;
    Ok(r.rows_affected())
}
