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
