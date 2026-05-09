use tauri::State;

use crate::identity::{self, EntityIndex};
use crate::AppState;

#[tauri::command]
pub async fn list_entities(
    state: State<'_, AppState>,
    kind: Option<String>,
    limit: Option<i64>,
) -> Result<Vec<EntityIndex>, String> {
    let limit = limit.unwrap_or(50);
    identity::list_top(&state.db.pool, kind.as_deref(), limit)
        .await
        .map_err(|e| e.to_string())
}


#[tauri::command]
pub async fn get_profile_blurb(state: State<'_, AppState>) -> Result<String, String> {
    let profile = state
        .db
        .user_profile()
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "no user profile yet".to_string())?;
    identity::build_profile_blurb(&state.db.pool, &profile)
        .await
        .map_err(|e| e.to_string())
}
