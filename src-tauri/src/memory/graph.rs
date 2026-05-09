//! Graph-aware retrieval (Phase 4 slice 8).
//!
//! When the user's input names an entity Travis already knows about,
//! pull a tight summary of what Travis remembers about that entity:
//! the most recent events, the most recent mention snippets, and the
//! entities that co-mention most often. The journal + ask paths
//! inject this into the system prompt's RELEVANT MEMORY block
//! alongside the existing text-similarity hits — the LLM picks what
//! to use.
//!
//! Cheap by construction: indexed lookups on entity / event /
//! relation tables, with strict per-hit row caps. Slice 8's only
//! retrieval path is name-based; embedding-based fuzzy lookup is a
//! follow-up once we have telemetry on misses.

use serde::Serialize;
use sqlx::SqlitePool;

use crate::identity;

/// One entity's worth of graph context. Renders as a short block in
/// the LLM's user message.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphHit {
    pub entity_id: i64,
    pub display_name: String,
    pub kind: String,
    pub mentions_count: i64,
    pub last_seen: String,
    pub recent_events: Vec<EventSummary>,
    pub recent_mention_snippets: Vec<MentionSnippet>,
    pub related_entities: Vec<RelatedEntity>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventSummary {
    pub kind: String,
    pub occurred_at: String,
    pub attributes_json: Option<String>,
    pub pack_slug: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MentionSnippet {
    pub journal_entry_id: i64,
    pub occurred_at: String,
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelatedEntity {
    pub entity_id: i64,
    pub display_name: String,
    pub kind: String,
    pub co_mention_count: i64,
}

/// Look up graph context for each name in `name_hints`, scoped to
/// `workspace_ids`. Names that don't resolve to a known non-archived
/// entity are silently skipped — caller can mix this with text
/// similarity retrieval to cover those cases.
pub async fn retrieve(
    pool: &SqlitePool,
    workspace_ids: &[i64],
    name_hints: &[String],
) -> Vec<GraphHit> {
    if workspace_ids.is_empty() || name_hints.is_empty() {
        return Vec::new();
    }
    // Pick the highest-confidence visible workspace as the lookup
    // pivot. find_by_normalized_name takes a single workspace; for
    // cross-workspace retrieval we iterate the visible set so a name
    // mentioned in any visible workspace can still anchor a hit.
    let mut hits: Vec<GraphHit> = Vec::new();
    let mut seen: std::collections::HashSet<i64> = std::collections::HashSet::new();
    for hint in name_hints {
        for ws in workspace_ids {
            let Some((eid, kind, _pack_slug)) =
                identity::find_by_normalized_name(pool, *ws, hint).await
            else {
                continue;
            };
            if seen.contains(&eid) {
                continue;
            }
            seen.insert(eid);
            if let Some(hit) = build_hit(pool, eid, kind).await {
                hits.push(hit);
            }
            break; // resolved this hint to one entity; stop scanning ws
        }
    }
    hits
}

async fn build_hit(pool: &SqlitePool, entity_id: i64, kind: String) -> Option<GraphHit> {
    // Header — display_name + counts. Cheap; one row.
    let header: Result<Option<(String, i64, String)>, _> = sqlx::query_as(
        "SELECT display_name, mentions_count, last_seen
         FROM entity WHERE id = ?1 AND archived_at IS NULL",
    )
    .bind(entity_id)
    .fetch_optional(pool)
    .await;
    let (display_name, mentions_count, last_seen) = match header {
        Ok(Some(row)) => row,
        _ => return None,
    };

    // Recent events (any kind) — entity-detail timeline shape.
    let event_rows: Result<Vec<(String, String, Option<String>, Option<String>)>, _> =
        sqlx::query_as(
            "SELECT kind, occurred_at, attributes_json, pack_slug
             FROM event
             WHERE entity_id = ?1
             ORDER BY occurred_at DESC, id DESC
             LIMIT 5",
        )
        .bind(entity_id)
        .fetch_all(pool)
        .await;
    let recent_events: Vec<EventSummary> = event_rows
        .unwrap_or_default()
        .into_iter()
        .map(|(kind, occurred_at, attributes_json, pack_slug)| EventSummary {
            kind,
            occurred_at,
            attributes_json,
            pack_slug,
        })
        .collect();

    // Recent mention snippets — the `mentioned` event subset, with
    // the journal_entry_id + snippet pulled out of attributes_json.
    let mention_rows: Result<Vec<(String, Option<String>)>, _> = sqlx::query_as(
        "SELECT occurred_at, attributes_json
         FROM event
         WHERE entity_id = ?1 AND kind = 'mentioned'
         ORDER BY occurred_at DESC, id DESC
         LIMIT 3",
    )
    .bind(entity_id)
    .fetch_all(pool)
    .await;
    let recent_mention_snippets: Vec<MentionSnippet> = mention_rows
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(occurred_at, attrs)| {
            let parsed: serde_json::Value = attrs
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or(serde_json::Value::Null);
            let journal_entry_id = parsed.get("journal_entry_id")?.as_i64()?;
            let snippet = parsed
                .get("snippet")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Some(MentionSnippet {
                journal_entry_id,
                occurred_at,
                snippet,
            })
        })
        .collect();

    // Top-2 co-mentioned entities. Search both directions of the
    // mentioned_with edge since canonical ordering means our entity
    // can be either endpoint.
    let related_rows: Result<Vec<(i64, String, String, Option<String>)>, _> = sqlx::query_as(
        "SELECT e.id, e.display_name, e.kind, r.attributes_json
         FROM relation r
         JOIN entity e
           ON e.id = CASE
                       WHEN r.from_entity = ?1 THEN r.to_entity
                       ELSE r.from_entity
                     END
         WHERE r.kind = 'mentioned_with'
           AND (r.from_entity = ?1 OR r.to_entity = ?1)
           AND e.archived_at IS NULL
         LIMIT 12",
    )
    .bind(entity_id)
    .fetch_all(pool)
    .await;
    let mut related: Vec<RelatedEntity> = related_rows
        .unwrap_or_default()
        .into_iter()
        .map(|(id, name, k, attrs)| {
            let parsed: serde_json::Value = attrs
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or(serde_json::Value::Null);
            let co = parsed
                .get("co_mention_count")
                .and_then(|v| v.as_i64())
                .unwrap_or(1);
            RelatedEntity {
                entity_id: id,
                display_name: name,
                kind: k,
                co_mention_count: co,
            }
        })
        .collect();
    related.sort_by(|a, b| b.co_mention_count.cmp(&a.co_mention_count));
    related.truncate(2);

    Some(GraphHit {
        entity_id,
        display_name,
        kind,
        mentions_count,
        last_seen,
        recent_events,
        recent_mention_snippets,
        related_entities: related,
    })
}

/// Render a list of GraphHits as the GRAPH MEMORY block injected
/// into the LLM's user message. Returns an empty string when there
/// are no hits — caller can append the result without a separator.
pub fn format_for_prompt(hits: &[GraphHit]) -> String {
    if hits.is_empty() {
        return String::new();
    }
    let mut out = String::from("GRAPH MEMORY (entities Travis already knows about):\n");
    for h in hits {
        out.push_str(&format!(
            "- {name} ({kind}) — {mentions} mention{plural}, last seen {last}\n",
            name = h.display_name,
            kind = h.kind,
            mentions = h.mentions_count,
            plural = if h.mentions_count == 1 { "" } else { "s" },
            last = h.last_seen,
        ));
        for snip in &h.recent_mention_snippets {
            let date = snip.occurred_at.split('T').next().unwrap_or(&snip.occurred_at);
            let date = date.split(' ').next().unwrap_or(date);
            out.push_str(&format!(
                "    [{date}] \"{}\"\n",
                snip.snippet.replace('"', "'")
            ));
        }
        if !h.related_entities.is_empty() {
            out.push_str("    related: ");
            let parts: Vec<String> = h
                .related_entities
                .iter()
                .map(|r| format!("{} ×{}", r.display_name, r.co_mention_count))
                .collect();
            out.push_str(&parts.join(", "));
            out.push('\n');
        }
    }
    out
}
