//! Tauri commands for workspace CRUD + active-workspace switching.
//!
//! Switching the active workspace mutates `AppState.workspace`
//! (tokio::sync::RwLock) and emits a `workspace-changed` event so
//! every frontend view that depends on workspace context can
//! refresh. Pack auto-CRUD list views, the splash alerts, the cmd-J
//! overlay's "Captured to <name>" chip — all subscribe.

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::workspaces::{self, Workspace, WorkspaceInput};
use crate::AppState;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceChangeEvent {
    pub active: Workspace,
    pub visible_ids: Vec<i64>,
}

#[tauri::command]
pub async fn list_workspaces(state: State<'_, AppState>) -> Result<Vec<Workspace>, String> {
    workspaces::list_all(&state.db.pool)
        .await
        .map_err(|e| e.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveWorkspaceInfo {
    pub workspace: Workspace,
    pub visible_ids: Vec<i64>,
}

#[tauri::command]
pub async fn get_active_workspace(
    state: State<'_, AppState>,
) -> Result<ActiveWorkspaceInfo, String> {
    let snap = state.workspace.read().await.clone();
    let workspace = workspaces::fetch_one(&state.db.pool, snap.active_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ActiveWorkspaceInfo {
        workspace,
        visible_ids: snap.visible_ids,
    })
}

#[tauri::command]
pub async fn set_active_workspace(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
) -> Result<ActiveWorkspaceInfo, String> {
    let new_state = workspaces::switch_active(&state.db.pool, id)
        .await
        .map_err(|e| e.to_string())?;
    *state.workspace.write().await = new_state.clone();

    let active = workspaces::fetch_one(&state.db.pool, new_state.active_id)
        .await
        .map_err(|e| e.to_string())?;

    let _ = app.emit(
        "workspace-changed",
        WorkspaceChangeEvent {
            active: active.clone(),
            visible_ids: new_state.visible_ids.clone(),
        },
    );

    Ok(ActiveWorkspaceInfo {
        workspace: active,
        visible_ids: new_state.visible_ids,
    })
}

#[tauri::command]
pub async fn create_workspace(
    state: State<'_, AppState>,
    input: WorkspaceInput,
) -> Result<Workspace, String> {
    let new = workspaces::create(&state.db.pool, input)
        .await
        .map_err(|e| e.to_string())?;
    // Recompute visible — the new workspace's cross_visible flag
    // changes the visible set when active is non-sensitive.
    refresh_state(&state).await.map_err(|e| e.to_string())?;
    Ok(new)
}

#[tauri::command]
pub async fn update_workspace(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
    input: WorkspaceInput,
) -> Result<Workspace, String> {
    let updated = workspaces::update(&state.db.pool, id, input)
        .await
        .map_err(|e| e.to_string())?;
    refresh_state(&state).await.map_err(|e| e.to_string())?;

    // If we just edited the active workspace, surface the change so
    // headers / chips update.
    let snap = state.workspace.read().await.clone();
    if snap.active_id == updated.id {
        let _ = app.emit(
            "workspace-changed",
            WorkspaceChangeEvent {
                active: updated.clone(),
                visible_ids: snap.visible_ids,
            },
        );
    }

    Ok(updated)
}

#[tauri::command]
pub async fn archive_workspace(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
) -> Result<Workspace, String> {
    let archived = workspaces::archive(&state.db.pool, id)
        .await
        .map_err(|e| e.to_string())?;

    // If the active workspace just got archived, fall back to the
    // default Personal workspace (id=1) and emit the change.
    let snap = state.workspace.read().await.clone();
    if snap.active_id == id {
        let new_state = workspaces::switch_active(&state.db.pool, 1)
            .await
            .map_err(|e| e.to_string())?;
        *state.workspace.write().await = new_state.clone();
        let active = workspaces::fetch_one(&state.db.pool, new_state.active_id)
            .await
            .map_err(|e| e.to_string())?;
        let _ = app.emit(
            "workspace-changed",
            WorkspaceChangeEvent {
                active,
                visible_ids: new_state.visible_ids,
            },
        );
    } else {
        refresh_state(&state).await.map_err(|e| e.to_string())?;
    }

    Ok(archived)
}

#[tauri::command]
pub async fn unarchive_workspace(
    state: State<'_, AppState>,
    id: i64,
) -> Result<Workspace, String> {
    let restored = workspaces::unarchive(&state.db.pool, id)
        .await
        .map_err(|e| e.to_string())?;
    refresh_state(&state).await.map_err(|e| e.to_string())?;
    Ok(restored)
}

/// Re-derive `AppState.workspace.visible_ids` from the DB. Called
/// after any workspace mutation that could change visibility — new
/// workspace, cross_visible toggle, archive, unarchive.
async fn refresh_state(state: &State<'_, AppState>) -> anyhow::Result<()> {
    let new_state = workspaces::State::load(&state.db.pool).await?;
    *state.workspace.write().await = new_state;
    Ok(())
}
