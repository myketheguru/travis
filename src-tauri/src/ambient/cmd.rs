//! Tauri commands for ambient transcript persistence.

use tauri::State;

use crate::AppState;

use super::{recent, insert, AmbientTranscript};

#[tauri::command]
pub async fn ambient_transcript_save(
    state: State<'_, AppState>,
    text: String,
) -> Result<Option<i64>, String> {
    insert(&state.db.pool, &text)
        .await
        .map_err(|e| format!("ambient save: {e}"))
}

#[tauri::command]
pub async fn ambient_transcript_recent(
    state: State<'_, AppState>,
    minutes: Option<i64>,
    limit: Option<i64>,
) -> Result<Vec<AmbientTranscript>, String> {
    recent(&state.db.pool, minutes.unwrap_or(60), limit.unwrap_or(100))
        .await
        .map_err(|e| format!("ambient recent: {e}"))
}
