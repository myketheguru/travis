//! v0.20.2 — platform-level Tauri commands for the frontend to ask
//! "what does this build of Travis support?"
//!
//! Currently exposes:
//! - `travis_cloud_available()` — whether the build was compiled with
//!   a Travis Cloud key. The Onboarding flow + Settings panel use this
//!   to know whether the cloud option is selectable.
//! - `previous_llm_provider()` — for users migrated from their own
//!   provider in v0.20.2, returns the prior provider/model strings so
//!   Settings can offer a "switch back" affordance with the right
//!   labels.

use serde::Serialize;
use sqlx::SqlitePool;
use tauri::State;

use crate::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformInfo {
    pub travis_cloud_available: bool,
    pub previous_llm_provider: Option<String>,
    pub previous_model: Option<String>,
}

#[tauri::command]
pub async fn platform_info(state: State<'_, AppState>) -> Result<PlatformInfo, String> {
    let previous_llm_provider = read_meta(&state.db.pool, "previous_llm_provider").await;
    let previous_model = read_meta(&state.db.pool, "previous_model").await;
    Ok(PlatformInfo {
        travis_cloud_available: crate::llm::travis_cloud_available(),
        previous_llm_provider,
        previous_model,
    })
}

async fn read_meta(pool: &SqlitePool, key: &str) -> Option<String> {
    let row: Option<(String,)> = sqlx::query_as("SELECT value FROM meta WHERE key = ?1")
        .bind(key)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();
    row.map(|(v,)| v)
}
