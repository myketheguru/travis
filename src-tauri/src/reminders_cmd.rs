use tauri::State;

use crate::reminders::{self, Reminder, ReminderFilter, ReminderInput};
use crate::AppState;

fn err(e: reminders::ReminderError) -> String {
    e.to_string()
}

#[tauri::command]
pub async fn list_reminders(
    state: State<'_, AppState>,
    filter: Option<ReminderFilter>,
) -> Result<Vec<Reminder>, String> {
    let visible = state.workspace.read().await.visible_ids.clone();
    reminders::list(&state.db.pool, &visible, filter.unwrap_or_default())
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn create_reminder(
    state: State<'_, AppState>,
    input: ReminderInput,
) -> Result<Reminder, String> {
    let ws_id = state.workspace.read().await.active_id;
    reminders::upsert(&state.db.pool, ws_id, input).await.map_err(err)
}

#[tauri::command]
pub async fn dismiss_reminder(
    state: State<'_, AppState>,
    id: i64,
) -> Result<Reminder, String> {
    reminders::dismiss(&state.db.pool, id).await.map_err(err)
}

#[tauri::command]
pub async fn delete_reminder(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    reminders::delete(&state.db.pool, id).await.map_err(err)
}
