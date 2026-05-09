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

    // --- Phase 4 graph extensions (KNOWLEDGE_GRAPH.md slice 1). ---

    /// How sure Travis is about `kind`. Pack-projected entities get
    /// 1.0; ambient-discovered entities start at 0.5 and rise with
    /// mentions / user confirmation.
    pub confidence: f64,

    /// Comma-separated user-assigned tags. Free-form, searchable.
    pub tags: Option<String>,

    /// Soft-archive timestamp; archived entities are hidden from
    /// retrieval but past mentions stay linked.
    pub archived_at: Option<String>,

    /// Back-reference to the pack-typed row this entity projects from
    /// (for example a `coach.id` for a kind="coach" entity). NULL for
    /// ambient-only entities.
    pub pack_table_id: Option<i64>,

    /// When the entity's embedding was last refreshed. NULL means
    /// never indexed; the embedding pipeline (slice 7) treats that
    /// and "stale" the same way.
    pub embedding_indexed_at: Option<String>,

    /// Workspace this entity belongs to. Reads stay scoped to the
    /// visible-set per the Phase 2 isolation rule.
    pub workspace_id: i64,
}

/// Column list reused by every Entity SELECT. Note: `embedding_vector`
/// is intentionally excluded — it's a fastembed BLOB (~3KB), only
/// fetched on the indexer / retrieval paths to keep listing queries
/// cheap.
const ENTITY_COLUMNS: &str = "id, kind, normalized_name, display_name, \
                              pack_slug, mentions_count, first_seen, last_seen, \
                              attributes_json, confidence, tags, archived_at, \
                              pack_table_id, embedding_indexed_at, workspace_id";

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
    /// Back-reference to the pack-typed table row this entity projects
    /// from (for example a `coach.id`). When set, the entity is a hard
    /// projection of a typed row — slice 6's ambient-dedup logic uses
    /// this to avoid creating duplicate `*:unknown` entities for names
    /// that already exist as typed records.
    pub pack_table_id: Option<i64>,
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

    // Pack-projected upserts write confidence=1.0 — the entity exists
    // because a typed row exists. ON CONFLICT we lift confidence to
    // 1.0 too (an ambient-discovered entity at 0.5 just got its
    // typed projection, so it's now certain).
    let id: (i64,) = sqlx::query_as(
        "INSERT INTO entity
             (kind, normalized_name, display_name,
              pack_slug, attributes_json, workspace_id,
              pack_table_id, confidence,
              mentions_count, first_seen, last_seen)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1.0, 0, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
         ON CONFLICT(kind, normalized_name) DO UPDATE SET
            display_name    = excluded.display_name,
            pack_slug       = COALESCE(excluded.pack_slug, entity.pack_slug),
            attributes_json = COALESCE(excluded.attributes_json, entity.attributes_json),
            pack_table_id   = COALESCE(excluded.pack_table_id, entity.pack_table_id),
            confidence      = MAX(entity.confidence, 1.0),
            last_seen       = CURRENT_TIMESTAMP
         RETURNING id",
    )
    .bind(kind)
    .bind(&normalized)
    .bind(display)
    .bind(p.pack_slug)
    .bind(p.attributes_json)
    .bind(p.workspace_id)
    .bind(p.pack_table_id)
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
    let sql = format!(
        "SELECT {ENTITY_COLUMNS} FROM entity WHERE kind = ?1 AND normalized_name = ?2"
    );
    let row = sqlx::query_as::<_, Entity>(&sql)
        .bind(kind)
        .bind(&normalized)
        .fetch_optional(pool)
        .await?;
    Ok(row)
}

/// Fetch by id. Errors if not found.
pub async fn fetch_one(pool: &SqlitePool, id: i64) -> anyhow::Result<Entity> {
    let sql = format!("SELECT {ENTITY_COLUMNS} FROM entity WHERE id = ?1");
    let row = sqlx::query_as::<_, Entity>(&sql)
    .bind(id)
    .fetch_one(pool)
    .await?;
    Ok(row)
}
