//! Tauri commands for Sentry — start/stop capture + query status.

use tauri::State;
use tokio::sync::Mutex;
use tokio::sync::OnceCell;

use crate::sentry::SentryState;
use crate::AppState;

/// Singleton wraps the Sentry state. `Mutex` because state.start /
/// state.stop mutate the JoinHandle in place.
static SENTRY: OnceCell<Mutex<SentryState>> = OnceCell::const_new();

async fn get_or_init(state: &AppState) -> &Mutex<SentryState> {
    SENTRY
        .get_or_init(|| async { Mutex::new(SentryState::new(state.http.clone())) })
        .await
}

const META_KEY: &str = "sentry.app_window.enabled";

async fn read_local_enabled(state: &AppState) -> bool {
    state
        .db
        .meta(META_KEY)
        .await
        .ok()
        .flatten()
        .map(|v| v == "1")
        .unwrap_or(false)
}

async fn write_local_enabled(state: &AppState, enabled: bool) -> Result<(), String> {
    state
        .db
        .set_meta(META_KEY, if enabled { "1" } else { "0" })
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn sentry_status(state: State<'_, AppState>) -> Result<SentryStatus, String> {
    let enabled = read_local_enabled(&state).await;
    let sentry = get_or_init(&state).await;
    let s = sentry.lock().await;
    let buffered = s.buffered_count().await;
    Ok(SentryStatus { enabled, buffered })
}

#[tauri::command]
pub async fn sentry_set_enabled(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    write_local_enabled(&state, enabled).await?;
    let sentry = get_or_init(&state).await;
    let mut s = sentry.lock().await;
    if enabled {
        s.start();
    } else {
        s.stop();
    }
    Ok(())
}

/// Called on app startup — resumes capture if the meta flag says so.
pub async fn resume_if_enabled(state: &AppState) {
    if !read_local_enabled(state).await {
        return;
    }
    let sentry = get_or_init(state).await;
    let mut s = sentry.lock().await;
    s.start();
}

#[derive(Debug, serde::Serialize)]
pub struct SentryStatus {
    pub enabled: bool,
    pub buffered: usize,
}
