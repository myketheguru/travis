//! Entity-table helpers for the pack spine.
//!
//! Pack code calls [`upsert`] from its CRUD paths to register a domain
//! object (coach, client, case, job, invoice). The spine keeps a single
//! row per (kind, normalized_name) so mentions and hard records dedupe
//! automatically.

use serde::Serialize;
use sqlx::SqlitePool;

use crate::identity::normalize;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Entity {
    pub id: i64,
    pub kind: String,
    pub normalized_name: String,
    pub display_name: String,
    pub pack_slug: Option<String>,
    pub mentions_count: i64,
    pub first_seen: String,
    pub last_seen: String,
    pub attributes_json: Option<String>,
}

/// Parameters for [`upsert`]. `display_name` is normalized to derive the
/// uniqueness key; pass the original casing so it shows up nicely in the UI.
#[derive(Debug, Clone)]
pub struct UpsertParams<'a> {
    pub kind: &'a str,
    pub display_name: &'a str,
    pub pack_slug: Option<&'a str>,
    pub attributes_json: Option<&'a str>,
    /// Workspace this entity belongs to. Pack code should pass the
    /// active workspace id (typically `state.workspace.read().await.active_id`).
    pub workspace_id: i64,
}

/// Upsert an entity row. Returns the row id. Idempotent on
/// (kind, normalized_name): repeated calls update `display_name`,
/// `pack_slug`, `attributes_json`, and `last_seen` but don't bump
/// `mentions_count` — use [`crate::identity::record_mention`] for that.
pub async fn upsert(pool: &SqlitePool, p: UpsertParams<'_>) -> anyhow::Result<i64> {
    let display = p.display_name.trim();
    if display.is_empty() {
        anyhow::bail!("display_name is required");
    }
    let normalized = normalize(display);
    if normalized.is_empty() {
        anyhow::bail!("display_name normalizes to empty — refusing to upsert");
    }
    let kind = p.kind.trim();
    if kind.is_empty() {
        anyhow::bail!("kind is required");
    }

    let id: (i64,) = sqlx::query_as(
        "INSERT INTO entity
             (kind, normalized_name, display_name,
              pack_slug, attributes_json, workspace_id,
              mentions_count, first_seen, last_seen)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
         ON CONFLICT(kind, normalized_name) DO UPDATE SET
            display_name    = excluded.display_name,
            pack_slug       = COALESCE(excluded.pack_slug, entity.pack_slug),
            attributes_json = COALESCE(excluded.attributes_json, entity.attributes_json),
            last_seen       = CURRENT_TIMESTAMP
         RETURNING id",
    )
    .bind(kind)
    .bind(&normalized)
    .bind(display)
    .bind(p.pack_slug)
    .bind(p.attributes_json)
    .bind(p.workspace_id)
    .fetch_one(pool)
    .await?;

    Ok(id.0)
}

/// Look up by (kind, normalized name). Returns `None` if no match.
pub async fn find_by_name(
    pool: &SqlitePool,
    kind: &str,
    display_name: &str,
) -> anyhow::Result<Option<Entity>> {
    let normalized = normalize(display_name);
    if normalized.is_empty() {
        return Ok(None);
    }
    let row = sqlx::query_as::<_, Entity>(
        "SELECT id, kind, normalized_name, display_name, pack_slug,
                mentions_count, first_seen, last_seen, attributes_json
         FROM entity
         WHERE kind = ?1 AND normalized_name = ?2",
    )
    .bind(kind)
    .bind(&normalized)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Fetch by id. Errors if not found.
pub async fn fetch_one(pool: &SqlitePool, id: i64) -> anyhow::Result<Entity> {
    let row = sqlx::query_as::<_, Entity>(
        "SELECT id, kind, normalized_name, display_name, pack_slug,
                mentions_count, first_seen, last_seen, attributes_json
         FROM entity WHERE id = ?1",
    )
    .bind(id)
    .fetch_one(pool)
    .await?;
    Ok(row)
}
