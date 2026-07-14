//! Tauri commands wrapping the T2T secure file transfer module.
//!
//! Every command is thin — most of the work lives in
//! `crate::t2t_transfer` proper so it stays unit-testable.

use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};

use super::InboxFile;
use crate::t2t_transfer;
use crate::AppState;

fn inbox_dir(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir().join("travis-fallback"))
        .join("t2t-inbox")
}

/// One-shot: publish this machine's static X25519 pubkey to the cloud.
/// Idempotent. Front-end can call once on sign-in and forget.
#[tauri::command]
pub async fn t2t_publish_pubkey(state: State<'_, AppState>) -> Result<(), String> {
    t2t_transfer::publish_my_pubkey(&state.http)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn t2t_send_file(
    state: State<'_, AppState>,
    peer_id: String,
    file_path: String,
) -> Result<String, String> {
    let path = std::path::Path::new(&file_path).to_path_buf();
    if !path.exists() {
        return Err(format!("file does not exist: {}", path.display()));
    }
    t2t_transfer::send_file(&state.http, &peer_id, &path)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn t2t_poll_inbox(
    state: State<'_, AppState>,
) -> Result<Vec<InboxFile>, String> {
    t2t_transfer::poll_inbox(&state.http)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn t2t_receive_file(
    app: AppHandle,
    state: State<'_, AppState>,
    transfer_id: String,
) -> Result<String, String> {
    let dir = inbox_dir(&app);
    let dest = t2t_transfer::download_and_decrypt(&state.http, &transfer_id, &dir)
        .await
        .map_err(|e| e.to_string())?;
    Ok(dest.to_string_lossy().to_string())
}
