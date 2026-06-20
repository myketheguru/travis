//! Tauri commands exposing the cloud client to the React frontend.

use serde::Serialize;
use tauri::State;

use crate::AppState;

use super::{clear_jwt, read_jwt, sign_in_with_google, ByokEvent, CloudClient, CloudUser};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudStatus {
    pub signed_in: bool,
    /// Populated when signed_in == true and we could fetch /auth/me.
    pub user: Option<CloudUser>,
    /// Populated when signed_in == true but the stored JWT was rejected.
    /// The frontend should treat this like "signed out" and prompt sign-in.
    pub invalid_token: bool,
}

/// Check whether a session is active and the JWT is still accepted by
/// the backend. Called at app launch + whenever we want to gate the UI.
#[tauri::command]
pub async fn cloud_status(state: State<'_, AppState>) -> Result<CloudStatus, String> {
    let Some(client) = CloudClient::current(state.http.clone()) else {
        return Ok(CloudStatus { signed_in: false, user: None, invalid_token: false });
    };
    match client.me().await {
        Ok(user) => Ok(CloudStatus { signed_in: true, user: Some(user), invalid_token: false }),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("unauthorized") {
                // Stale JWT. Clear it so subsequent calls start clean.
                let _ = clear_jwt();
                Ok(CloudStatus { signed_in: false, user: None, invalid_token: true })
            } else {
                Err(format!("cloud unreachable: {msg}"))
            }
        }
    }
}

/// Drive the full Google sign-in flow. Blocks until the user finishes
/// in their browser (capped at 5 minutes by the loopback listener).
#[tauri::command]
pub async fn cloud_sign_in_with_google(
    state: State<'_, AppState>,
) -> Result<CloudUser, String> {
    sign_in_with_google(&state.http)
        .await
        .map_err(|e| e.to_string())
}

/// Sign out. Tells the backend to revoke the token, then drops the
/// local copy. The local drop happens even if the backend round-trip
/// fails — a user who clicks "sign out" expects to be signed out.
#[tauri::command]
pub async fn cloud_sign_out(state: State<'_, AppState>) -> Result<(), String> {
    if let Some(client) = CloudClient::current(state.http.clone()) {
        if let Err(e) = client.signout().await {
            tracing::warn!("cloud signout backend call failed: {e}");
        }
    }
    clear_jwt().map_err(|e| e.to_string())?;
    Ok(())
}

/// Fetch the current tier policy + usage. The frontend uses this to
/// hide disallowed models from pickers and to render the daily usage
/// bar.
#[tauri::command]
pub async fn cloud_policy(
    state: State<'_, AppState>,
) -> Result<super::CloudPolicy, String> {
    let client = CloudClient::current(state.http.clone())
        .ok_or_else(|| "not signed in".to_string())?;
    client.policy().await.map_err(|e| e.to_string())
}

/// Record a BYOK LLM call. Best-effort — failures are logged but the
/// command itself returns Ok because the caller doesn't want to fail
/// the user's workflow over an analytics ping.
#[tauri::command]
pub async fn cloud_record_byok(
    state: State<'_, AppState>,
    event: ByokEvent,
) -> Result<(), String> {
    let Some(client) = CloudClient::current(state.http.clone()) else {
        // BYOK without identity is a v1 holdover; v2 requires sign-in.
        // Until the rest of the app catches up we silently drop these
        // rather than fail the LLM call.
        return Ok(());
    };
    if let Err(e) = client.record_byok_event(event).await {
        tracing::warn!("byok event report failed: {e}");
    }
    Ok(())
}

/// Returns whether a JWT is present in the keychain at all, without
/// hitting the network. Useful for UI gating during launch before the
/// first /auth/me round-trip completes.
#[tauri::command]
pub fn cloud_has_token() -> bool {
    read_jwt().is_some()
}
