use tauri::State;

use crate::behavioral::{self, DetectedPattern, EventRow};
use crate::AppState;

#[tauri::command]
pub async fn list_events(
    state: State<'_, AppState>,
    limit: Option<i64>,
    kind: Option<String>,
) -> Result<Vec<EventRow>, String> {
    let limit = limit.unwrap_or(100).clamp(1, 500);
    behavioral::list_events(&state.db.pool, limit, kind.as_deref())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_patterns(state: State<'_, AppState>) -> Result<Vec<DetectedPattern>, String> {
    behavioral::list_patterns(&state.db.pool)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn detect_patterns(state: State<'_, AppState>) -> Result<usize, String> {
    behavioral::detect(&state.db.pool)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn dismiss_pattern(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    sqlx::query(
        "UPDATE detected_pattern SET dismissed_at = CURRENT_TIMESTAMP WHERE id = ?1",
    )
    .bind(id)
    .execute(&state.db.pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}
