use tauri::State;

use crate::summary::{self, Summary};
use crate::AppState;

#[tauri::command]
pub async fn list_summaries(
    state: State<'_, AppState>,
    kind: Option<String>,
    limit: Option<i64>,
) -> Result<Vec<Summary>, String> {
    let limit = limit.unwrap_or(50);
    summary::list(&state.db.pool, kind.as_deref(), limit)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn generate_daily_summary(
    state: State<'_, AppState>,
    date: String,
) -> Result<Summary, String> {
    summary::generate_daily(&state, &date)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn generate_weekly_summary(
    state: State<'_, AppState>,
    week_start: String,
) -> Result<Summary, String> {
    summary::generate_weekly(&state, &week_start)
        .await
        .map_err(|e| e.to_string())
}
