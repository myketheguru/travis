//! Tauri commands for the lazy Python bootstrap (v0.22.10).
//!
//! The frontend calls `python_runtime_status` once on app launch to
//! decide whether to show the loader. When the user touches a feature
//! that needs Python, `python_runtime_ensure` kicks off the download
//! + extract + wheel install, emitting `runtime-progress` events for
//! the loader UI.

use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::python_runtime;
use crate::python_runtime::bootstrap::{self, BootstrapHandle};

/// Shared cancellation handle for the in-flight bootstrap (at most one
/// at a time). Lives on AppState. Frontend can call cancel via
/// `python_runtime_cancel` if the user closes the loader.
///
/// Wrapped in Arc<Mutex> so the spawned bootstrap task can re-acquire
/// the lock to clear the slot on completion — tokio::Mutex itself isn't
/// Clone, but Arc<...> is.
pub struct BootstrapState(pub Arc<tokio::sync::Mutex<Option<BootstrapHandle>>>);
impl BootstrapState {
    pub fn new() -> Self {
        Self(Arc::new(tokio::sync::Mutex::new(None)))
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStatus {
    /// True iff `python_runtime::resolve_python_bin` would succeed
    /// right now (cache or bundled).
    pub ready: bool,
    /// True iff the lazy cache exists. Distinguishes "ready via
    /// installer bundle" from "ready via prior bootstrap."
    pub cached: bool,
    /// True iff a bootstrap is currently running. The frontend uses
    /// this to keep the loader on screen across page navigations.
    pub in_progress: bool,
}

#[tauri::command]
pub async fn python_runtime_status(
    app: AppHandle,
    state: State<'_, BootstrapState>,
) -> Result<RuntimeStatus, String> {
    let ready = python_runtime::resolve_python_bin(&app).is_some();
    let cached = bootstrap::cache_python_ready(&app);
    let in_progress = state.0.lock().await.is_some();
    Ok(RuntimeStatus { ready, cached, in_progress })
}

/// Kick off the bootstrap if it isn't already running. Returns
/// immediately — the frontend listens for `runtime-progress` events
/// for the actual updates. Idempotent: if a bootstrap is already
/// running, returns Ok without starting a second one.
#[tauri::command]
pub async fn python_runtime_ensure(
    app: AppHandle,
    state: State<'_, BootstrapState>,
) -> Result<(), String> {
    if python_runtime::resolve_python_bin(&app).is_some() {
        // Already ready — emit a synthetic ready event so any UI
        // observer that just subscribed sees the steady state.
        return Ok(());
    }

    let mut guard = state.0.lock().await;
    if guard.is_some() {
        // Another caller already started the bootstrap.
        return Ok(());
    }
    let handle = BootstrapHandle::default();
    *guard = Some(handle.clone());
    drop(guard);

    let app_clone = app.clone();
    // Arc clone — tokio Mutex isn't Clone, but Arc is. The spawned
    // task re-acquires the lock when it's done to clear the slot.
    let state_arc: Arc<tokio::sync::Mutex<Option<BootstrapHandle>>> = state.0.clone();
    tauri::async_runtime::spawn(async move {
        let result = bootstrap::ensure_ready(&app_clone, handle).await;
        if let Err(e) = &result {
            tracing::warn!("python bootstrap failed: {e}");
        }
        // Clear the in-progress slot whether the bootstrap succeeded
        // or failed. A failure leaves the cache half-built; ensure_ready
        // wipes the dir before retry, so it self-heals.
        *state_arc.lock().await = None;
    });
    Ok(())
}

/// Cancel the in-flight bootstrap. Useful if the user closes the loader
/// modal before the download completes. The bootstrap will exit at the
/// next checkpoint (chunk boundary or extraction entry).
#[tauri::command]
pub async fn python_runtime_cancel(
    state: State<'_, BootstrapState>,
) -> Result<(), String> {
    let mut guard = state.0.lock().await;
    if let Some(handle) = guard.take() {
        handle.cancel();
    }
    Ok(())
}

/// Install (or no-op) a set of additional pip packages. Used by tools
/// that need pip dependencies beyond the preinstalled wheel set.
/// Requires the bootstrap to have completed first.
#[tauri::command]
pub async fn python_runtime_ensure_packages(
    app: AppHandle,
    packages: Vec<String>,
) -> Result<(), String> {
    bootstrap::ensure_packages(&app, &packages).await
}
