use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Conversation {
    pub id: i64,
    pub kind: String,
    pub title: Option<String>,
    pub status: String,
    pub link_kind: Option<String>,
    pub link_id: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
    pub workspace_id: i64,
    /// v2 Phase 2.4 — stable cross-device identifier. Generated at
    /// create time via SQLite's randomblob(); used by the sync apply
    /// pipeline to match incoming conversation.upsert events against
    /// the local row.
    pub cloud_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ConversationMessage {
    pub id: i64,
    pub conversation_id: i64,
    pub role: String,
    pub content: String,
    pub payload_json: Option<String>,
    pub created_at: String,
    /// v0.17.0 — classification stamped by the agent loop. One of
    /// "extraction" / "text_response" / "reasoning_only", or NULL for
    /// rows written before the column existed. Drives the chat
    /// surface's reasoning-card render.
    pub response_kind: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Thread {
    pub conversation: Conversation,
    pub messages: Vec<ConversationMessage>,
}

/// v0.18.3 — row shape for the conversation switcher. Includes a
/// short preview snippet (first user message, capped at 80 chars)
/// so the dropdown can show "Invoice for IS 217 PO/WO docs..." next
/// to each thread, since most conversations don't have explicit
/// titles.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationListItem {
    pub id: i64,
    pub title: Option<String>,
    pub preview: Option<String>,
    pub status: String,
    pub kind: String,
    pub message_count: i64,
    pub updated_at: String,
    pub created_at: String,
}

pub async fn open(
    pool: &SqlitePool,
    workspace_id: i64,
    kind: &str,
    title: Option<&str>,
) -> Result<Conversation, sqlx::Error> {
    // v2 Phase 2.4 — stamp a cloud_id at create time so the
    // conversation has a stable cross-device identity. Remote devices
    // pulling a conversation.upsert event for this thread will match
    // on this id rather than risking a duplicate insert.
    let id = sqlx::query(
        "INSERT INTO conversation (kind, title, workspace_id, cloud_id)
         VALUES (?1, ?2, ?3, lower(hex(randomblob(16))))",
    )
    .bind(kind)
    .bind(title)
    .bind(workspace_id)
    .execute(pool)
    .await?
    .last_insert_rowid();
    fetch(pool, id).await
}

pub async fn fetch(pool: &SqlitePool, id: i64) -> Result<Conversation, sqlx::Error> {
    sqlx::query_as::<_, Conversation>(
        "SELECT id, kind, title, status, link_kind, link_id, created_at, updated_at, workspace_id, cloud_id
         FROM conversation WHERE id=?1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
}

pub async fn append(
    pool: &SqlitePool,
    conversation_id: i64,
    role: &str,
    content: &str,
    payload_json: Option<&str>,
) -> Result<ConversationMessage, sqlx::Error> {
    append_with_kind(pool, conversation_id, role, content, payload_json, None).await
}

/// v0.17.0 — append a message with an explicit response_kind tag.
/// Agent-loop callers use this to stamp "extraction" / "text_response"
/// / "reasoning_only"; the plain [`append`] above keeps backward
/// compat for callers that don't classify.
pub async fn append_with_kind(
    pool: &SqlitePool,
    conversation_id: i64,
    role: &str,
    content: &str,
    payload_json: Option<&str>,
    response_kind: Option<&str>,
) -> Result<ConversationMessage, sqlx::Error> {
    let id = sqlx::query(
        "INSERT INTO conversation_message (conversation_id, role, content, payload_json, response_kind)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(conversation_id)
    .bind(role)
    .bind(content)
    .bind(payload_json)
    .bind(response_kind)
    .execute(pool)
    .await?
    .last_insert_rowid();

    sqlx::query("UPDATE conversation SET updated_at = CURRENT_TIMESTAMP WHERE id = ?1")
        .bind(conversation_id)
        .execute(pool)
        .await?;

    let result = sqlx::query_as::<_, ConversationMessage>(
        "SELECT id, conversation_id, role, content, payload_json, created_at, response_kind
         FROM conversation_message WHERE id=?1",
    )
    .bind(id)
    .fetch_one(pool)
    .await;

    // v2 Phase 2.3 — enqueue conversation.upsert snapshot so the
    // cloud has the running thread (and other devices will eventually
    // be able to pull it). Best-effort — a failure here doesn't fail
    // the user's message append. If the enqueue dies between message
    // insert and snapshot enqueue, the next append picks up the
    // entire thread state anyway.
    if let Err(e) = enqueue_conversation_snapshot(pool, conversation_id).await {
        tracing::warn!(
            "sync: failed to enqueue conversation.upsert (conv_id={conversation_id}): {e}"
        );
    }

    result
}

/// Build a full conversation snapshot and append it to the sync outbox.
/// Triggered after every message append so the cloud always has the
/// latest state of the thread. Conversations rarely exceed a few
/// hundred messages; sending the whole thing each time keeps the
/// apply contract dead simple (last write wins).
async fn enqueue_conversation_snapshot(
    pool: &SqlitePool,
    conversation_id: i64,
) -> Result<(), sqlx::Error> {
    let conv = sqlx::query_as::<_, Conversation>(
        "SELECT id, kind, title, status, link_kind, link_id, created_at, updated_at, workspace_id, cloud_id
         FROM conversation WHERE id = ?1",
    )
    .bind(conversation_id)
    .fetch_one(pool)
    .await?;
    let messages = sqlx::query_as::<_, ConversationMessage>(
        "SELECT id, conversation_id, role, content, payload_json, created_at, response_kind
         FROM conversation_message WHERE conversation_id = ?1 ORDER BY id",
    )
    .bind(conversation_id)
    .fetch_all(pool)
    .await?;
    let payload = serde_json::json!({
        "cloudId": conv.cloud_id,
        "localId": conv.id,
        "kind": conv.kind,
        "title": conv.title,
        "status": conv.status,
        "workspaceId": conv.workspace_id,
        "createdAt": conv.created_at,
        "updatedAt": conv.updated_at,
        "messages": messages.iter().map(|m| serde_json::json!({
            "role": m.role,
            "content": m.content,
            "payload": m.payload_json.as_ref().and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok()),
            "responseKind": m.response_kind,
            "createdAt": m.created_at,
        })).collect::<Vec<_>>(),
    })
    .to_string();
    sqlx::query("INSERT INTO sync_outbox (kind, payload) VALUES ('conversation.upsert', ?1)")
        .bind(payload)
        .execute(pool)
        .await?;
    Ok(())
}

/// Delete a message and every message that came after it in the
/// thread (id-ordered). Returns the number of rows removed.
///
/// Mirrors Claude.ai's "delete this turn" affordance: removing a turn
/// from the middle of a thread without trimming subsequent turns would
/// leave dangling references in Travis's later replies. Trimming forward
/// keeps the transcript coherent.
///
/// Orphaned step rows are intentionally left in place — they're keyed on
/// conversation_id, not message_id, so the cleanup is conversation-wide,
/// not turn-wide. They become harmless history.
pub async fn delete_message_and_after(
    pool: &SqlitePool,
    conversation_id: i64,
    message_id: i64,
) -> Result<u64, sqlx::Error> {
    let res = sqlx::query(
        "DELETE FROM conversation_message
         WHERE conversation_id = ?1 AND id >= ?2",
    )
    .bind(conversation_id)
    .bind(message_id)
    .execute(pool)
    .await?;
    sqlx::query("UPDATE conversation SET updated_at = CURRENT_TIMESTAMP WHERE id = ?1")
        .bind(conversation_id)
        .execute(pool)
        .await?;
    Ok(res.rows_affected())
}

pub async fn set_status(pool: &SqlitePool, id: i64, status: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE conversation SET status=?1, updated_at=CURRENT_TIMESTAMP WHERE id=?2",
    )
    .bind(status)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn messages(
    pool: &SqlitePool,
    conversation_id: i64,
    limit: Option<i64>,
) -> Result<Vec<ConversationMessage>, sqlx::Error> {
    // v0.18.2 — default returns the MOST RECENT 50 messages, ordered
    // ASC for display. Earlier history loads on demand via
    // `messages_before`. Pass an explicit `limit` to override.
    // SQL pulls last N by id DESC then we reverse to ASC for the
    // chat to render top-down naturally.
    let lim = limit.unwrap_or(50);
    let mut rows = sqlx::query_as::<_, ConversationMessage>(
        "SELECT id, conversation_id, role, content, payload_json, created_at, response_kind
         FROM conversation_message
         WHERE conversation_id = ?1
         ORDER BY id DESC
         LIMIT ?2",
    )
    .bind(conversation_id)
    .bind(lim)
    .fetch_all(pool)
    .await?;
    rows.reverse();
    Ok(rows)
}

/// v0.18.2 — fetch the chunk of older messages strictly before a given
/// id, ordered ASC. Powers the chat surface's scroll-to-load-older
/// pagination. Returns at most `limit` rows; an empty result means
/// there's nothing earlier and the chat can stop offering to load.
pub async fn messages_before(
    pool: &SqlitePool,
    conversation_id: i64,
    before_id: i64,
    limit: i64,
) -> Result<Vec<ConversationMessage>, sqlx::Error> {
    let lim = limit.clamp(1, 200);
    let mut rows = sqlx::query_as::<_, ConversationMessage>(
        "SELECT id, conversation_id, role, content, payload_json, created_at, response_kind
         FROM conversation_message
         WHERE conversation_id = ?1 AND id < ?2
         ORDER BY id DESC
         LIMIT ?3",
    )
    .bind(conversation_id)
    .bind(before_id)
    .bind(lim)
    .fetch_all(pool)
    .await?;
    rows.reverse();
    Ok(rows)
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ConversationFilter {
    pub status: Option<String>,
    pub kind: Option<String>,
}

pub async fn list(
    pool: &SqlitePool,
    visible_ids: &[i64],
    filter: ConversationFilter,
    limit: i64,
) -> Result<Vec<Conversation>, sqlx::Error> {
    let lim = limit.clamp(1, 500);
    let n_ws = visible_ids.len();
    let ws_clause: String = (4..4 + n_ws)
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT id, kind, title, status, link_kind, link_id, created_at, updated_at, workspace_id
         FROM conversation
         WHERE (?1 IS NULL OR status = ?1)
           AND (?2 IS NULL OR kind = ?2)
           AND workspace_id IN ({ws_clause})
         ORDER BY updated_at DESC LIMIT ?3"
    );
    let mut q = sqlx::query_as::<_, Conversation>(&sql)
        .bind(&filter.status)
        .bind(&filter.kind)
        .bind(lim);
    for id in visible_ids {
        q = q.bind(id);
    }
    q.fetch_all(pool).await
}

/// v0.18.3 — list with first-user-message preview snippets for the
/// switcher UI. Optional `query` does a case-insensitive LIKE match
/// against the conversation title AND against the first 4000 chars
/// of any message content (so users can search for "IS 217" or
/// "Wallace Ave" and find the right thread). Newest-first; `limit`
/// defaults to 50.
pub async fn list_for_switcher(
    pool: &SqlitePool,
    visible_ids: &[i64],
    query: Option<&str>,
    limit: i64,
) -> Result<Vec<ConversationListItem>, sqlx::Error> {
    if visible_ids.is_empty() {
        return Ok(Vec::new());
    }
    let lim = limit.clamp(1, 200);
    let n_ws = visible_ids.len();
    // SQL placeholders: ?1 = like pattern (or NULL when no query),
    // ?2 = limit, ?3..?3+n = workspace ids.
    let ws_clause: String = (3..3 + n_ws)
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT
           c.id,
           c.title,
           c.status,
           c.kind,
           c.created_at,
           c.updated_at,
           (SELECT COUNT(*) FROM conversation_message m
              WHERE m.conversation_id = c.id) AS message_count,
           (SELECT m.content FROM conversation_message m
              WHERE m.conversation_id = c.id AND m.role = 'user'
              ORDER BY m.id ASC LIMIT 1) AS preview
         FROM conversation c
         WHERE c.workspace_id IN ({ws_clause})
           AND c.kind != 'nudge'
           AND (
             ?1 IS NULL
             OR LOWER(IFNULL(c.title, '')) LIKE ?1
             OR EXISTS (
                 SELECT 1 FROM conversation_message m
                  WHERE m.conversation_id = c.id
                    AND LOWER(SUBSTR(m.content, 1, 4000)) LIKE ?1
             )
           )
         ORDER BY c.updated_at DESC
         LIMIT ?2"
    );
    let like_pattern = query.map(|q| format!("%{}%", q.to_lowercase()));
    let mut q = sqlx::query_as::<
        _,
        (
            i64,
            Option<String>,
            String,
            String,
            String,
            String,
            i64,
            Option<String>,
        ),
    >(&sql)
    .bind(&like_pattern)
    .bind(lim);
    for id in visible_ids {
        q = q.bind(id);
    }
    let rows = q.fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .map(
            |(id, title, status, kind, created_at, updated_at, message_count, preview)| {
                ConversationListItem {
                    id,
                    title,
                    preview: preview.map(|s| {
                        // Trim attachment marker + truncate to 80 chars
                        let cleaned = s
                            .lines()
                            .next()
                            .unwrap_or("")
                            .replace("[Attached:", "")
                            .trim()
                            .to_string();
                        cleaned.chars().take(80).collect::<String>()
                    }),
                    status,
                    kind,
                    message_count,
                    updated_at,
                    created_at,
                }
            },
        )
        .collect())
}

