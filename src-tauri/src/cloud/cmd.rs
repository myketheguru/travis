//! Tauri commands exposing the cloud client to the React frontend.

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::AppState;

use super::engine::{SyncEngine, SyncRunResult, SyncStatus};
use super::sync::{
    migration_status, record_decision, upload_local, MigrationDetails, MigrationStatus,
};
use super::{
    clear_jwt, device_id, read_jwt, sign_in_with_google, ByokEvent, CloudClient, CloudUser,
    CreateScheduleInput, RunNowInput, WorkflowRun, WorkflowSchedule,
};

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
/// in their browser (capped at 2 minutes by the loopback listener, or
/// when the UI fires cloud_sign_in_cancel).
#[tauri::command]
pub async fn cloud_sign_in_with_google(
    state: State<'_, AppState>,
) -> Result<CloudUser, String> {
    sign_in_with_google(&state.http)
        .await
        .map_err(|e| e.to_string())
}

/// Abort the in-flight sign_in_with_google call. Triggers the cancel
/// notify; the awaiting task drops out of its tokio::select! and the
/// listener port is released. Idempotent — calling when no sign-in is
/// in flight is a no-op.
#[tauri::command]
pub fn cloud_sign_in_cancel() {
    crate::cloud::SIGN_IN_CANCEL.notify_waiters();
}

/// v3 Slice 4 — pick up the existing web session for this user.
/// Opens browser to usetravis.com/app/handoff with a loopback redirect;
/// when the user approves on the web, the cloud sends back a single-use
/// code, the desktop swaps it for a fresh JWT, and the user is in. This
/// is the primary v3 sign-in path; Google-direct stays as fallback.
#[tauri::command]
pub async fn cloud_handoff_from_web(
    state: State<'_, AppState>,
) -> Result<CloudUser, String> {
    crate::cloud::claim_handoff_from_web(&state.http)
        .await
        .map_err(|e| e.to_string())
}

/// Tier 2 — extend the user's Google grant to include inbox + calendar
/// reads. `scopes` is a list like `["gmail", "gcal"]`. Returns the
/// comma-separated list of providers actually enrolled.
#[tauri::command]
pub async fn cloud_extend_google_grant(
    state: State<'_, AppState>,
    scopes: Vec<String>,
) -> Result<String, String> {
    let refs: Vec<&str> = scopes.iter().map(|s| s.as_str()).collect();
    crate::cloud::extend_google_grant(&state.http, &refs)
        .await
        .map_err(|e| e.to_string())
}

/// Tier 3 — update a proposed action's status on a cloud workflow run.
#[tauri::command]
pub async fn cloud_update_action_status(
    state: State<'_, AppState>,
    run_id: String,
    action_id: String,
    status: String,
    payload: Option<serde_json::Value>,
) -> Result<(), String> {
    crate::cloud::update_action_status(&state.http, &run_id, &action_id, &status, payload)
        .await
        .map_err(|e| e.to_string())
}

