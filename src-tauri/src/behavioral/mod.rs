use std::collections::HashMap;

use serde::Serialize;
use sqlx::SqlitePool;

/// Append a row to `event_log`. Best-effort: errors are logged via `tracing::warn!`
/// and never propagated to the caller, so a failure here cannot break the
/// originating user action (task save, invoice transition, etc.).
pub async fn log_event(
    pool: &SqlitePool,
    kind: &str,
    ref_kind: Option<&str>,
    ref_id: Option<i64>,
    payload: Option<&serde_json::Value>,
) -> Result<(), sqlx::Error> {
    let payload_str = payload.map(|v| v.to_string());
    let res = sqlx::query(
        "INSERT INTO event_log (kind, ref_kind, ref_id, payload_json)
         VALUES (?1, ?2, ?3, ?4)",
    )
    .bind(kind)
    .bind(ref_kind)
    .bind(ref_id)
    .bind(payload_str.as_deref())
    .execute(pool)
    .await;

    if let Err(e) = res {
        tracing::warn!("behavioral::log_event failed: kind={kind} err={e}");
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct EventRow {
    pub id: i64,
    pub kind: String,
    pub ref_kind: Option<String>,
    pub ref_id: Option<i64>,
    pub payload_json: Option<String>,
    pub created_at: String,
}

pub async fn list_events(
    pool: &SqlitePool,
    limit: i64,
    kind: Option<&str>,
) -> Result<Vec<EventRow>, sqlx::Error> {
    let rows = sqlx::query_as::<_, EventRow>(
        "SELECT id, kind, ref_kind, ref_id, payload_json, created_at
         FROM event_log
         WHERE (?1 IS NULL OR kind = ?1)
         ORDER BY created_at DESC, id DESC
         LIMIT ?2",
    )
    .bind(kind)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct DetectedPattern {
    pub id: i64,
    pub sequence_json: String,
    pub occurrences: i64,
    pub last_seen: String,
    pub suggested_at: Option<String>,
    pub dismissed_at: Option<String>,
    pub automated_at: Option<String>,
    pub created_at: String,
}

pub async fn list_patterns(pool: &SqlitePool) -> Result<Vec<DetectedPattern>, sqlx::Error> {
    let rows = sqlx::query_as::<_, DetectedPattern>(
        "SELECT id, sequence_json, occurrences, last_seen, suggested_at,
                dismissed_at, automated_at, created_at
         FROM detected_pattern
         ORDER BY last_seen DESC, id DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Slide a 3-window over the most recent 200 events (oldest -> newest), count
/// each `(kind1, kind2, kind3)` tuple, and upsert any tuple seen >= 3 times
/// into `detected_pattern`. Returns the number of distinct patterns at threshold.
pub async fn detect(pool: &SqlitePool) -> Result<usize, sqlx::Error> {
    // Pull last 200 events ordered ascending so windows reflect real chronology.
    let kinds: Vec<(String,)> = sqlx::query_as(
        "SELECT kind FROM (
            SELECT id, kind FROM event_log ORDER BY created_at DESC, id DESC LIMIT 200
         ) AS recent ORDER BY id ASC",
    )
    .fetch_all(pool)
    .await?;

    if kinds.len() < 3 {
        return Ok(0);
    }

    let kinds: Vec<String> = kinds.into_iter().map(|(k,)| k).collect();

    // Count windows. The HashMap key is the JSON-encoded sequence so the
    // serialization matches what we store and look up by.
    let mut counts: HashMap<String, i64> = HashMap::new();
    for w in kinds.windows(3) {
        let seq = serde_json::json!([w[0], w[1], w[2]]).to_string();
        *counts.entry(seq).or_insert(0) += 1;
    }

    let mut refreshed = 0usize;
    for (seq, occ) in counts.into_iter().filter(|(_, c)| *c >= 3) {
        sqlx::query(
            "INSERT INTO detected_pattern (sequence_json, occurrences, last_seen)
             VALUES (?1, ?2, CURRENT_TIMESTAMP)
             ON CONFLICT(sequence_json) DO UPDATE SET
                occurrences = excluded.occurrences,
                last_seen = CURRENT_TIMESTAMP",
        )
        .bind(&seq)
        .bind(occ)
        .execute(pool)
        .await?;
        refreshed += 1;
    }

    Ok(refreshed)
}
