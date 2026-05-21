//! `recall_entity` — Tauri command for the capture-chip hover tooltip
//! (BRAIN.md Phase 4.5 #9). Returns a compact "what Travis remembers
//! about this entity" payload the frontend renders as a popover.
//!
//! Reuses memory::graph::build_hit's components: header counts, recent
//! mention snippets, claims, related entities. No new SQL — just one
//! Tauri command wrapping the existing graph helpers.

use serde::Serialize;
use tauri::State;

use crate::memory::claims;
use crate::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecallSummary {
    pub entity_id: i64,
    pub display_name: String,
    pub kind: String,
    pub mentions_count: i64,
    pub last_seen: String,
    pub confidence: String,
    pub claims: Vec<RecallClaim>,
    pub recent_snippets: Vec<RecallSnippet>,
    pub related: Vec<RecallRelated>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecallClaim {
    pub predicate: String,
    pub value: String,
    pub confidence: String,
    pub source: String,
    pub contested: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecallSnippet {
    pub occurred_at: String,
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecallRelated {
    pub entity_id: i64,
    pub display_name: String,
    pub kind: String,
    pub co_mention_count: i64,
}

#[tauri::command]
pub async fn recall_entity(
    state: State<'_, AppState>,
    entity_id: i64,
) -> Result<RecallSummary, String> {
    let pool = &state.db.pool;

    #[derive(sqlx::FromRow)]
    struct Header {
        display_name: String,
        kind: String,
        mentions_count: i64,
        last_seen: String,
        confidence: f32,
        workspace_id: i64,
    }
    let header: Header = sqlx::query_as(
        "SELECT display_name, kind, mentions_count, last_seen, confidence, workspace_id
         FROM entity WHERE id = ?1 AND archived_at IS NULL",
    )
    .bind(entity_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("load entity: {e}"))?
    .ok_or_else(|| format!("entity #{entity_id} not found or archived"))?;

    let band = crate::memory::graph::ConfidenceBand::from_metrics(
        header.mentions_count,
        header.confidence,
    );

    // Recent mention snippets (up to 3).
    let mention_rows: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT occurred_at, attributes_json
         FROM event
         WHERE entity_id = ?1 AND kind = 'mentioned'
         ORDER BY occurred_at DESC, id DESC
         LIMIT 3",
    )
    .bind(entity_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    let recent_snippets: Vec<RecallSnippet> = mention_rows
        .into_iter()
        .filter_map(|(occurred_at, attrs)| {
            let parsed: serde_json::Value = attrs
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or(serde_json::Value::Null);
            let snippet = parsed
                .get("snippet")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            snippet.map(|snippet| RecallSnippet { occurred_at, snippet })
        })
        .collect();

    // Top 3 related via mentioned_with.
    let related_rows: Vec<(i64, String, String, Option<String>)> = sqlx::query_as(
        "SELECT e.id, e.display_name, e.kind, r.attributes_json
         FROM relation r
         JOIN entity e
           ON e.id = CASE WHEN r.from_entity = ?1 THEN r.to_entity ELSE r.from_entity END
         WHERE r.kind = 'mentioned_with'
           AND (r.from_entity = ?1 OR r.to_entity = ?1)
           AND e.archived_at IS NULL
         LIMIT 20",
    )
    .bind(entity_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    let mut related: Vec<RecallRelated> = related_rows
        .into_iter()
        .map(|(id, name, kind, attrs)| {
            let parsed: serde_json::Value = attrs
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or(serde_json::Value::Null);
            let co = parsed
                .get("co_mention_count")
                .and_then(|v| v.as_i64())
                .unwrap_or(1);
            RecallRelated {
                entity_id: id,
                display_name: name,
                kind,
                co_mention_count: co,
            }
        })
        .collect();
    related.sort_by(|a, b| b.co_mention_count.cmp(&a.co_mention_count));
    related.truncate(3);

    // Active claims (up to 5).
    let claim_rows = claims::for_entity(pool, header.workspace_id, entity_id, 5).await;
    let claims_out: Vec<RecallClaim> = claim_rows
        .into_iter()
        .map(|c| RecallClaim {
            predicate: c.predicate,
            value: c.value,
            confidence: c.confidence,
            source: c.source,
            contested: c.contested == 1,
        })
        .collect();

    Ok(RecallSummary {
        entity_id,
        display_name: header.display_name,
        kind: header.kind,
        mentions_count: header.mentions_count,
        last_seen: header.last_seen,
        confidence: band.label().to_string(),
        claims: claims_out,
        recent_snippets,
        related,
    })
}
