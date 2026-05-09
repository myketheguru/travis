//! Relation-table helpers for the pack spine.
//!
//! Relations are typed edges between two entities. Pack code (and,
//! from Phase 4 onwards, the journal extraction path) calls [`link`]
//! whenever a meaningful tie exists between two domain objects —
//! e.g. "Coach Maria works_at PS 142" or "Maria mentioned_with PS
//! 142" (the co-mention edge written from journal turns).
//!
//! Relations are NOT auto-deduped — `link` always inserts. If you
//! want single-edge semantics for a (from, to, kind) triple, dedupe
//! at the caller (slice 5 of Phase 4 does this for `mentioned_with`
//! by upserting an attributes-tracked count).

use serde::Serialize;
use sqlx::SqlitePool;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Relation {
    pub id: i64,
    pub from_entity: i64,
    pub to_entity: i64,
    pub kind: String,
    pub pack_slug: Option<String>,
    pub attributes_json: Option<String>,
    pub created_at: String,
    /// Workspace this edge belongs to. Mirrors the workspace of its
    /// endpoint entities (the caller is responsible for not crossing
    /// workspaces in a single edge).
    pub workspace_id: i64,
}

#[derive(Debug, Clone)]
pub struct LinkParams<'a> {
    pub from_entity: i64,
    pub to_entity: i64,
    pub kind: &'a str,
    pub pack_slug: Option<&'a str>,
    pub attributes_json: Option<&'a str>,
    /// Workspace this edge belongs to. Pack code passes the active
    /// workspace id; journal-extraction co-mention writes pass the
    /// workspace of the originating journal entry.
    pub workspace_id: i64,
}

/// Insert a relation row. Returns the row id. The FKs require both
/// entities to exist; foreign_keys is enabled in db::open. Caller is
/// responsible for ensuring `from_entity` and `to_entity` live in the
/// same workspace as `workspace_id`.
pub async fn link(pool: &SqlitePool, p: LinkParams<'_>) -> anyhow::Result<i64> {
    let kind = p.kind.trim();
    if kind.is_empty() {
        anyhow::bail!("relation kind is required");
    }
    let id: (i64,) = sqlx::query_as(
        "INSERT INTO relation
             (from_entity, to_entity, kind, pack_slug, attributes_json, workspace_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         RETURNING id",
    )
    .bind(p.from_entity)
    .bind(p.to_entity)
    .bind(kind)
    .bind(p.pack_slug)
    .bind(p.attributes_json)
    .bind(p.workspace_id)
    .fetch_one(pool)
    .await?;
    Ok(id.0)
}

const RELATION_COLUMNS: &str =
    "id, from_entity, to_entity, kind, pack_slug, attributes_json, created_at, workspace_id";

/// All relations originating from a given entity, optionally filtered
/// by kind. Newest first. Workspace-scoped: only edges in the visible
/// set are returned.
pub async fn list_from(
    pool: &SqlitePool,
    workspace_ids: &[i64],
    from_entity: i64,
    kind: Option<&str>,
    limit: i64,
) -> anyhow::Result<Vec<Relation>> {
    let limit = limit.clamp(1, 500);
    let ws_start = 4usize;
    let ws_placeholders = (ws_start..ws_start + workspace_ids.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT {RELATION_COLUMNS}
         FROM relation
         WHERE from_entity = ?1
           AND (?2 IS NULL OR kind = ?2)
           AND workspace_id IN ({ws_placeholders})
         ORDER BY id DESC
         LIMIT ?3"
    );
    let mut q = sqlx::query_as::<_, Relation>(&sql)
        .bind(from_entity)
        .bind(kind)
        .bind(limit);
    for ws in workspace_ids {
        q = q.bind(ws);
    }
    Ok(q.fetch_all(pool).await?)
}

/// All relations terminating at a given entity, optionally filtered
/// by kind. Newest first. Workspace-scoped.
pub async fn list_to(
    pool: &SqlitePool,
    workspace_ids: &[i64],
    to_entity: i64,
    kind: Option<&str>,
    limit: i64,
) -> anyhow::Result<Vec<Relation>> {
    let limit = limit.clamp(1, 500);
    let ws_start = 4usize;
    let ws_placeholders = (ws_start..ws_start + workspace_ids.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT {RELATION_COLUMNS}
         FROM relation
         WHERE to_entity = ?1
           AND (?2 IS NULL OR kind = ?2)
           AND workspace_id IN ({ws_placeholders})
         ORDER BY id DESC
         LIMIT ?3"
    );
    let mut q = sqlx::query_as::<_, Relation>(&sql)
        .bind(to_entity)
        .bind(kind)
        .bind(limit);
    for ws in workspace_ids {
        q = q.bind(ws);
    }
    Ok(q.fetch_all(pool).await?)
}

/// Look up an existing edge for (from, to, kind) within a workspace.
/// Used by upsert-style callers (slice 5 dedups `mentioned_with`
/// edges this way before deciding to insert vs. update attributes).
pub async fn find_edge(
    pool: &SqlitePool,
    workspace_id: i64,
    from_entity: i64,
    to_entity: i64,
    kind: &str,
) -> anyhow::Result<Option<Relation>> {
    let sql = format!(
        "SELECT {RELATION_COLUMNS}
         FROM relation
         WHERE workspace_id = ?1
           AND from_entity = ?2
           AND to_entity = ?3
           AND kind = ?4
         LIMIT 1"
    );
    let row = sqlx::query_as::<_, Relation>(&sql)
        .bind(workspace_id)
        .bind(from_entity)
        .bind(to_entity)
        .bind(kind)
        .fetch_optional(pool)
        .await?;
    Ok(row)
}

/// Update an edge's attributes_json by id. Used by slice 5 to bump
/// co-mention counts on existing `mentioned_with` edges.
pub async fn update_attributes(
    pool: &SqlitePool,
    id: i64,
    attributes_json: &str,
) -> anyhow::Result<()> {
    sqlx::query("UPDATE relation SET attributes_json = ?1 WHERE id = ?2")
        .bind(attributes_json)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}
