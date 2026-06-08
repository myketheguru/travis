//! Error observability.
//!
//! User-facing problem (v0.15.4): when an LLM call fails, a tool
//! input fails to parse, or the agent loop hits its iteration cap,
//! the chat shows a generic "Travis hit an error" message and the
//! underlying cause goes to `tracing::warn!` — which the user can
//! only see by opening devtools.
//!
//! Fix: persist structured rows to `error_event` whenever a
//! fail-soft path fires, and expose them via a Tauri command so
//! a Diagnostics UI can list / copy them for bug reports.
//!
//! Best-effort: writing the error row must never propagate further
//! errors. If the DB write fails, we just trace and move on — the
//! chat already surfaced something to the user.

use serde::Serialize;
use sqlx::SqlitePool;

#[derive(Debug, Clone, Copy)]
pub enum ErrorKind {
    /// HTTP error from the LLM provider (Anthropic 4xx/5xx).
    LlmApi,
    /// Model returned text/json the agent loop couldn't parse into
    /// an Extraction.
    Parse,
    /// Agent loop hit MAX_ITER without finalizing.
    IterCap,
    /// A read-only or write tool failed during the agent loop.
    ToolCall,
    /// Background capture pipeline failed.
    CaptureBg,
    /// Anything else.
    Other,
}

impl ErrorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorKind::LlmApi => "llm_api",
            ErrorKind::Parse => "parse",
            ErrorKind::IterCap => "iter_cap",
            ErrorKind::ToolCall => "tool_call",
            ErrorKind::CaptureBg => "capture_bg",
            ErrorKind::Other => "other",
        }
    }
}

/// Record an error event. Fire-and-forget: if persistence itself
/// fails we just log it, never propagate.
pub async fn record_error(
    pool: &SqlitePool,
    conversation_id: Option<i64>,
    kind: ErrorKind,
    source: &str,
    message: impl Into<String>,
    detail: Option<serde_json::Value>,
) {
    let message = message.into();
    let detail_json = detail.map(|d| d.to_string());
    let result = sqlx::query(
        "INSERT INTO error_event
            (conversation_id, kind, message, detail_json, source)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(conversation_id)
    .bind(kind.as_str())
    .bind(&message)
    .bind(detail_json.as_deref())
    .bind(source)
    .execute(pool)
    .await;
    if let Err(e) = result {
        tracing::warn!(
            "diagnostics::record_error failed to persist: {e} (orig: {kind:?} from {source}: {message})"
        );
    } else {
        tracing::info!(
            "diagnostics: recorded error event kind={} source={} msg={}",
            kind.as_str(),
            source,
            message
        );
    }
}

/// One row shaped for the Diagnostics UI list.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ErrorRow {
    pub id: i64,
    pub conversation_id: Option<i64>,
    pub kind: String,
    pub message: String,
    pub detail_json: Option<String>,
    pub source: Option<String>,
    pub created_at: String,
}

/// Tauri command: most-recent errors, newest first.
#[tauri::command]
pub async fn list_recent_errors(
    state: tauri::State<'_, crate::AppState>,
    limit: Option<i64>,
) -> Result<Vec<ErrorRow>, String> {
    let lim = limit.unwrap_or(50).clamp(1, 500);
    sqlx::query_as::<_, ErrorRow>(
        "SELECT id, conversation_id, kind, message, detail_json, source, created_at
         FROM error_event
         ORDER BY id DESC
         LIMIT ?1",
    )
    .bind(lim)
    .fetch_all(&state.db.pool)
    .await
    .map_err(|e| e.to_string())
}

/// Tauri command: wipe the error log (user-triggered cleanup).
#[tauri::command]
pub async fn clear_error_log(
    state: tauri::State<'_, crate::AppState>,
) -> Result<u64, String> {
    let res = sqlx::query("DELETE FROM error_event")
        .execute(&state.db.pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(res.rows_affected())
}