/// Tier 3 — execute an approved draft_reply: send the email via the
/// user's local Gmail OAuth, then PATCH the cloud run row marking the
/// action 'executed'. Failures on the send step are surfaced; the
/// action stays 'approved' (not 'executed') so the user can retry.
#[tauri::command]
pub async fn cloud_action_execute_draft_reply(
    state: State<'_, AppState>,
    run_id: String,
    action_id: String,
    to: String,
    subject: String,
    body: String,
) -> Result<(), String> {
    crate::email::gmail::send(
        &state.db.pool,
        &state.http,
        &to,
        &subject,
        &body,
        Some("workflow_action"),
        Some("workflow_run"),
        None,
    )
    .await
    .map_err(|e| e.to_string())?;
    crate::cloud::update_action_status(
        &state.http,
        &run_id,
        &action_id,
        "executed",
        None,
    )
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Tier 2 — list the user's connected accounts (Gmail / Calendar /
/// Outlook). Surfaced in Settings so the user can see what Travis can
/// reach + revoke.
#[tauri::command]
pub async fn cloud_connected_accounts(
    state: State<'_, AppState>,
) -> Result<Vec<crate::cloud::ConnectedAccount>, String> {
    crate::cloud::connected_accounts(&state.http)
        .await
        .map_err(|e| e.to_string())
}

/// Tier 2 — revoke a connected account by provider. Server marks
/// is_active=0; the WorkflowLoop stops reading from it on next tick.
#[tauri::command]
pub async fn cloud_disconnect_account(
    state: State<'_, AppState>,
    provider: String,
) -> Result<(), String> {
    crate::cloud::disconnect_account(&state.http, &provider)
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

// --- v2 Phase 2.1 — migration of existing local data --------------------

/// Inspect the migration state + count what we have to migrate. Called
/// from the MigrationPrompt UI before the user picks a path.
#[tauri::command]
pub async fn cloud_migration_status(
    state: State<'_, AppState>,
) -> Result<MigrationStatus, String> {
    migration_status(&state.db).await.map_err(|e| e.to_string())
}

/// User picked "Upload my work" — push the local DB to the cloud.
/// Records `complete` status on success so the prompt doesn't re-appear.
#[tauri::command]
pub async fn cloud_migration_upload(
    state: State<'_, AppState>,
) -> Result<MigrationDetails, String> {
    let host = whoami_device();
    upload_local(state.http.clone(), &state.db, Some(host))
        .await
        .map_err(|e| e.to_string())
}

/// User picked "Start fresh" — local stays untouched, cloud starts
/// empty. Records `fresh` so we don't re-prompt.
#[tauri::command]
pub async fn cloud_migration_start_fresh(
    state: State<'_, AppState>,
) -> Result<(), String> {
    record_decision(&state.db, "fresh", "fresh")
        .await
        .map_err(|e| e.to_string())
}

/// User picked "Skip for now" — records `skipped`. The UI may offer
/// the prompt again from settings; the gate in App.tsx will treat
/// `skipped` as "don't prompt automatically" but Settings can still
/// surface it.
#[tauri::command]
pub async fn cloud_migration_skip(state: State<'_, AppState>) -> Result<(), String> {
    record_decision(&state.db, "skipped", "skipped")
        .await
        .map_err(|e| e.to_string())
}

fn whoami_device() -> String {
    device_id()
}

// --- v2 Phase 2.2 — continuous sync -------------------------------------

/// Trigger an immediate push + pull cycle. Returns the counts so the
/// UI can surface a small confirmation toast. Safe to call frequently —
/// no-ops cleanly if there's nothing to do.
#[tauri::command]
pub async fn cloud_sync_now(state: State<'_, AppState>) -> Result<SyncRunResult, String> {
    if read_jwt().is_none() {
        return Err("not signed in".to_string());
    }
    let engine = SyncEngine::new(state.http.clone(), device_id());
    engine.run_once(&state.db).await.map_err(|e| e.to_string())
}

/// Status snapshot for the Settings sync indicator.
#[tauri::command]
pub async fn cloud_sync_status(state: State<'_, AppState>) -> Result<SyncStatus, String> {
    let engine = SyncEngine::new(state.http.clone(), device_id());
    engine.status(&state.db).await.map_err(|e| e.to_string())
}

// --- v2 Phase 4 — workflow loop --------------------------------------

#[tauri::command]
pub async fn cloud_workflow_schedules(
    state: State<'_, AppState>,
) -> Result<Vec<WorkflowSchedule>, String> {
    let client = CloudClient::current(state.http.clone())
        .ok_or_else(|| "not signed in".to_string())?;
    client.list_schedules().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn cloud_workflow_create_schedule(
    state: State<'_, AppState>,
    input: CreateScheduleInput,
) -> Result<String, String> {
    let client = CloudClient::current(state.http.clone())
        .ok_or_else(|| "not signed in".to_string())?;
    client.create_schedule(input).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn cloud_workflow_delete_schedule(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let client = CloudClient::current(state.http.clone())
        .ok_or_else(|| "not signed in".to_string())?;
    client.delete_schedule(&id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn cloud_workflow_run_now(
    state: State<'_, AppState>,
    input: RunNowInput,
) -> Result<String, String> {
    let client = CloudClient::current(state.http.clone())
        .ok_or_else(|| "not signed in".to_string())?;
    client.run_workflow_now(input).await.map_err(|e| e.to_string())
}

/// v2 Phase 5 — list packs the cloud has authorized for this user.
/// Returns [] for Free/Pro tiers (packs are Org-only). Used by the
/// desktop's pack resolver to filter the bundled pack catalog so a
/// user only sees what an admin granted them.
#[tauri::command]
pub async fn cloud_authorized_packs(
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    let client = CloudClient::current(state.http.clone())
        .ok_or_else(|| "not signed in".to_string())?;
    client.authorized_packs().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn cloud_workflow_runs(
    state: State<'_, AppState>,
    since: Option<String>,
) -> Result<Vec<WorkflowRun>, String> {
    let client = CloudClient::current(state.http.clone())
        .ok_or_else(|| "not signed in".to_string())?;
    client
        .list_runs(since.as_deref())
        .await
        .map_err(|e| e.to_string())
}
