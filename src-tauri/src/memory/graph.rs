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
    /// Graded certainty Travis can quote rather than asserting flat.
    pub confidence: ConfidenceBand,
    /// Persisted reasoning conclusions about this entity (Phase 4.5 #7).
    /// Up to 5 active claims sorted by confidence then recency. Empty
    /// when the entity is new or no reasoning has fired yet.
    pub claims: Vec<ClaimSummary>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimSummary {
    pub predicate: String,
    pub value: String,
    pub confidence: String,
    pub source: String,
    pub contested: bool,
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

/// Confidence band derived from mentions_count + entity.confidence
/// + co_mention edge count. Surfaces in the prompt so Travis can
/// express graded certainty rather than stating things flat
/// (BRAIN.md Phase 4.5 #5).
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ConfidenceBand {
    High,
    Medium,
    Low,
}

impl ConfidenceBand {
    /// `mentions_count` is the raw mention tally; `entity_confidence`
    /// is the 0.0–1.0 score the capture pipeline assigned (typed=1.0,
    /// pack-kinded=0.7, generic=0.5). High = many mentions + typed
    /// origin; Low = few mentions + generic origin.
    pub fn from_metrics(mentions_count: i64, entity_confidence: f32) -> Self {
        if mentions_count >= 8 && entity_confidence >= 0.9 {
            ConfidenceBand::High
        } else if mentions_count >= 3 || entity_confidence >= 0.7 {
            ConfidenceBand::Medium
        } else {
            ConfidenceBand::Low
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ConfidenceBand::High => "high",
            ConfidenceBand::Medium => "medium",
            ConfidenceBand::Low => "low",
        }
    }
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
    // Header — display_name + counts + confidence score. Cheap; one row.
    let header: Result<Option<(String, i64, String, f32)>, _> = sqlx::query_as(
        "SELECT display_name, mentions_count, last_seen, confidence
         FROM entity WHERE id = ?1 AND archived_at IS NULL",
    )
    .bind(entity_id)
    .fetch_optional(pool)
    .await;
    let (display_name, mentions_count, last_seen, entity_confidence) = match header {
        Ok(Some(row)) => row,
        _ => return None,
    };
    let confidence = ConfidenceBand::from_metrics(mentions_count, entity_confidence);

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

    // Active claims for this entity (Phase 4.5 #7).
    let workspace_id: i64 =
        sqlx::query_scalar("SELECT workspace_id FROM entity WHERE id = ?1")
            .bind(entity_id)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten()
            .unwrap_or(0);
    let claim_rows = crate::memory::claims::for_entity(pool, workspace_id, entity_id, 5).await;
    let claims: Vec<ClaimSummary> = claim_rows
        .into_iter()
        .map(|c| ClaimSummary {
            predicate: c.predicate,
            value: c.value,
            confidence: c.confidence,
            source: c.source,
            contested: c.contested == 1,
        })
        .collect();

    Some(GraphHit {
        entity_id,
        display_name,
        kind,
        mentions_count,
        last_seen,
        recent_events,
        recent_mention_snippets,
        related_entities: related,
        confidence,
        claims,
    })
}

/// Embedding-based fuzzy retrieval (BRAIN.md Phase 4.5 item 1).
///
/// Embeds `query` and cosine-sims against every workspace-visible
/// non-archived entity that has an embedding_vector. Returns the top
/// `limit` matches with similarity ≥ `min_score`, each built into a
/// full GraphHit so callers can drop the result straight into the
/// LLM prompt alongside name-resolved hits.
///
/// This is the path for resolving "the coach who teaches PS 142",
/// "that parent from last month", or pronominal references — the
/// existing name-based `retrieve` returns empty for anything that
/// isn't a literal name match.
///
/// Cost shape: O(n × 384) per query; n = entities in scope. At
/// thousands of entities this is sub-100ms. If telemetry shows
/// retrieval cost dominating, swap in `sqlite-vec` or precompute
/// an in-memory index — the surface API stays the same.
pub async fn retrieve_semantic(
    pool: &SqlitePool,
    workspace_ids: &[i64],
    query: &str,
    limit: usize,
    min_score: f32,
) -> Vec<GraphHit> {
    let query = query.trim();
    if workspace_ids.is_empty() || query.is_empty() || limit == 0 {
        return Vec::new();
    }

    let q_vec = match crate::memory::embedder::embed_one(query) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("retrieve_semantic embed failed: {e}");
            return Vec::new();
        }
    };

    // Pull entity_id + display_name + kind + embedding_vector +
    // last_seen for every visible non-archived entity. Skip rows
    // without an embedding (the indexer hasn't reached them yet).
    let placeholders = (1..=workspace_ids.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT id, display_name, kind, embedding_vector, last_seen
         FROM entity
         WHERE archived_at IS NULL
           AND embedding_vector IS NOT NULL
           AND workspace_id IN ({placeholders})"
    );
    let mut q = sqlx::query_as::<_, (i64, String, String, Vec<u8>, String)>(&sql);
    for ws in workspace_ids {
        q = q.bind(*ws);
    }
    let rows = match q.fetch_all(pool).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("retrieve_semantic query failed: {e}");
            return Vec::new();
        }
    };

    let mut scored: Vec<(f32, i64, String)> = Vec::with_capacity(rows.len());
    for (id, _name, kind, blob, last_seen) in rows {
        let entity_vec = crate::memory::embedder::bytes_to_vec(&blob);
        if entity_vec.is_empty() || entity_vec.len() != q_vec.len() {
            continue;
        }
        let sim = cosine_similarity(&q_vec, &entity_vec);
        if sim < min_score {
            continue;
        }
        // Apply recency decay (Phase 4.5 #8). Half-life 30 days —
        // an entity not mentioned in a month gets ~halved; not in
        // three months gets ~eighth. Cosine-only ranking would
        // surface long-stale entities ahead of recently-discussed
        // ones with slightly weaker name match; decay corrects that.
        let recency = recency_decay(&last_seen);
        let score = sim * recency;
        scored.push((score, id, kind));
    }
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(limit);

    let mut hits = Vec::with_capacity(scored.len());
    for (_score, id, kind) in scored {
        if let Some(hit) = build_hit(pool, id, kind).await {
            hits.push(hit);
        }
    }
    hits
}

