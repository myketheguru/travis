//! Tauri commands for the core `task` table.

use tauri::State;

use crate::domain::{task, DomainError};
use crate::AppState;

fn err(e: DomainError) -> String {
    e.to_string()
}

#[tauri::command]
pub async fn list_tasks(
    state: State<'_, AppState>,
    filter: Option<task::TaskFilter>,
) -> Result<Vec<task::Task>, String> {
    let ws = state.workspace.read().await.clone();
    task::list(&state.db.pool, &ws, filter.unwrap_or_default())
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn upsert_task(
    state: State<'_, AppState>,
    input: task::TaskInput,
) -> Result<task::Task, String> {
    let ws = state.workspace.read().await.clone();
    task::upsert(&state.db.pool, &ws, input).await.map_err(err)
}

#[tauri::command]
pub async fn set_task_status(
    state: State<'_, AppState>,
    id: i64,
    status: String,
) -> Result<task::Task, String> {
    let ws = state.workspace.read().await.clone();
    task::set_status(&state.db.pool, &ws, id, &status)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn delete_task(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    let ws = state.workspace.read().await.clone();
    task::delete(&state.db.pool, &ws, id).await.map_err(err)
}
