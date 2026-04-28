use tauri::{AppHandle, State};

use crate::calendar::{self, google, microsoft, ConnectionStatus};
use crate::AppState;

#[tauri::command]
pub async fn calendar_status(
    state: State<'_, AppState>,
) -> Result<ConnectionStatus, String> {
    let account = calendar::fetch_account(&state.db.pool, google::PROVIDER)
        .await
        .map_err(|e| e.to_string())?;
    Ok(calendar::status_for(
        account.as_ref(),
        google::PROVIDER,
        google::is_configured(),
    ))
}

#[tauri::command]
pub async fn calendar_connect_google(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    google::connect(app, &state.db.pool, state.http.clone())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn calendar_disconnect_google(
    state: State<'_, AppState>,
) -> Result<(), String> {
    google::disconnect(&state.db.pool)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn microsoft_status(
    state: State<'_, AppState>,
) -> Result<ConnectionStatus, String> {
    let account = calendar::fetch_account(&state.db.pool, microsoft::PROVIDER)
        .await
        .map_err(|e| e.to_string())?;
    Ok(calendar::status_for(
        account.as_ref(),
        microsoft::PROVIDER,
        microsoft::is_configured(),
    ))
}

#[tauri::command]
pub async fn microsoft_connect(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    microsoft::connect(app, &state.db.pool, state.http.clone())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn microsoft_disconnect(
    state: State<'_, AppState>,
) -> Result<(), String> {
    microsoft::disconnect(&state.db.pool)
        .await
        .map_err(|e| e.to_string())
}