/// Recency-decay factor for entity ranking (Phase 4.5 #8).
/// Half-life: 30 days. Returns 1.0 for "seen today", 0.5 for
/// "30 days ago", ~0.25 for "60 days ago". Falls back to 1.0
/// when last_seen can't be parsed (don't penalise unknowns).
pub fn recency_decay(last_seen: &str) -> f32 {
    // Try ISO 8601 RFC3339 first ("2026-05-20T12:34:56Z"); fall back
    // to SQLite's default datetime format ("2026-05-20 12:34:56").
    let parsed = chrono::DateTime::parse_from_rfc3339(last_seen)
        .ok()
        .map(|d| d.with_timezone(&chrono::Utc))
        .or_else(|| {
            chrono::NaiveDateTime::parse_from_str(last_seen, "%Y-%m-%d %H:%M:%S")
                .ok()
                .map(|n| chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(n, chrono::Utc))
        });
    let Some(when) = parsed else { return 1.0 };
    let age_secs = (chrono::Utc::now() - when).num_seconds() as f64;
    if age_secs <= 0.0 {
        return 1.0;
    }
    let half_life_secs = 30.0 * 86400.0;
    let factor = 0.5f64.powf(age_secs / half_life_secs);
    factor as f32
}

/// Cosine similarity for two equal-length f32 vectors. Returns 0.0
/// for zero-magnitude inputs rather than NaN.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    let denom = na.sqrt() * nb.sqrt();
    if denom == 0.0 {
        0.0
    } else {
        dot / denom
    }
}

