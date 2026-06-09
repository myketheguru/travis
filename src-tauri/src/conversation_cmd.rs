use tauri::State;

use crate::conversation::{self, Conversation, ConversationFilter, ConversationMessage, Thread};
use crate::AppState;

#[tauri::command]
pub async fn list_conversations(
    state: State<'_, AppState>,
    filter: Option<ConversationFilter>,
    limit: Option<i64>,
) -> Result<Vec<Conversation>, String> {
    let visible = state.workspace.read().await.visible_ids.clone();
    conversation::list(
        &state.db.pool,
        &visible,
        filter.unwrap_or_default(),
        limit.unwrap_or(50),
    )
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_thread(
    state: State<'_, AppState>,
    conversation_id: i64,
) -> Result<Thread, String> {
    let conv = conversation::fetch(&state.db.pool, conversation_id)
        .await
        .map_err(|e| e.to_string())?;
    let msgs = conversation::messages(&state.db.pool, conversation_id, None)
        .await
        .map_err(|e| e.to_string())?;
    Ok(Thread {
        conversation: conv,
        messages: msgs,
    })
}

/// v0.18.2 — paginate older messages on scroll-up. Returns at most
/// `limit` (default 50, max 200) messages whose ids are strictly less
/// than `before_id`, in chat order (ASC). An empty array means
/// nothing earlier exists.
#[tauri::command]
pub async fn load_more_messages(
    state: State<'_, AppState>,
    conversation_id: i64,
    before_id: i64,
    limit: Option<i64>,
) -> Result<Vec<ConversationMessage>, String> {
    conversation::messages_before(
        &state.db.pool,
        conversation_id,
        before_id,
        limit.unwrap_or(50),
    )
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn active_conversation(
    state: State<'_, AppState>,
) -> Result<Option<Thread>, String> {
    let visible = state.workspace.read().await.visible_ids.clone();
    let conv = conversation::most_recent_awaiting_user(&state.db.pool, &visible)
        .await
        .map_err(|e| e.to_string())?;
    if let Some(c) = conv {
        let msgs = conversation::messages(&state.db.pool, c.id, None)
            .await
            .map_err(|e| e.to_string())?;
        Ok(Some(Thread {
            conversation: c,
            messages: msgs,
        }))
    } else {
        Ok(None)
    }
}

#[tauri::command]
pub async fn resolve_conversation(
    state: State<'_, AppState>,
    id: i64,
) -> Result<(), String> {
    conversation::set_status(&state.db.pool, id, "resolved")
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn append_user_message(
    state: State<'_, AppState>,
    conversation_id: i64,
    content: String,
) -> Result<ConversationMessage, String> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Err("empty message".into());
    }
    conversation::append(&state.db.pool, conversation_id, "user", trimmed, None)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_message_and_after(
    state: State<'_, AppState>,
    conversation_id: i64,
    message_id: i64,
) -> Result<u64, String> {
    conversation::delete_message_and_after(&state.db.pool, conversation_id, message_id)
        .await
        .map_err(|e| e.to_string())
}
