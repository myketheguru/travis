//! Tauri commands for Travis-to-Travis (task 311).
//!
//! Front-end wrappers over `cloud::t2t`. Every fn requires the user to
//! be signed in; if not, the cloud request errors and the UI shows the
//! sign-in prompt.

use tauri::State;

use crate::cloud::t2t::{self, Relationship, T2tQuery};
use crate::AppState;

#[tauri::command]
pub async fn t2t_list_relationships(
    state: State<'_, AppState>,
) -> Result<Vec<Relationship>, String> {
    t2t::list_relationships(&state.http)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn t2t_invite(
    state: State<'_, AppState>,
    email: String,
    reason: Option<String>,
) -> Result<String, String> {
    t2t::invite_relationship(&state.http, &email, reason.as_deref())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn t2t_accept(state: State<'_, AppState>, id: String) -> Result<(), String> {
    t2t::accept_relationship(&state.http, &id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn t2t_revoke(
    state: State<'_, AppState>,
    id: String,
    reason: Option<String>,
) -> Result<(), String> {
    t2t::revoke_relationship(&state.http, &id, reason.as_deref())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn t2t_send_query(
    state: State<'_, AppState>,
    to_user_id: String,
    question: String,
    from_conversation_id: Option<String>,
    expires_after_days: Option<u32>,
) -> Result<String, String> {
    t2t::send_query(
        &state.http,
        &to_user_id,
        &question,
        from_conversation_id.as_deref(),
        expires_after_days,
    )
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn t2t_inbox(state: State<'_, AppState>) -> Result<Vec<T2tQuery>, String> {
    t2t::inbox(&state.http).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn t2t_outbox(state: State<'_, AppState>) -> Result<Vec<T2tQuery>, String> {
    t2t::outbox(&state.http).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn t2t_draft_reply(
    state: State<'_, AppState>,
    id: String,
    drafted_response: String,
) -> Result<(), String> {
    t2t::draft_reply(&state.http, &id, &drafted_response)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn t2t_approve_reply(
    state: State<'_, AppState>,
    id: String,
    final_response: Option<String>,
) -> Result<(), String> {
    t2t::approve_reply(&state.http, &id, final_response.as_deref())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn t2t_decline_reply(
    state: State<'_, AppState>,
    id: String,
    reason: Option<String>,
) -> Result<(), String> {
    t2t::decline_reply(&state.http, &id, reason.as_deref())
        .await
        .map_err(|e| e.to_string())
}
