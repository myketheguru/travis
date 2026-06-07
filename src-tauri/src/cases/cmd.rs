//! Tauri commands for case operations (frontend access).

use sqlx::SqlitePool;
use tauri::State;

use super::db::{self, Case, CaseArtifact, CaseInput};
use crate::AppState;

#[tauri::command]
pub async fn list_open_cases(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> Result<Vec<Case>, String> {
    let ws = state.workspace.read().await;
    let cases = db::list_open(&state.db.pool, &ws.visible_ids, limit.unwrap_or(20)).await;
    Ok(cases)
}

#[tauri::command]
pub async fn open_case(
    state: State<'_, AppState>,
    input: CaseInput,
) -> Result<Case, String> {
    let workspace_id = state.workspace.read().await.active_id;
    db::upsert_open(&state.db.pool, workspace_id, input)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn close_case(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    db::close(&state.db.pool, id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_case_artifacts(
    state: State<'_, AppState>,
    case_id: i64,
    limit: Option<i64>,
) -> Result<Vec<CaseArtifact>, String> {
    Ok(db::recent_artifacts(&state.db.pool, case_id, limit.unwrap_or(50)).await)
}

/// Internal — used by the LLM tools to add artifacts without going
/// through Tauri command surface.
pub async fn add_artifact_internal(
    pool: &SqlitePool,
    case_id: i64,
    kind: &str,
    payload_json: &str,
    document_id: Option<i64>,
) -> anyhow::Result<i64> {
    db::add_artifact(pool, case_id, kind, payload_json, document_id).await
}
