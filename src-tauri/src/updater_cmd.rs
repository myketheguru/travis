use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_updater::UpdaterExt;

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub notes: Option<String>,
    pub date: Option<String>,
}

/// Library-callable version of [`check_for_update`] — same logic, just
/// returns `anyhow::Result` so it composes cleanly inside the
/// background polling task in `lib.rs::setup`. Tauri-command callers
/// should still go through [`check_for_update`].
pub async fn silent_check(app: &AppHandle) -> anyhow::Result<Option<UpdateInfo>> {
    let updater = app
        .updater()
        .map_err(|e| anyhow::anyhow!("updater not configured: {e}"))?;
    let maybe_update = updater
        .check()
        .await
        .map_err(|e| anyhow::anyhow!("update check failed: {e}"))?;
    Ok(maybe_update.map(|u| UpdateInfo {
        version: u.version.clone(),
        notes: u.body.clone(),
        date: u.date.map(|d| d.to_string()),
    }))
}

/// Probe the configured updater endpoint(s). Returns `Some(UpdateInfo)` when a
/// newer version is available, `None` otherwise. Errors are surfaced as `Err`
/// strings rather than panics.
#[tauri::command]
pub async fn check_for_update(app: AppHandle) -> Result<Option<UpdateInfo>, String> {
    silent_check(&app).await.map_err(|e| e.to_string())
}

/// Download + install the pending update, then restart the app. If no update
/// is available this is a no-op (returns Ok). The app will only restart on
/// successful install.
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    let Some(update) = updater.check().await.map_err(|e| e.to_string())? else {
        return Ok(());
    };

    update
        .download_and_install(|_chunk, _total| {}, || {})
        .await
        .map_err(|e| e.to_string())?;

    app.restart();
}
