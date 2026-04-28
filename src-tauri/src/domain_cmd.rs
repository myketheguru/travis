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
    domain::coach::list(&state.db.pool).await.map_err(err)
}

#[tauri::command]
pub async fn upsert_coach(
    state: State<'_, AppState>,
    input: domain::coach::CoachInput,
) -> Result<domain::coach::Coach, String> {
    domain::coach::upsert(&state.db.pool, input).await.map_err(err)
}

#[tauri::command]
pub async fn delete_coach(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    domain::coach::delete(&state.db.pool, id).await.map_err(err)
}

#[tauri::command]
pub async fn list_schools(state: State<'_, AppState>) -> Result<Vec<domain::school::School>, String> {
    domain::school::list(&state.db.pool).await.map_err(err)
}

#[tauri::command]
pub async fn upsert_school(
    state: State<'_, AppState>,
    input: domain::school::SchoolInput,
) -> Result<domain::school::School, String> {
    domain::school::upsert(&state.db.pool, input).await.map_err(err)
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
    domain::coach_hours::list(&state.db.pool, filter.unwrap_or_default())
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn log_coach_hours(
    state: State<'_, AppState>,
    input: domain::coach_hours::CoachHoursInput,
) -> Result<domain::coach_hours::CoachHours, String> {
    domain::coach_hours::upsert(&state.db.pool, input)
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
    domain::signing_sheet::list(&state.db.pool, filter.unwrap_or_default())
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn upsert_signing_sheet(
    state: State<'_, AppState>,
    input: domain::signing_sheet::SigningSheetInput,
) -> Result<domain::signing_sheet::SigningSheet, String> {
    domain::signing_sheet::upsert(&state.db.pool, input)
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
    domain::invoice::list(&state.db.pool, filter.unwrap_or_default())
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn upsert_invoice(
    state: State<'_, AppState>,
    input: domain::invoice::InvoiceInput,
) -> Result<domain::invoice::Invoice, String> {
    domain::invoice::upsert(&state.db.pool, input).await.map_err(err)
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

#[tauri::command]
pub async fn list_tasks(
    state: State<'_, AppState>,
    filter: Option<domain::task::TaskFilter>,
) -> Result<Vec<domain::task::Task>, String> {
    domain::task::list(&state.db.pool, filter.unwrap_or_default())
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn upsert_task(
    state: State<'_, AppState>,
    input: domain::task::TaskInput,
) -> Result<domain::task::Task, String> {
    domain::task::upsert(&state.db.pool, input).await.map_err(err)
}

#[tauri::command]
pub async fn set_task_status(
    state: State<'_, AppState>,
    id: i64,
    status: String,
) -> Result<domain::task::Task, String> {
    domain::task::set_status(&state.db.pool, id, &status)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn delete_task(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    domain::task::delete(&state.db.pool, id).await.map_err(err)
}
