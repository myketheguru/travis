//! Typed-edge memory graph — AutoMem-style relationship layer.
//!
//! v0.16.4 — adds the lineage / reconciliation primitive Travis needs
//! for cross-document reasoning. PO `LEADS_TO` invoice. Invoice
//! `DERIVED_FROM` sign-in sheet. Amendment `EVOLVED_INTO` from
//! contract. Corrected total `INVALIDATES` prior one.
//!
//! The 11 canonical relation types (per AutoMem):
//!   `RELATES_TO`, `LEADS_TO`, `OCCURRED_BEFORE`, `PREFERS_OVER`,
//!   `EXEMPLIFIES`, `CONTRADICTS`, `REINFORCES`, `INVALIDATED_BY`,
//!   `EVOLVED_INTO`, `DERIVED_FROM`, `PART_OF`.
//!
//! Strings stay open-ended for future extension — Travis can invent
//! new types during reasoning. Constants here are the canonical set
//! the prompt + tools steer toward.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

#[allow(dead_code)]
pub const RELATES_TO: &str = "RELATES_TO";
#[allow(dead_code)]
pub const LEADS_TO: &str = "LEADS_TO";
#[allow(dead_code)]
pub const OCCURRED_BEFORE: &str = "OCCURRED_BEFORE";
#[allow(dead_code)]
pub const PREFERS_OVER: &str = "PREFERS_OVER";
#[allow(dead_code)]
pub const EXEMPLIFIES: &str = "EXEMPLIFIES";
#[allow(dead_code)]
pub const CONTRADICTS: &str = "CONTRADICTS";
#[allow(dead_code)]
pub const REINFORCES: &str = "REINFORCES";
#[allow(dead_code)]
pub const INVALIDATED_BY: &str = "INVALIDATED_BY";
#[allow(dead_code)]
pub const EVOLVED_INTO: &str = "EVOLVED_INTO";
#[allow(dead_code)]
pub const DERIVED_FROM: &str = "DERIVED_FROM";
#[allow(dead_code)]
pub const PART_OF: &str = "PART_OF";

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct MemoryEdge {
    pub id: i64,
    pub workspace_id: i64,
    pub from_kind: String,
    pub from_id: i64,
    pub to_kind: String,
    pub to_id: i64,
    pub relation: String,
    pub attributes_json: Option<String>,
    pub created_at: String,
}

/// Idempotent insert. The unique index on (workspace, from, to,
/// relation) means a re-insert is a no-op; we use INSERT OR IGNORE
/// so the call site doesn't need to check existence first.
pub async fn link(
    pool: &SqlitePool,
    workspace_id: i64,
    from_kind: &str,
    from_id: i64,
    to_kind: &str,
    to_id: i64,
    relation: &str,
    attributes: Option<serde_json::Value>,
) -> anyhow::Result<()> {
    let attrs = attributes.map(|v| v.to_string());
    sqlx::query(
        "INSERT OR IGNORE INTO memory_edge
            (workspace_id, from_kind, from_id, to_kind, to_id, relation, attributes_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )
    .bind(workspace_id)
    .bind(from_kind)
    .bind(from_id)
    .bind(to_kind)
    .bind(to_id)
    .bind(relation)
    .bind(attrs.as_deref())
    .execute(pool)
    .await?;
    Ok(())
}

/// All edges originating from a given node.
#[allow(dead_code)]
pub async fn edges_from(
    pool: &SqlitePool,
    from_kind: &str,
    from_id: i64,
) -> Vec<MemoryEdge> {
    sqlx::query_as::<_, MemoryEdge>(
        "SELECT id, workspace_id, from_kind, from_id, to_kind, to_id,
                relation, attributes_json, created_at
         FROM memory_edge
         WHERE from_kind = ?1 AND from_id = ?2
         ORDER BY created_at DESC",
    )
    .bind(from_kind)
    .bind(from_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
}

/// All edges pointing at a given node.
#[allow(dead_code)]
pub async fn edges_to(
    pool: &SqlitePool,
    to_kind: &str,
    to_id: i64,
) -> Vec<MemoryEdge> {
    sqlx::query_as::<_, MemoryEdge>(
        "SELECT id, workspace_id, from_kind, from_id, to_kind, to_id,
                relation, attributes_json, created_at
         FROM memory_edge
         WHERE to_kind = ?1 AND to_id = ?2
         ORDER BY created_at DESC",
    )
    .bind(to_kind)
    .bind(to_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
}

/// All edges of a given relation type across the workspace. Used
/// for queries like "show me everything that CONTRADICTS something".
#[allow(dead_code)]
pub async fn edges_by_relation(
    pool: &SqlitePool,
    workspace_id: i64,
    relation: &str,
    limit: i64,
) -> Vec<MemoryEdge> {
    sqlx::query_as::<_, MemoryEdge>(
        "SELECT id, workspace_id, from_kind, from_id, to_kind, to_id,
                relation, attributes_json, created_at
         FROM memory_edge
         WHERE workspace_id = ?1 AND relation = ?2
         ORDER BY created_at DESC
         LIMIT ?3",
    )
    .bind(workspace_id)
    .bind(relation)
    .bind(limit)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
}
