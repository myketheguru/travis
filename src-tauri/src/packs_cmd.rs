//! Tauri commands for runtime pack selection.
//!
//! `list_packs` enumerates every compiled-in pack with its current
//! runtime-enabled state. `set_pack_enabled` writes the
//! `meta.pack.<slug>.enabled` flag.
//!
//! Toggling takes effect on the next app launch — action / tool
//! registries and prompt fragments are constructed once during
//! startup. The frontend surfaces a "Restart Travis" hint after a
//! successful toggle.

use serde::Serialize;
use tauri::State;

use crate::packs;
use crate::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackInfo {
    pub slug: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub enabled: bool,
}

#[tauri::command]
pub async fn list_packs(state: State<'_, AppState>) -> Result<Vec<PackInfo>, String> {
    let mut out = Vec::new();
    for pack in packs::compiled_in_packs() {
        let enabled = packs::is_pack_enabled(&state.db.pool, *pack)
            .await
            .map_err(|e| e.to_string())?;
        out.push(PackInfo {
            slug: pack.slug().to_string(),
            name: pack.name().to_string(),
            description: pack.description().to_string(),
            version: pack.version().to_string(),
            enabled,
        });
    }
    Ok(out)
}

#[tauri::command]
pub async fn set_pack_enabled(
    state: State<'_, AppState>,
    slug: String,
    enabled: bool,
) -> Result<(), String> {
    packs::set_pack_enabled(&state.db.pool, &slug, enabled)
        .await
        .map_err(|e| e.to_string())
}
