//! Memory consolidation tick (BRAIN.md Phase 4.5 #3).
//!
//! Periodic background pass that turns the cloud of raw `event` rows
//! behind an entity into a stable summary persisted as a `claim` with
//! source='consolidation'. Without this, retrieval gets noisier as
//! Travis is used more — every mention is the same weight forever.
//!
//! Cheap by construction: scans at most `MAX_PER_TICK` entities per
//! invocation, picks the most-stale ones, summarises locally (no LLM
//! call), writes a fact-shaped claim, stamps `last_consolidated_at`.
//! The schedule lives in lib.rs alongside the existing graph indexer.

use sqlx::SqlitePool;

const MAX_PER_TICK: i64 = 25;
/// Re-consolidate entities whose last_consolidated_at is older than
/// this (days). Newer entities skip — their summary is still fresh.
const STALE_DAYS: i64 = 7;
/// Skip entities with fewer than this many events — there's nothing
/// to summarise yet.
const MIN_EVENTS_TO_CONSOLIDATE: i64 = 4;

/// Run one consolidation tick. Returns the number of entities updated.
pub async fn run_tick(pool: &SqlitePool) -> usize {
    let candidates: Vec<(i64, i64, String, String)> = match sqlx::query_as(
        "SELECT e.id, e.workspace_id, e.display_name, e.kind
         FROM entity e
         WHERE e.archived_at IS NULL
           AND (
             e.last_consolidated_at IS NULL
             OR e.last_consolidated_at <= datetime('now', ?1)
           )
           AND (
             SELECT COUNT(*) FROM event ev WHERE ev.entity_id = e.id
           ) >= ?2
         ORDER BY e.last_consolidated_at ASC NULLS FIRST,
                  e.mentions_count DESC
         LIMIT ?3",
    )
    .bind(format!("-{STALE_DAYS} day"))
    .bind(MIN_EVENTS_TO_CONSOLIDATE)
    .bind(MAX_PER_TICK)
    .fetch_all(pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("consolidate: candidate query failed: {e}");
            return 0;
        }
    };

    let mut consolidated = 0usize;
    for (entity_id, workspace_id, display_name, kind) in candidates {
        if let Err(e) = consolidate_one(pool, entity_id, workspace_id, &display_name, &kind).await {
            tracing::warn!("consolidate entity {entity_id} failed: {e}");
            continue;
        }
        consolidated += 1;
    }
    consolidated
}

async fn consolidate_one(
    pool: &SqlitePool,
    entity_id: i64,
    workspace_id: i64,
    display_name: &str,
    kind: &str,
) -> anyhow::Result<()> {
    // Pull aggregate metrics for the summary.
    #[derive(sqlx::FromRow)]
    struct Agg {
        event_count: i64,
        first_seen: Option<String>,
        last_seen: Option<String>,
        mention_count: i64,
    }
    let agg: Option<Agg> = sqlx::query_as(
        "SELECT
            (SELECT COUNT(*) FROM event WHERE entity_id = ?1) AS event_count,
            (SELECT MIN(occurred_at) FROM event WHERE entity_id = ?1) AS first_seen,
            (SELECT MAX(occurred_at) FROM event WHERE entity_id = ?1) AS last_seen,
            (SELECT COUNT(*) FROM event WHERE entity_id = ?1 AND kind = 'mentioned') AS mention_count",
    )
    .bind(entity_id)
    .fetch_optional(pool)
    .await?;
    let Some(a) = agg else { return Ok(()) };

    // Top co-mentioned entity for relational context.
    #[derive(sqlx::FromRow)]
    struct CoMention {
        other_id: i64,
        other_name: String,
        count: i64,
    }
    let top_co: Option<CoMention> = sqlx::query_as(
        "SELECT
            CASE WHEN r.from_entity = ?1 THEN r.to_entity ELSE r.from_entity END AS other_id,
            e.display_name AS other_name,
            COALESCE(json_extract(r.attributes_json, '$.co_mention_count'), 1) AS count
         FROM relation r
         JOIN entity e ON e.id = CASE WHEN r.from_entity = ?1 THEN r.to_entity ELSE r.from_entity END
         WHERE r.kind = 'mentioned_with'
           AND (r.from_entity = ?1 OR r.to_entity = ?1)
           AND e.archived_at IS NULL
         ORDER BY count DESC
         LIMIT 1",
    )
    .bind(entity_id)
    .fetch_optional(pool)
    .await?;

    // Compose the human-shaped summary string. This is what the LLM
    // sees when the claim surfaces in retrieval — Travis-voice, not
    // schema dump.
    let mut summary = format!(
        "{} ({kind}) — {} event{} on record",
        display_name,
        a.event_count,
        if a.event_count == 1 { "" } else { "s" }
    );
    if a.mention_count > 0 {
        summary.push_str(&format!(
            "; {} mention{}",
            a.mention_count,
            if a.mention_count == 1 { "" } else { "s" }
        ));
    }
    if let (Some(first), Some(last)) = (a.first_seen.as_deref(), a.last_seen.as_deref()) {
        let f = first.split([' ', 'T']).next().unwrap_or(first);
        let l = last.split([' ', 'T']).next().unwrap_or(last);
        if f == l {
            summary.push_str(&format!("; seen {f}"));
        } else {
            summary.push_str(&format!("; first {f}, last {l}"));
        }
    }
    if let Some(co) = top_co.as_ref() {
        summary.push_str(&format!("; often co-mentioned with {} (×{})", co.other_name, co.count));
    }
    summary.push('.');

    // Upsert as a consolidation claim. Predicate 'summary' is the
    // standard predicate the consolidation pass writes — supersedes
    // any older summary for this entity.
    sqlx::query(
        "UPDATE claim SET superseded_at = CURRENT_TIMESTAMP, updated_at = CURRENT_TIMESTAMP
         WHERE entity_id = ?1
           AND predicate = 'summary'
           AND source = 'consolidation'
           AND superseded_at IS NULL",
    )
    .bind(entity_id)
    .execute(pool)
    .await?;

    let confidence = if a.event_count >= 8 { "high" } else { "medium" };
    sqlx::query(
        "INSERT INTO claim
            (workspace_id, entity_id, other_entity_id, predicate, value,
             confidence, source)
         VALUES (?1, ?2, ?3, 'summary', ?4, ?5, 'consolidation')",
    )
    .bind(workspace_id)
    .bind(entity_id)
    .bind(top_co.as_ref().map(|c| c.other_id))
    .bind(&summary)
    .bind(confidence)
    .execute(pool)
    .await?;

    // Stamp the entity so the next tick skips it until STALE_DAYS pass.
    sqlx::query("UPDATE entity SET last_consolidated_at = CURRENT_TIMESTAMP WHERE id = ?1")
        .bind(entity_id)
        .execute(pool)
        .await?;

    Ok(())
}
