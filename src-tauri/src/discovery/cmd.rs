//! Tauri commands for peer discovery.

use tauri::State;
use tokio::sync::OnceCell;

use crate::discovery::{DiscoveredPeer, DiscoveryState};
use crate::AppState;

/// Lazy-initialized singleton. First call to discovery_peers() starts
/// the daemon; subsequent calls return the current peer map.
static DISCOVERY: OnceCell<DiscoveryState> = OnceCell::const_new();

async fn get_or_init(state: &AppState) -> Result<&DiscoveryState, String> {
    DISCOVERY
        .get_or_try_init(|| async {
            let profile = state
                .db
                .user_profile()
                .await
                .map_err(|e| format!("profile: {e}"))?;
            let name = profile
                .as_ref()
                .map(|p| p.name.clone())
                .unwrap_or_else(|| "Travis user".to_string());
            // Email + user_id are cloud-side and would require a round-trip
            // to fetch — advertising just display name is enough for
            // "someone on the LAN wants to pair with X". The T2T invite
            // flow that fires on click uses the actual cloud email lookup.
            DiscoveryState::start(&name, None, None).map_err(|e| e.to_string())
        })
        .await
}

#[tauri::command]
pub async fn discovery_peers(state: State<'_, AppState>) -> Result<Vec<DiscoveredPeer>, String> {
    let d = get_or_init(&state).await?;
    Ok(d.peers().await)
}

#[tauri::command]
pub async fn discovery_start(state: State<'_, AppState>) -> Result<(), String> {
    let _ = get_or_init(&state).await?;
    Ok(())
}
