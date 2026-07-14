//! Tauri commands for Sentry — start/stop capture + query status +
//! snapshot listing.

use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};
use tokio::sync::Mutex;
use tokio::sync::OnceCell;

use crate::sentry::{self, SentrySnapshotInfo, SentryState};
use crate::AppState;

/// Singleton wraps the Sentry state. `Mutex` because state.start /
/// state.stop mutate the JoinHandle in place.
static SENTRY: OnceCell<Mutex<SentryState>> = OnceCell::const_new();

fn snapshot_dir(app: &AppHandle) -> PathBuf {
    // Falls back to a per-process temp dir if the platform path
    // resolver refuses; we'd rather write somewhere than crash Sentry.
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir().join("travis-fallback"))
        .join("sentry-snapshots")
}

async fn get_or_init(state: &AppState, app: &AppHandle) -> &'static Mutex<SentryState> {
    SENTRY
        .get_or_init(|| async {
            let dir = snapshot_dir(app);
            Mutex::new(SentryState::new(state.http.clone(), dir))
        })
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
pub async fn sentry_status(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<SentryStatus, String> {
    let enabled = read_local_enabled(&state).await;
    let sentry = get_or_init(&state, &app).await;
    let s = sentry.lock().await;
    let buffered = s.buffered_count().await;
    let snapshots = sentry::list_snapshots(s.snapshot_dir(), 20);
    let disk_bytes: u64 = snapshots.iter().map(|s| s.bytes).sum();
    Ok(SentryStatus {
        enabled,
        buffered,
        snapshot_count: snapshots.len(),
        snapshot_bytes: disk_bytes,
    })
}

#[tauri::command]
pub async fn sentry_set_enabled(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    write_local_enabled(&state, enabled).await?;
    let sentry = get_or_init(&state, &app).await;
    let http = {
        let s = sentry.lock().await;
        // Clone the shared http client from state so the consent grant
        // runs after we drop the lock — it's a network call that we
        // don't want holding the singleton.
        state.http.clone()
    };
    // Fire-and-forget the cloud consent sync so the local toggle isn't
    // blocked on network. Failures here just mean the server rejects
    // ingest until the next successful sync — which is safe by design.
    tauri::async_runtime::spawn(async move {
        sentry::set_cloud_consent(&http, enabled).await;
    });
    let mut s = sentry.lock().await;
    if enabled {
        s.start();
    } else {
        s.stop();
    }
    Ok(())
}

/// List local snapshots (newest first) so the UI can render a
/// gallery. Returns paths as strings — frontend passes them through
/// `convertFileSrc` to render via the asset protocol.
#[tauri::command]
pub async fn sentry_list_snapshots(
    app: AppHandle,
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> Result<Vec<SentrySnapshotInfo>, String> {
    let sentry = get_or_init(&state, &app).await;
    let s = sentry.lock().await;
    Ok(sentry::list_snapshots(s.snapshot_dir(), limit.unwrap_or(20)))
}

/// Manual "capture now" — useful for QA + for the consent modal's
/// "try it" affordance. Bypasses the sample-loop cadence but still
/// respects the local enabled flag: if Sentry is off, refuses.
#[tauri::command]
pub async fn sentry_capture_now(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<SentrySnapshotInfo, String> {
    if !read_local_enabled(&state).await {
        return Err("Sentry is not enabled — turn it on in Settings first.".into());
    }
    let sentry = get_or_init(&state, &app).await;
    let dir = sentry.lock().await.snapshot_dir().to_path_buf();
    let path = tokio::task::spawn_blocking(move || sentry::capture_and_prune(&dir))
        .await
        .map_err(|e| format!("capture task: {e}"))??;
    let filename = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    Ok(SentrySnapshotInfo {
        path: path.to_string_lossy().to_string(),
        filename,
        captured_at: chrono::Utc::now().to_rfc3339(),
        bytes,
    })
}

/// Called on app startup — resumes capture if the meta flag says so.
pub async fn resume_if_enabled(state: &AppState, app: &AppHandle) {
    if !read_local_enabled(state).await {
        return;
    }
    let sentry = get_or_init(state, app).await;
    let mut s = sentry.lock().await;
    s.start();
}

#[derive(Debug, serde::Serialize)]
pub struct SentryStatus {
    pub enabled: bool,
    pub buffered: usize,
    pub snapshot_count: usize,
    pub snapshot_bytes: u64,
}
