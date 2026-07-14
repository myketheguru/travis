//! Tauri commands for BLE (v0.28.51 — btleplug scan).

use crate::ble::{self, BlePeer};

#[tauri::command]
pub async fn ble_scan_peers() -> Result<Vec<BlePeer>, String> {
    Ok(ble::scan_peers().await)
}

#[tauri::command]
pub async fn ble_start_advertise(
    display_name: String,
    user_id: Option<String>,
    public_key: Option<String>,
) -> Result<(), String> {
    ble::start_advertise(&display_name, user_id.as_deref(), public_key.as_deref())
}

#[tauri::command]
pub async fn ble_send_file(peer_instance_id: String, path: String) -> Result<String, String> {
    ble::send_file_ble(&peer_instance_id, &path)
}
