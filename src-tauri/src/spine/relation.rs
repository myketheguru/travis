//! Relation-table helpers for the pack spine.
//!
//! Relations are typed edges between two entities. Pack code calls [`link`]
//! whenever a meaningful tie exists between two domain objects — e.g.
//! "Coach Maria works_at PS 142" or "Invoice 0042 billed_to NYC DoF".
//!
//! Relations are NOT auto-deduped — `link` always inserts. If you want
//! single-edge semantics for a (from, to, kind) triple, dedupe at the
//! pack-code level before calling.

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
}

#[derive(Debug, Clone)]
pub struct LinkParams<'a> {
    pub from_entity: i64,
    pub to_entity: i64,
    pub kind: &'a str,
    pub pack_slug: Option<&'a str>,
    pub attributes_json: Option<&'a str>,
}

/// Insert a relation row. Returns the row id. The FKs require both entities
/// to exist; foreign_keys is enabled in db::open.
pub async fn link(pool: &SqlitePool, p: LinkParams<'_>) -> anyhow::Result<i64> {
    let kind = p.kind.trim();
    if kind.is_empty() {
        anyhow::bail!("relation kind is required");
    }
    let id: (i64,) = sqlx::query_as(
        "INSERT INTO relation
             (from_entity, to_entity, kind, pack_slug, attributes_json)
         VALUES (?1, ?2, ?3, ?4, ?5)
         RETURNING id",
    )
    .bind(p.from_entity)
    .bind(p.to_entity)
    .bind(kind)
    .bind(p.pack_slug)
    .bind(p.attributes_json)
    .fetch_one(pool)
    .await?;
    Ok(id.0)
}

/// All relations originating from a given entity, optionally filtered by
/// kind. Newest first.
pub async fn list_from(
    pool: &SqlitePool,
    from_entity: i64,
    kind: Option<&str>,
    limit: i64,
) -> anyhow::Result<Vec<Relation>> {
    let limit = limit.clamp(1, 500);
    let rows = sqlx::query_as::<_, Relation>(
        "SELECT id, from_entity, to_entity, kind, pack_slug, attributes_json, created_at
         FROM relation
         WHERE from_entity = ?1
           AND (?2 IS NULL OR kind = ?2)
         ORDER BY id DESC
         LIMIT ?3",
    )
    .bind(from_entity)
    .bind(kind)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// All relations terminating at a given entity, optionally filtered by
/// kind. Newest first.
pub async fn list_to(
    pool: &SqlitePool,
    to_entity: i64,
    kind: Option<&str>,
    limit: i64,
) -> anyhow::Result<Vec<Relation>> {
    let limit = limit.clamp(1, 500);
    let rows = sqlx::query_as::<_, Relation>(
        "SELECT id, from_entity, to_entity, kind, pack_slug, attributes_json, created_at
         FROM relation
         WHERE to_entity = ?1
           AND (?2 IS NULL OR kind = ?2)
         ORDER BY id DESC
         LIMIT ?3",
    )
    .bind(to_entity)
    .bind(kind)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