/// Multi-hop traversal (Phase 4.5 #4). Find entities reachable from
/// `start_entity_id` via up to `max_hops` `mentioned_with` edges.
/// Returns up to `limit` distinct entities ordered by hop distance
/// (1-hop first), then co-mention strength on the closest edge.
///
/// Implemented as recursive CTE over the `relation` table. The
/// `mentioned_with` edge is undirected at the data level (the
/// canonical ordering means our endpoint can be either side), so the
/// CTE walks both endpoints. Hop count capped at 3 to keep the
/// blast radius bounded.
pub async fn neighbors(
    pool: &SqlitePool,
    workspace_ids: &[i64],
    start_entity_id: i64,
    max_hops: i64,
    limit: i64,
) -> Vec<MultiHopNeighbor> {
    if workspace_ids.is_empty() || max_hops <= 0 {
        return Vec::new();
    }
    let max_hops = max_hops.min(3).max(1);
    let limit = limit.clamp(1, 50);
    let ws_placeholders = (3..3 + workspace_ids.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "WITH RECURSIVE walk(entity_id, hops, co_mentions, prev_id) AS ( \
           SELECT ?1 AS entity_id, 0 AS hops, 0 AS co_mentions, NULL AS prev_id \
           UNION ALL \
           SELECT \
             CASE WHEN r.from_entity = w.entity_id THEN r.to_entity ELSE r.from_entity END AS entity_id, \
             w.hops + 1 AS hops, \
             COALESCE(json_extract(r.attributes_json, '$.co_mention_count'), 1) AS co_mentions, \
             w.entity_id AS prev_id \
           FROM walk w \
           JOIN relation r \
             ON r.kind = 'mentioned_with' \
            AND (r.from_entity = w.entity_id OR r.to_entity = w.entity_id) \
           WHERE w.hops < ?2 \
         ) \
         SELECT DISTINCT w.entity_id, MIN(w.hops) AS min_hops, MAX(w.co_mentions) AS strength, \
                         e.display_name, e.kind \
         FROM walk w \
         JOIN entity e ON e.id = w.entity_id \
         WHERE w.entity_id != ?1 \
           AND e.archived_at IS NULL \
           AND e.workspace_id IN ({ws_placeholders}) \
         GROUP BY w.entity_id \
         ORDER BY min_hops ASC, strength DESC \
         LIMIT ?{}",
        3 + workspace_ids.len()
    );

    let mut q = sqlx::query_as::<_, (i64, i64, i64, String, String)>(&sql)
        .bind(start_entity_id)
        .bind(max_hops);
    for ws in workspace_ids {
        q = q.bind(*ws);
    }
    q = q.bind(limit);
    let rows = match q.fetch_all(pool).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("neighbors traversal failed: {e}");
            return Vec::new();
        }
    };
    rows.into_iter()
        .map(|(id, hops, strength, name, kind)| MultiHopNeighbor {
            entity_id: id,
            hops,
            strength,
            display_name: name,
            kind,
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MultiHopNeighbor {
    pub entity_id: i64,
    /// Shortest path length from the start entity (1, 2, or 3).
    pub hops: i64,
    /// Max co-mention count on the path's closest edge — proxy for
    /// "how strongly connected" they are.
    pub strength: i64,
    pub display_name: String,
    pub kind: String,
}

/// Render a list of GraphHits as the GRAPH MEMORY block injected
/// into the LLM's user message. Returns an empty string when there
/// are no hits — caller can append the result without a separator.
pub fn format_for_prompt(hits: &[GraphHit]) -> String {
    if hits.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "GRAPH MEMORY (entities Travis already knows about; confidence band tells you \
         how firmly to assert facts about each):\n",
    );
    for h in hits {
        out.push_str(&format!(
            "- {name} ({kind}, confidence: {conf}) — {mentions} mention{plural}, last seen {last}\n",
            name = h.display_name,
            kind = h.kind,
            conf = h.confidence.label(),
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
        for c in &h.claims {
            let contested = if c.contested { " [contested]" } else { "" };
            out.push_str(&format!(
                "    [{conf} confidence, {src}] {pred}: {val}{contested}\n",
                conf = c.confidence,
                src = c.source,
                pred = c.predicate,
                val = c.value,
            ));
        }
    }
    out
}
