use tauri::State;

use crate::domain;
use crate::AppState;

fn err(e: domain::DomainError) -> String {
    e.to_string()
}

#[tauri::command]
pub async fn db_stats(state: State<'_, AppState>) -> Result<domain::Stats, String> {
    domain::stats(&state.db.pool).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_coaches(state: State<'_, AppState>) -> Result<Vec<domain::coach::Coach>, String> {
    let visible = state.workspace.read().await.visible_ids.clone();
    domain::coach::list(&state.db.pool, &visible).await.map_err(err)
}

#[tauri::command]
pub async fn upsert_coach(
    state: State<'_, AppState>,
    input: domain::coach::CoachInput,
) -> Result<domain::coach::Coach, String> {
    let ws_id = state.workspace.read().await.active_id;
    domain::coach::upsert(&state.db.pool, ws_id, input).await.map_err(err)
}

#[tauri::command]
pub async fn delete_coach(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    domain::coach::delete(&state.db.pool, id).await.map_err(err)
}

#[tauri::command]
pub async fn list_schools(state: State<'_, AppState>) -> Result<Vec<domain::school::School>, String> {
    let visible = state.workspace.read().await.visible_ids.clone();
    domain::school::list(&state.db.pool, &visible).await.map_err(err)
}

#[tauri::command]
pub async fn upsert_school(
    state: State<'_, AppState>,
    input: domain::school::SchoolInput,
) -> Result<domain::school::School, String> {
    let ws_id = state.workspace.read().await.active_id;
    domain::school::upsert(&state.db.pool, ws_id, input).await.map_err(err)
}

#[tauri::command]
pub async fn delete_school(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    domain::school::delete(&state.db.pool, id).await.map_err(err)
}

#[tauri::command]
pub async fn list_coach_hours(
    state: State<'_, AppState>,
    filter: Option<domain::coach_hours::CoachHoursFilter>,
) -> Result<Vec<domain::coach_hours::CoachHours>, String> {
    let visible = state.workspace.read().await.visible_ids.clone();
    domain::coach_hours::list(&state.db.pool, &visible, filter.unwrap_or_default())
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn log_coach_hours(
    state: State<'_, AppState>,
    input: domain::coach_hours::CoachHoursInput,
) -> Result<domain::coach_hours::CoachHours, String> {
    let ws_id = state.workspace.read().await.active_id;
    domain::coach_hours::upsert(&state.db.pool, ws_id, input)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn delete_coach_hours(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    domain::coach_hours::delete(&state.db.pool, id).await.map_err(err)
}

#[tauri::command]
pub async fn list_signing_sheets(
    state: State<'_, AppState>,
    filter: Option<domain::signing_sheet::SigningSheetFilter>,
) -> Result<Vec<domain::signing_sheet::SigningSheet>, String> {
    let visible = state.workspace.read().await.visible_ids.clone();
    domain::signing_sheet::list(&state.db.pool, &visible, filter.unwrap_or_default())
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn upsert_signing_sheet(
    state: State<'_, AppState>,
    input: domain::signing_sheet::SigningSheetInput,
) -> Result<domain::signing_sheet::SigningSheet, String> {
    let ws_id = state.workspace.read().await.active_id;
    domain::signing_sheet::upsert(&state.db.pool, ws_id, input)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn delete_signing_sheet(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    domain::signing_sheet::delete(&state.db.pool, id).await.map_err(err)
}

#[tauri::command]
pub async fn list_invoices(
    state: State<'_, AppState>,
    filter: Option<domain::invoice::InvoiceFilter>,
) -> Result<Vec<domain::invoice::Invoice>, String> {
    let visible = state.workspace.read().await.visible_ids.clone();
    domain::invoice::list(&state.db.pool, &visible, filter.unwrap_or_default())
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn upsert_invoice(
    state: State<'_, AppState>,
    input: domain::invoice::InvoiceInput,
) -> Result<domain::invoice::Invoice, String> {
    let ws_id = state.workspace.read().await.active_id;
    domain::invoice::upsert(&state.db.pool, ws_id, input).await.map_err(err)
}

#[tauri::command]
pub async fn transition_invoice(
    state: State<'_, AppState>,
    id: i64,
    status: String,
) -> Result<domain::invoice::Invoice, String> {
    domain::invoice::transition_status(&state.db.pool, id, &status)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn delete_invoice(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    domain::invoice::delete(&state.db.pool, id).await.map_err(err)
}

// Task commands moved to crate::task_cmd (task is core, not L2E).
