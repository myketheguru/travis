//! Canonical event log per conversation. v0.17.0 substrate.
//!
//! Every observable thing that happens during a turn writes one event
//! row. The list of events for a conversation is the ground truth;
//! the existing `conversation_message` table stays as the UI read
//! path for now (dual-write) and a future slice can flip reads to
//! project from events.
//!
//! Why an event log:
//! - **Branching / time-travel.** `parent_event_id` lets us fork a
//!   conversation at any point without losing the original chain.
//! - **Condenser pattern.** A long-running case can collapse old
//!   events into a `CondensationEvent` while preserving the raw log
//!   on disk. See [`condenser`].
//! - **Reasoning vs action UI.** Each agent turn is classified
//!   ([`ResponseKind`]); the chat surface renders reasoning-only
//!   turns as a distinct card.
//! - **Multi-day case state.** Cases that span days/weeks can replay
//!   events on resume instead of re-querying the LLM.
//!
//! See PLANS / BRAIN.md → "Event log substrate" for the long-form
//! discipline note.

pub mod condenser;

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

/// Discriminant for [`Event::kind`]. Stored as TEXT in the table so
/// new kinds are additive — readers that don't recognise a kind
/// should ignore the event rather than fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    /// Raw text the user sent. `payload_json` carries any attached
    /// document ids and metadata.
    UserMessage,
    /// Final assistant turn shown to the user. `payload_json` carries
    /// the [`AgentResponsePayload`] classification.
    AgentResponse,
    /// One tool call the worker dispatched mid-turn. `payload_json`
    /// carries name + serialized input.
    ToolCall,
    /// Result the tool returned. `payload_json` carries the raw
    /// result string and the originating tool_call event id.
    ToolResult,
    /// One extended-thinking block. `payload_json` carries the
    /// thinking text (private to Travis; never shown verbatim
    /// outside the chat surface).
    Thinking,
    /// A summary that replaces older events in the LLM-visible view
    /// while the original events remain on disk. Written by
    /// [`condenser::condense`].
    Condensation,
    /// Something went wrong. `payload_json` carries kind + message.
    /// Mirrors the existing `error_event` table for new code that's
    /// already on the event substrate.
    Error,
}

impl EventKind {
    pub fn as_str(self) -> &'static str {
        match self {
            EventKind::UserMessage => "user_message",
            EventKind::AgentResponse => "agent_response",
            EventKind::ToolCall => "tool_call",
            EventKind::ToolResult => "tool_result",
            EventKind::Thinking => "thinking",
            EventKind::Condensation => "condensation",
            EventKind::Error => "error",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "user_message" => EventKind::UserMessage,
            "agent_response" => EventKind::AgentResponse,
            "tool_call" => EventKind::ToolCall,
            "tool_result" => EventKind::ToolResult,
            "thinking" => EventKind::Thinking,
            "condensation" => EventKind::Condensation,
            "error" => EventKind::Error,
            _ => return None,
        })
    }
}

/// Classification of an agent turn for UI rendering. Lives on
/// [`AgentResponsePayload`] and is mirrored onto the dual-written
/// `conversation_message.response_kind` so the chat surface can
/// render the distinct reasoning card without joining back to
/// `event`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResponseKind {
    /// The worker delivered a finished extraction — invoice ready,
    /// document drafted, question answered. The typical case.
    Extraction,
    /// Text-only response, no extraction yet. E.g. "I need the PO
    /// doc to fill in WO #" or a clarifying question.
    TextResponse,
    /// The worker emitted thinking blocks + planning text but never
    /// called any tool. Surfaces as a distinct card.
    ReasoningOnly,
}

impl ResponseKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ResponseKind::Extraction => "extraction",
            ResponseKind::TextResponse => "text_response",
            ResponseKind::ReasoningOnly => "reasoning_only",
        }
    }
}

