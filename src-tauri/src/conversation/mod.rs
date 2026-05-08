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
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Thread {
    pub conversation: Conversation,
    pub messages: Vec<ConversationMessage>,
}

pub async fn open(
    pool: &SqlitePool,
    workspace_id: i64,
    kind: &str,
    title: Option<&str>,
) -> Result<Conversation, sqlx::Error> {
    let id = sqlx::query(
        "INSERT INTO conversation (kind, title, workspace_id) VALUES (?1, ?2, ?3)",
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
        "SELECT id, kind, title, status, link_kind, link_id, created_at, updated_at, workspace_id
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
    let id = sqlx::query(
        "INSERT INTO conversation_message (conversation_id, role, content, payload_json)
         VALUES (?1, ?2, ?3, ?4)",
    )
    .bind(conversation_id)
    .bind(role)
    .bind(content)
    .bind(payload_json)
    .execute(pool)
    .await?
    .last_insert_rowid();

    sqlx::query("UPDATE conversation SET updated_at = CURRENT_TIMESTAMP WHERE id = ?1")
        .bind(conversation_id)
        .execute(pool)
        .await?;

    sqlx::query_as::<_, ConversationMessage>(
        "SELECT id, conversation_id, role, content, payload_json, created_at
         FROM conversation_message WHERE id=?1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
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
    let lim = limit.unwrap_or(200).clamp(1, 500);
    sqlx::query_as::<_, ConversationMessage>(
        "SELECT id, conversation_id, role, content, payload_json, created_at
         FROM conversation_message
         WHERE conversation_id = ?1
         ORDER BY id ASC
         LIMIT ?2",
    )
    .bind(conversation_id)
    .bind(lim)
    .fetch_all(pool)
    .await
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
