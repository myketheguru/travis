//! Tauri commands for pack_template.

use tauri::State;

use super::db::{self, PackTemplate, PackTemplateInput};
use crate::AppState;

#[tauri::command]
pub async fn save_pack_template(
    state: State<'_, AppState>,
    input: PackTemplateInput,
) -> Result<PackTemplate, String> {
    let workspace_id = state.workspace.read().await.active_id;
    db::save(&state.db.pool, workspace_id, input)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_pack_templates(
    state: State<'_, AppState>,
    pack_slug: String,
    kind: String,
    counterparty_hint: Option<String>,
) -> Result<Vec<PackTemplate>, String> {
    let workspace_id = state.workspace.read().await.active_id;
    Ok(db::find(
        &state.db.pool,
        workspace_id,
        &pack_slug,
        &kind,
        counterparty_hint.as_deref(),
    )
    .await)
}

#[tauri::command]
pub async fn delete_pack_template(
    state: State<'_, AppState>,
    id: i64,
) -> Result<(), String> {
    db::delete(&state.db.pool, id).await.map_err(|e| e.to_string())
}
