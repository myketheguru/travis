//! Tauri commands for the core `task` table.
//!
//! Tasks are core (PACKS.md data model: opt-in convenience for any
//! pack), so their command surface lives here, not in any pack.
//! Domain CRUD for L2E typed tables (coach, school, etc.) lives in
//! the L2E pack's `domain_cmd`.

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
    task::list(&state.db.pool, filter.unwrap_or_default())
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn upsert_task(
    state: State<'_, AppState>,
    input: task::TaskInput,
) -> Result<task::Task, String> {
    task::upsert(&state.db.pool, input).await.map_err(err)
}

#[tauri::command]
pub async fn set_task_status(
    state: State<'_, AppState>,
    id: i64,
    status: String,
) -> Result<task::Task, String> {
    task::set_status(&state.db.pool, id, &status)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn delete_task(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    task::delete(&state.db.pool, id).await.map_err(err)
}
