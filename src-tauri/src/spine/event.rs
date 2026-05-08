//! Event-table helpers for the pack spine.
//!
//! Events are append-only records of "something happened". Pack code calls
//! [`record`] whenever meaningful state changes — "hours logged", "invoice
//! issued", "session completed", "job dispatched". The activity timeline,
//! daily/weekly summary, and audit trail all read from this table.
//!
//! Events are immutable once written. There is no update or delete API.

use serde::Serialize;
use sqlx::SqlitePool;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub id: i64,
    pub entity_id: Option<i64>,
    pub kind: String,
    pub pack_slug: Option<String>,
    pub occurred_at: String,
    pub attributes_json: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct RecordParams<'a> {
    pub entity_id: Option<i64>,
    pub kind: &'a str,
    pub pack_slug: Option<&'a str>,
    /// Defaults to CURRENT_TIMESTAMP when None. Pass a specific ISO 8601
    /// timestamp for events that happened in the past (e.g. journal entries
    /// being indexed retroactively).
    pub occurred_at: Option<&'a str>,
    pub attributes_json: Option<&'a str>,
    /// Workspace this event belongs to. Pack code should pass the
    /// active workspace id.
    pub workspace_id: i64,
}

/// Append an event row. Returns the row id.
pub async fn record(pool: &SqlitePool, p: RecordParams<'_>) -> anyhow::Result<i64> {
    let kind = p.kind.trim();
    if kind.is_empty() {
        anyhow::bail!("event kind is required");
    }
    let id: (i64,) = if let Some(at) = p.occurred_at {
        sqlx::query_as(
            "INSERT INTO event (entity_id, kind, pack_slug, occurred_at, attributes_json, workspace_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             RETURNING id",
        )
        .bind(p.entity_id)
        .bind(kind)
        .bind(p.pack_slug)
        .bind(at)
        .bind(p.attributes_json)
        .bind(p.workspace_id)
        .fetch_one(pool)
        .await?
    } else {
        sqlx::query_as(
            "INSERT INTO event (entity_id, kind, pack_slug, attributes_json, workspace_id)
             VALUES (?1, ?2, ?3, ?4, ?5)
             RETURNING id",
        )
        .bind(p.entity_id)
        .bind(kind)
        .bind(p.pack_slug)
        .bind(p.attributes_json)
        .bind(p.workspace_id)
        .fetch_one(pool)
        .await?
    };
    Ok(id.0)
}

/// Most recent events for an entity, newest first.
pub async fn list_for_entity(
    pool: &SqlitePool,
    entity_id: i64,
    limit: i64,
) -> anyhow::Result<Vec<Event>> {
    let limit = limit.clamp(1, 500);
    let rows = sqlx::query_as::<_, Event>(
        "SELECT id, entity_id, kind, pack_slug, occurred_at, attributes_json, created_at
         FROM event
         WHERE entity_id = ?1
         ORDER BY occurred_at DESC, id DESC
         LIMIT ?2",
    )
    .bind(entity_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Most recent events of a given kind across all entities, newest first.
pub async fn list_by_kind(
    pool: &SqlitePool,
    kind: &str,
    limit: i64,
) -> anyhow::Result<Vec<Event>> {
    let limit = limit.clamp(1, 500);
    let rows = sqlx::query_as::<_, Event>(
        "SELECT id, entity_id, kind, pack_slug, occurred_at, attributes_json, created_at
         FROM event
         WHERE kind = ?1
         ORDER BY occurred_at DESC, id DESC
         LIMIT ?2",
    )
    .bind(kind)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
