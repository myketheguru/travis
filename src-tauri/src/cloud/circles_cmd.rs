//! Tauri commands for Circles.

use tauri::State;

use crate::cloud::circles::{self, Circle, CircleContact, CircleMember, JoinResult};
use crate::AppState;

#[tauri::command]
pub async fn circles_create(
    state: State<'_, AppState>,
    name: String,
    description: Option<String>,
) -> Result<Circle, String> {
    circles::create_circle(&state.http, &name, description.as_deref())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn circles_list(state: State<'_, AppState>) -> Result<Vec<Circle>, String> {
    circles::list_circles(&state.http)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn circles_join(
    state: State<'_, AppState>,
    code: String,
) -> Result<JoinResult, String> {
    circles::join_circle(&state.http, &code)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn circles_leave(state: State<'_, AppState>, id: String) -> Result<(), String> {
    circles::leave_circle(&state.http, &id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn circles_members(
    state: State<'_, AppState>,
    id: String,
) -> Result<Vec<CircleMember>, String> {
    circles::list_members(&state.http, &id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn circles_contacts(state: State<'_, AppState>) -> Result<Vec<CircleContact>, String> {
    circles::list_contacts(&state.http)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn circles_delete(state: State<'_, AppState>, id: String) -> Result<(), String> {
    circles::delete_circle(&state.http, &id)
        .await
        .map_err(|e| e.to_string())
}
