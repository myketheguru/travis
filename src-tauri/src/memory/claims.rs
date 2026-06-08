//! Persisted claims layer (BRAIN.md Phase 4.5 #7).
//!
//! Stores reasoning conclusions Travis has tentatively reached so they
//! survive across sessions and inform future replies. A claim is
//! `(entity, predicate, value)` with confidence + source attribution
//! — e.g. `(Maria, "role", "math coach at PS 142", confidence="high",
//! source="derived")`. Contradicting claims are kept side-by-side
//! flagged `contested = 1` rather than overwritten, so Travis can
//! surface the conflict in conversation rather than silently picking.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Claim {
    pub id: i64,
    pub workspace_id: i64,
    pub entity_id: i64,
    pub other_entity_id: Option<i64>,
    pub predicate: String,
    pub value: String,
    pub confidence: String,
    pub source: String,
    pub source_journal_entry_id: Option<i64>,
    pub contested: i64,
    pub superseded_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ClaimInput {
    pub workspace_id: i64,
    pub entity_id: i64,
    pub other_entity_id: Option<i64>,
    pub predicate: String,
    pub value: String,
    pub confidence: Option<String>,
    pub source: Option<String>,
    pub source_journal_entry_id: Option<i64>,
}

/// Idempotent insert. If a claim already exists with the same
/// (workspace, entity, predicate, value) and is not superseded, its
/// confidence may upgrade (only) and its updated_at refreshes. Returns
/// the resulting claim's id.
pub async fn upsert(pool: &SqlitePool, input: ClaimInput) -> anyhow::Result<i64> {
    let confidence = input.confidence.as_deref().unwrap_or("medium");
    let source = input.source.as_deref().unwrap_or("extraction");

    let existing: Option<(i64, String)> = sqlx::query_as(
        "SELECT id, confidence FROM claim
         WHERE workspace_id = ?1
           AND entity_id = ?2
           AND predicate = ?3
           AND value = ?4
           AND superseded_at IS NULL
         LIMIT 1",
    )
    .bind(input.workspace_id)
    .bind(input.entity_id)
    .bind(&input.predicate)
    .bind(&input.value)
    .fetch_optional(pool)
    .await?;

    if let Some((id, existing_conf)) = existing {
        // Only upgrade confidence, never downgrade. high > medium > low.
        let new_conf = upgrade_confidence(&existing_conf, confidence);
        sqlx::query(
            "UPDATE claim SET confidence = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
        )
        .bind(&new_conf)
        .bind(id)
        .execute(pool)
        .await?;
        return Ok(id);
    }

    let new_id: i64 = sqlx::query_scalar(
        "INSERT INTO claim
            (workspace_id, entity_id, other_entity_id, predicate, value,
             confidence, source, source_journal_entry_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) RETURNING id",
    )
    .bind(input.workspace_id)
    .bind(input.entity_id)
    .bind(input.other_entity_id)
    .bind(&input.predicate)
    .bind(&input.value)
    .bind(confidence)
    .bind(source)
    .bind(input.source_journal_entry_id)
    .fetch_one(pool)
    .await?;

    // After insert, check for contradicting claims on the same
    // (entity, predicate) and flag both sides if found.
    flag_contradictions(pool, input.workspace_id, input.entity_id, &input.predicate).await?;

    Ok(new_id)
}

/// Active claims for an entity (not superseded). Ordered by
/// confidence DESC then recency DESC so the strongest current claim
/// shows first.
pub async fn for_entity(
    pool: &SqlitePool,
    workspace_id: i64,
    entity_id: i64,
    limit: i64,
) -> Vec<Claim> {
    sqlx::query_as::<_, Claim>(
        "SELECT id, workspace_id, entity_id, other_entity_id, predicate, value,
                confidence, source, source_journal_entry_id, contested,
                superseded_at, created_at, updated_at
         FROM claim
         WHERE workspace_id = ?1
           AND entity_id = ?2
           AND superseded_at IS NULL
         ORDER BY
           CASE confidence WHEN 'high' THEN 0 WHEN 'medium' THEN 1 ELSE 2 END,
           updated_at DESC
         LIMIT ?3",
    )
    .bind(workspace_id)
    .bind(entity_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
}

/// Mark a claim as superseded (soft delete).
pub async fn supersede(pool: &SqlitePool, claim_id: i64) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE claim SET superseded_at = CURRENT_TIMESTAMP,
                          updated_at = CURRENT_TIMESTAMP
         WHERE id = ?1 AND superseded_at IS NULL",
    )
    .bind(claim_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// User-confirms a claim — upgrades source to user_confirmed and
/// confidence to high. The strongest signal Travis can record.
pub async fn confirm(pool: &SqlitePool, claim_id: i64) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE claim SET source = 'user_confirmed',
                          confidence = 'high',
                          contested = 0,
                          updated_at = CURRENT_TIMESTAMP
         WHERE id = ?1",
    )
    .bind(claim_id)
    .execute(pool)
    .await?;
    Ok(())
}

