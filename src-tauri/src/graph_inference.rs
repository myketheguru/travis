//! Graph-driven inference loops (Phase 4 slices 9–11).
//!
//! These functions answer "what's interesting about the graph right
//! now?" without any LLM call. They're cheap SQL queries that
//! callers (background nudge writer, splash surface, entity detail
//! page) consume to drive UX. The user's posture preference
//! (`feedback_track_everything`) is honoured throughout: nothing
//! here gates *tracking*; it only surfaces the patterns Travis has
//! already accumulated.

use serde::Serialize;
use sqlx::SqlitePool;

/// One ambient-discovered entity that Travis has seen enough times
/// to be worth refining the role of. Surfaced by slice 13's detail
/// UI as a "Travis isn't sure what role X plays — is she a coach, a
/// parent, …?" prompt.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefinementCandidate {
    pub entity_id: i64,
    pub display_name: String,
    /// One of `person:unknown` / `place:unknown` / `org:unknown`.
    /// The `:unknown` suffix is the signal we'd refine away.
    pub kind: String,
    pub mentions_count: i64,
    pub last_seen: String,
    pub workspace_id: i64,
}

/// Find ambient `*:unknown` entities ripe for a categorisation
/// prompt. Filters:
///
/// - Workspace-scoped to the visible set (sensitive isolation rule
///   still holds — refinement candidates from a sensitive workspace
///   only surface when that workspace is active).
/// - Not archived.
/// - `mentions_count >= MIN_MENTIONS` so we only ask once the user
///   has clearly returned to the entity.
/// - `last_seen` within the trailing window (so dormant entities
///   aren't re-prompted forever).
/// - Not prompted in the cooldown window — `last_clarification_at`
///   must be NULL or older than 30 days.
///
/// Ordered by mentions desc so the most-relevant refinements
/// surface first.
pub async fn recurring_mention_candidates(
    pool: &SqlitePool,
    workspace_ids: &[i64],
    limit: i64,
) -> anyhow::Result<Vec<RefinementCandidate>> {
    if workspace_ids.is_empty() {
        return Ok(Vec::new());
    }
    let limit = limit.clamp(1, 100);
    let ws_start = 1usize;
    let ws_placeholders = (ws_start..ws_start + workspace_ids.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let mentions_param = workspace_ids.len() + 1;
    let recent_param = workspace_ids.len() + 2;
    let limit_param = workspace_ids.len() + 3;
    let sql = format!(
        "SELECT id, display_name, kind, mentions_count, last_seen, workspace_id
         FROM entity
         WHERE workspace_id IN ({ws_placeholders})
           AND archived_at IS NULL
           AND kind LIKE '%:unknown'
           AND mentions_count >= ?{mentions_param}
           AND datetime(last_seen) >= datetime('now', ?{recent_param})
           AND (
             last_clarification_at IS NULL
             OR datetime(last_clarification_at) < datetime('now', '-30 days')
           )
         ORDER BY mentions_count DESC, last_seen DESC
         LIMIT ?{limit_param}"
    );
    let mut q = sqlx::query_as::<_, (i64, String, String, i64, String, i64)>(&sql);
    for ws in workspace_ids {
        q = q.bind(ws);
    }
    q = q.bind(MIN_MENTIONS);
    q = q.bind(RECENT_WINDOW);
    q = q.bind(limit);
    let rows = q.fetch_all(pool).await?;

    Ok(rows
        .into_iter()
        .map(|(id, name, kind, mentions, last_seen, ws)| RefinementCandidate {
            entity_id: id,
            display_name: name,
            kind,
            mentions_count: mentions,
            last_seen,
            workspace_id: ws,
        })
        .collect())
}

/// Mark an entity as "we just asked the user about this" so the
/// 30-day cooldown kicks in. Called when the UI shows the
/// refinement prompt; idempotent.
pub async fn mark_clarification_prompted(
    pool: &SqlitePool,
    entity_id: i64,
) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE entity SET last_clarification_at = CURRENT_TIMESTAMP WHERE id = ?1",
    )
    .bind(entity_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Apply a user-confirmed refinement to an ambient entity: switch
/// its kind from `*:unknown` to the supplied kind, lift confidence
/// to a high value (the user told us). The previous unknown row's
/// id is preserved so all existing events / relations stay linked.
///
/// Caller is responsible for validating that `new_kind` is a
/// reasonable target (e.g. matches a pack-declared kind or one of
/// the generic non-unknown kinds like `person`). We don't gate it
/// here so packs that introduce new kinds aren't blocked.
pub async fn apply_refinement(
    pool: &SqlitePool,
    entity_id: i64,
    new_kind: &str,
) -> anyhow::Result<()> {
    let kind = new_kind.trim();
    if kind.is_empty() {
        anyhow::bail!("new kind is required");
    }
    sqlx::query(
        "UPDATE entity
         SET kind = ?1,
             confidence = MAX(confidence, 0.95),
             last_clarification_at = CURRENT_TIMESTAMP
         WHERE id = ?2",
    )
    .bind(kind)
    .bind(entity_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Fewer than this many mentions = ambient noise; nudge would feel
/// premature. Tunable later from telemetry.
const MIN_MENTIONS: i64 = 4;

/// Window of recent activity. Entities last seen outside this
/// window have probably moved on; we don't re-prompt for those even
/// if mention counts are high.
const RECENT_WINDOW: &str = "-14 days";

// ---------------------------------------------------------------------
// Slice 10: co-mention edge proposal
// ---------------------------------------------------------------------

/// One pair of entities that co-occur often enough that a labelled
/// edge would be worth confirming. Surfaced by slice 13's detail UI:
/// "You mention Maria and PS 142 together — 'Maria works at PS 142'?"
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EdgeProposal {
    pub from_entity_id: i64,
    pub from_display_name: String,
    pub from_kind: String,
    pub to_entity_id: i64,
    pub to_display_name: String,
    pub to_kind: String,
    pub co_mention_count: i64,
    pub workspace_id: i64,
    /// The id of the existing `mentioned_with` edge whose payload
    /// drives this proposal. Caller passes it back to
    /// `accept_edge_proposal` so we can label the edge in place.
    pub mentioned_with_edge_id: i64,
}

/// Find pairs of entities co-mentioned ≥ MIN_CO_MENTIONS times
/// where no labelled (non-`mentioned_with`) edge exists between
/// them. Workspace-scoped.
pub async fn edge_proposals(
    pool: &SqlitePool,
    workspace_ids: &[i64],
    limit: i64,
) -> anyhow::Result<Vec<EdgeProposal>> {
    if workspace_ids.is_empty() {
        return Ok(Vec::new());
    }
    let limit = limit.clamp(1, 100);
    let ws_placeholders = (1..=workspace_ids.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let min_param = workspace_ids.len() + 1;
    let limit_param = workspace_ids.len() + 2;

    // The subquery filter: skip pairs that already have a labelled
    // edge (any kind != 'mentioned_with') in either direction.
    //
    // Sorting by parsed co_mention_count from JSON requires us to
    // pull the count out as a real column. SQLite's json_extract
    // works here.
    let sql = format!(
        "SELECT
             r.id AS edge_id,
             r.from_entity,
             ef.display_name,
             ef.kind,
             r.to_entity,
             et.display_name,
             et.kind,
             COALESCE(json_extract(r.attributes_json, '$.co_mention_count'), 1) AS co_count,
             r.workspace_id
         FROM relation r
         JOIN entity ef ON ef.id = r.from_entity
         JOIN entity et ON et.id = r.to_entity
         WHERE r.kind = 'mentioned_with'
           AND r.workspace_id IN ({ws_placeholders})
           AND ef.archived_at IS NULL
           AND et.archived_at IS NULL
           AND COALESCE(json_extract(r.attributes_json, '$.co_mention_count'), 1) >= ?{min_param}
           AND NOT EXISTS (
             SELECT 1 FROM relation r2
             WHERE r2.workspace_id = r.workspace_id
               AND r2.kind != 'mentioned_with'
               AND (
                 (r2.from_entity = r.from_entity AND r2.to_entity = r.to_entity)
                 OR
                 (r2.from_entity = r.to_entity AND r2.to_entity = r.from_entity)
               )
           )
         ORDER BY co_count DESC, r.id DESC
         LIMIT ?{limit_param}"
    );
    let mut q =
        sqlx::query_as::<_, (i64, i64, String, String, i64, String, String, i64, i64)>(&sql);
    for ws in workspace_ids {
        q = q.bind(ws);
    }
    q = q.bind(MIN_CO_MENTIONS);
    q = q.bind(limit);
    let rows = q.fetch_all(pool).await?;

    Ok(rows
        .into_iter()
        .map(
            |(edge_id, from_id, from_name, from_kind, to_id, to_name, to_kind, co, ws)| {
                EdgeProposal {
                    from_entity_id: from_id,
                    from_display_name: from_name,
                    from_kind,
                    to_entity_id: to_id,
                    to_display_name: to_name,
                    to_kind,
                    co_mention_count: co,
                    workspace_id: ws,
                    mentioned_with_edge_id: edge_id,
                }
            },
        )
        .collect())
}

/// Promote a `mentioned_with` edge to a labelled relation. The
/// existing edge keeps its co_mention_count audit; we insert a new
/// edge with the user-supplied kind. Bidirectional kinds (e.g.
/// `colleagues`) get the same edge rendered both ways at read time;
/// directional kinds (`works_at`) carry orientation in the
/// from/to assignment chosen by the caller.
pub async fn accept_edge_proposal(
    pool: &SqlitePool,
    workspace_id: i64,
    from_entity_id: i64,
    to_entity_id: i64,
    new_kind: &str,
) -> anyhow::Result<i64> {
    let kind = new_kind.trim();
    if kind.is_empty() {
        anyhow::bail!("relation kind is required");
    }
    if kind.eq_ignore_ascii_case("mentioned_with") {
        anyhow::bail!("'mentioned_with' is reserved — pick a labelled kind");
    }
    crate::spine::relation::link(
        pool,
        crate::spine::relation::LinkParams {
            from_entity: from_entity_id,
            to_entity: to_entity_id,
            kind,
            pack_slug: None,
            attributes_json: None,
            workspace_id,
        },
    )
    .await
}

/// Co-mention threshold below which we don't bother proposing a
/// labelled edge. Three is the smallest count that's clearly not
/// a one-off: three captures pairing the entities suggests a real
/// relationship rather than a coincidence.
const MIN_CO_MENTIONS: i64 = 3;

// ---------------------------------------------------------------------
// Slice 11: same-name conflict detection
// ---------------------------------------------------------------------

/// One same-name conflict: two (or more) entities share a normalised
/// name in the same workspace under different kinds. Surfaced by
/// slice 13 as a "merge or distinguish" prompt.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NameConflict {
    pub workspace_id: i64,
    pub display_name: String,
    pub normalized_name: String,
    pub entries: Vec<ConflictEntry>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictEntry {
    pub entity_id: i64,
    pub kind: String,
    pub mentions_count: i64,
    pub last_seen: String,
    pub confidence: f64,
}

/// Find normalised names that point to 2+ non-archived entities in
/// the same workspace under different kinds, where at least one was
/// mentioned in the last 7 days (the conflict is "live"). The query
/// is one round-trip per window; result rows are grouped in Rust
/// so we don't fight SQLite's group-concat ergonomics.
pub async fn name_conflicts(
    pool: &SqlitePool,
    workspace_ids: &[i64],
    limit: i64,
) -> anyhow::Result<Vec<NameConflict>> {
    if workspace_ids.is_empty() {
        return Ok(Vec::new());
    }
    let limit = limit.clamp(1, 100);
    let ws_placeholders = (1..=workspace_ids.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ");

    let sql = format!(
        "SELECT id, kind, display_name, normalized_name, mentions_count,
                last_seen, confidence, workspace_id
         FROM entity
         WHERE workspace_id IN ({ws_placeholders})
           AND archived_at IS NULL
           AND (workspace_id, normalized_name) IN (
             SELECT workspace_id, normalized_name
             FROM entity
             WHERE archived_at IS NULL
               AND workspace_id IN ({ws_placeholders})
             GROUP BY workspace_id, normalized_name
             HAVING COUNT(DISTINCT kind) >= 2
                AND MAX(datetime(last_seen)) >= datetime('now', '-7 days')
           )
         ORDER BY workspace_id ASC, normalized_name ASC, mentions_count DESC, id ASC"
    );
    let mut q = sqlx::query_as::<_, (i64, String, String, String, i64, String, f64, i64)>(&sql);
    for ws in workspace_ids {
        q = q.bind(ws);
    }
    // Bind workspace_ids again for the inner subquery — SQLite
    // doesn't share parameter slots across subquery boundaries.
    for ws in workspace_ids {
        q = q.bind(ws);
    }
    let rows = q.fetch_all(pool).await?;

    // Group rows by (workspace_id, normalized_name).
    let mut grouped: Vec<NameConflict> = Vec::new();
    for (id, kind, display_name, normalized_name, mentions_count, last_seen, confidence, ws) in
        rows
    {
        let key_match = grouped
            .last()
            .map(|c| c.workspace_id == ws && c.normalized_name == normalized_name)
            .unwrap_or(false);
        if !key_match {
            grouped.push(NameConflict {
                workspace_id: ws,
                display_name: display_name.clone(),
                normalized_name: normalized_name.clone(),
                entries: Vec::new(),
            });
        }
        if let Some(c) = grouped.last_mut() {
            c.entries.push(ConflictEntry {
                entity_id: id,
                kind,
                mentions_count,
                last_seen,
                confidence,
            });
        }
    }
    grouped.truncate(limit as usize);
    Ok(grouped)
}

/// Merge two entities. The losing id's events + relations are
/// reassigned to the winner; mentions_count is summed; the loser is
/// archived rather than deleted so a misclick can be reversed.
/// Caller is responsible for ensuring both entities live in the
/// same workspace — we assert it here as a defensive check.
pub async fn merge_entities(
    pool: &SqlitePool,
    keep_id: i64,
    drop_id: i64,
) -> anyhow::Result<()> {
    if keep_id == drop_id {
        anyhow::bail!("cannot merge an entity with itself");
    }
    let pair: Option<(i64, i64)> = sqlx::query_as(
        "SELECT
            (SELECT workspace_id FROM entity WHERE id = ?1),
            (SELECT workspace_id FROM entity WHERE id = ?2)",
    )
    .bind(keep_id)
    .bind(drop_id)
    .fetch_optional(pool)
    .await?;
    match pair {
        Some((a, b)) if a == b => {} // ok
        Some(_) => anyhow::bail!("cross-workspace merge is not allowed"),
        None => anyhow::bail!("one or both entities not found"),
    }

    let mut tx = pool.begin().await?;
    sqlx::query("UPDATE event SET entity_id = ?1 WHERE entity_id = ?2")
        .bind(keep_id)
        .bind(drop_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE relation SET from_entity = ?1 WHERE from_entity = ?2")
        .bind(keep_id)
        .bind(drop_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE relation SET to_entity = ?1 WHERE to_entity = ?2")
        .bind(keep_id)
        .bind(drop_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "UPDATE entity
         SET mentions_count = mentions_count
                              + COALESCE((SELECT mentions_count FROM entity WHERE id = ?2), 0)
         WHERE id = ?1",
    )
    .bind(keep_id)
    .bind(drop_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE entity SET archived_at = CURRENT_TIMESTAMP WHERE id = ?1")
        .bind(drop_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}
