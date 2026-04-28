use serde_json::Value;
use tauri::State;

use crate::flags;
use crate::AppState;

#[tauri::command]
pub async fn refresh_flags(state: State<'_, AppState>) -> Result<Value, String> {
    flags::refresh(&state.db.pool, &state.http)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_flags(state: State<'_, AppState>) -> Result<Value, String> {
    flags::cached(&state.db.pool)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_flag(
    state: State<'_, AppState>,
    name: String,
) -> Result<Option<Value>, String> {
    flags::get_flag(&state.db.pool, &name)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_flags_url(state: State<'_, AppState>, url: String) -> Result<(), String> {
    flags::set_flags_url(&state.db.pool, &url)
        .await
        .map_err(|e| e.to_string())
}