fn upgrade_confidence(existing: &str, incoming: &str) -> String {
    let rank = |c: &str| match c {
        "high" => 2,
        "medium" => 1,
        _ => 0,
    };
    if rank(incoming) > rank(existing) {
        incoming.to_string()
    } else {
        existing.to_string()
    }
}

/// Flag all active claims with the same (entity, predicate) but
/// different values as contested. Called after every insert so
/// contradictions are detected eagerly. Travis can then surface
/// them in retrieval.
async fn flag_contradictions(
    pool: &SqlitePool,
    workspace_id: i64,
    entity_id: i64,
    predicate: &str,
) -> anyhow::Result<()> {
    // Count distinct values for this (entity, predicate).
    let distinct: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT value) FROM claim
         WHERE workspace_id = ?1
           AND entity_id = ?2
           AND predicate = ?3
           AND superseded_at IS NULL",
    )
    .bind(workspace_id)
    .bind(entity_id)
    .bind(predicate)
    .fetch_one(pool)
    .await?;

    let flag = if distinct > 1 { 1 } else { 0 };
    sqlx::query(
        "UPDATE claim SET contested = ?1
         WHERE workspace_id = ?2
           AND entity_id = ?3
           AND predicate = ?4
           AND superseded_at IS NULL",
    )
    .bind(flag)
    .bind(workspace_id)
    .bind(entity_id)
    .bind(predicate)
    .execute(pool)
    .await?;
    Ok(())
}

/// v0.16.3 — exponential relevance decay.
///
/// Applies `relevance_score *= decay_factor` to every active,
/// unpinned claim. The decay factor is computed from the half-life
/// and the elapsed days since last decay (or since creation if no
/// prior decay). At a 180-day half-life with daily ticks, the
/// per-tick factor is ~0.9962 — roughly 0.4% per day. After 6
/// months unpinned claims sit at 0.5; after a year at 0.25.
///
/// We do the work in SQL so the bulk update is a single statement.
/// Travis's claim table won't grow large enough (initially) to need
/// chunked decay, but if it does the approach is to add a WHERE on
/// `updated_at < datetime('now', '-1 day')` so freshly-touched
/// claims aren't decayed twice.
///
/// Returns the number of rows updated.
pub async fn decay_all(
    pool: &SqlitePool,
    half_life_days: f64,
) -> anyhow::Result<u64> {
    // Daily decay factor solved from half-life:
    //   factor^days = 0.5  →  factor = 0.5^(1/days)
    let factor = 0.5_f64.powf(1.0 / half_life_days);
    let result = sqlx::query(
        "UPDATE claim
         SET relevance_score = MAX(0.0, relevance_score * ?1),
             updated_at = updated_at
         WHERE pinned = 0
           AND superseded_at IS NULL
           AND relevance_score > 0.0",
    )
    .bind(factor)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Archive (supersede) claims whose relevance has decayed below the
/// floor. Travis still has the row for audit purposes but it's no
/// longer surfaced in recall.
pub async fn archive_low_relevance(
    pool: &SqlitePool,
    floor: f64,
) -> anyhow::Result<u64> {
    let result = sqlx::query(
        "UPDATE claim
         SET superseded_at = CURRENT_TIMESTAMP
         WHERE pinned = 0
           AND superseded_at IS NULL
           AND relevance_score < ?1",
    )
    .bind(floor)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Pin a claim so future decay passes skip it. Used for
/// user-confirmed durable facts ("Maria is the math coach at
/// PS 142" — she stays the math coach until told otherwise).
#[allow(dead_code)] // exposed for upcoming pack-level pinning
pub async fn pin(pool: &SqlitePool, claim_id: i64) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE claim SET pinned = 1, relevance_score = 1.0
         WHERE id = ?1",
    )
    .bind(claim_id)
    .execute(pool)
    .await?;
    Ok(())
}