/// Payload schema for [`EventKind::AgentResponse`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentResponsePayload {
    pub response_kind: ResponseKind,
    /// Number of thinking blocks the worker emitted this turn.
    /// Drives the "reasoning depth" indicator in the chat surface.
    pub thinking_blocks: usize,
    /// Number of tool calls the worker dispatched. Zero ⇒
    /// reasoning-only or text-only.
    pub tool_calls: usize,
    /// Number of agent-loop iterations consumed. Diagnostic.
    pub iterations: usize,
}

/// One row of the `event` table.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub id: i64,
    pub conversation_id: i64,
    pub kind: String,
    pub payload_json: Option<String>,
    pub parent_event_id: Option<i64>,
    pub message_id: Option<i64>,
    pub created_at: String,
}

/// Append an event. Returns the new event's id.
pub async fn append(
    pool: &SqlitePool,
    conversation_id: i64,
    kind: EventKind,
    payload: Option<&serde_json::Value>,
    parent_event_id: Option<i64>,
    message_id: Option<i64>,
) -> anyhow::Result<i64> {
    let payload_str = match payload {
        Some(v) => Some(v.to_string()),
        None => None,
    };
    let id = sqlx::query(
        "INSERT INTO conversation_event (conversation_id, kind, payload_json, parent_event_id, message_id)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(conversation_id)
    .bind(kind.as_str())
    .bind(payload_str)
    .bind(parent_event_id)
    .bind(message_id)
    .execute(pool)
    .await?
    .last_insert_rowid();
    Ok(id)
}

/// Best-effort append — logs and swallows errors so the caller (the
/// agent loop) never fails because of an event-log write hiccup.
pub async fn append_or_warn(
    pool: &SqlitePool,
    conversation_id: i64,
    kind: EventKind,
    payload: Option<&serde_json::Value>,
    parent_event_id: Option<i64>,
    message_id: Option<i64>,
) -> Option<i64> {
    match append(pool, conversation_id, kind, payload, parent_event_id, message_id).await {
        Ok(id) => Some(id),
        Err(e) => {
            tracing::warn!("event::append({}): {e}", kind.as_str());
            None
        }
    }
}

/// All events for a conversation in id order. Use [`list_after`] when
/// streaming new events to an open chat surface.
pub async fn list_for_conversation(
    pool: &SqlitePool,
    conversation_id: i64,
) -> anyhow::Result<Vec<Event>> {
    Ok(sqlx::query_as::<_, Event>(
        "SELECT id, conversation_id, kind, payload_json, parent_event_id, message_id, created_at
         FROM conversation_event WHERE conversation_id = ?1 ORDER BY id ASC",
    )
    .bind(conversation_id)
    .fetch_all(pool)
    .await?)
}

/// Events for a conversation strictly after a given event id, in id
/// order. The natural query for "tail the log from here" UIs.
pub async fn list_after(
    pool: &SqlitePool,
    conversation_id: i64,
    after_id: i64,
) -> anyhow::Result<Vec<Event>> {
    Ok(sqlx::query_as::<_, Event>(
        "SELECT id, conversation_id, kind, payload_json, parent_event_id, message_id, created_at
         FROM conversation_event WHERE conversation_id = ?1 AND id > ?2 ORDER BY id ASC",
    )
    .bind(conversation_id)
    .bind(after_id)
    .fetch_all(pool)
    .await?)
}

/// Classify an agent turn from its observable shape. Used by the
/// agent loop to stamp `AgentResponsePayload::response_kind` and to
/// populate `conversation_message.response_kind` for the UI.
pub fn classify_response(
    final_text: &str,
    thinking_blocks: usize,
    tool_calls: usize,
    finalized: bool,
) -> ResponseKind {
    // A successful extraction is the strongest signal — anything
    // else after that is just retry/manager bookkeeping.
    if finalized {
        return ResponseKind::Extraction;
    }
    // No tool calls AND substantive thinking ⇒ the worker was
    // reasoning out loud without acting. Cap matters because a
    // 1-block trivial response shouldn't be a "reasoning card".
    if tool_calls == 0 && thinking_blocks >= 1 && final_text.trim().len() > 80 {
        return ResponseKind::ReasoningOnly;
    }
    ResponseKind::TextResponse
}