/// Auto-close any conversation that's been idle for 7+ days.
///
/// Runs on app startup and on a daily schedule. Closes any
/// conversation in `awaiting_user` status whose updated_at is older
/// than the cutoff. Returns the number of rows updated.
///
/// Why a job: long-running awaiting_user conversations clog the
/// "where did we leave off" surface (most_recent_awaiting_user) and
/// stale workspaces feel cluttered. 7d is short enough that real
/// follow-ups still fall in the window, long enough that paused work
/// doesn't get yanked away.
pub async fn auto_close_idle(pool: &SqlitePool) -> Result<u64, sqlx::Error> {
    let res = sqlx::query(
        "UPDATE conversation
         SET status = 'closed', updated_at = updated_at
         WHERE status = 'awaiting_user'
           AND datetime(updated_at) < datetime('now', '-7 days')",
    )
    .execute(pool)
    .await?;
    Ok(res.rows_affected())
}

pub async fn most_recent_awaiting_user(
    pool: &SqlitePool,
    visible_ids: &[i64],
) -> Result<Option<Conversation>, sqlx::Error> {
    let n_ws = visible_ids.len();
    let ws_clause: String = (1..1 + n_ws)
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT id, kind, title, status, link_kind, link_id, created_at, updated_at, workspace_id
         FROM conversation
         WHERE workspace_id IN ({ws_clause})
           AND status = 'awaiting_user'
           AND datetime(updated_at) > datetime('now','-24 hours')
         ORDER BY updated_at DESC LIMIT 1"
    );
    let mut q = sqlx::query_as::<_, Conversation>(&sql);
    for id in visible_ids {
        q = q.bind(id);
    }
    q.fetch_optional(pool).await
}
