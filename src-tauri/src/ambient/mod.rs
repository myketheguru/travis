//! Ambient transcript persistence — v0.28.4.
//!
//! When ambient listening is on, every VAD-bounded utterance is
//! transcribed and stored here. The `get_ambient_transcripts` tool
//! lets the LLM query recent captures by time window so it can answer
//! "what was decided in the meeting?" or "what did they say about
//! Q4?".
//!
//! Storage is intentionally minimal: just text + occurred_at. No
//! entity linkage, no session grouping, no speaker diarization yet.
//! Those are follow-ups.

pub mod cmd;

use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AmbientTranscript {
    pub id: i64,
    pub text: String,
    pub occurred_at: String,
}

/// Insert a single ambient transcript. Returns the new row id.
/// Silently ignores empty text.
pub async fn insert(pool: &SqlitePool, text: &str) -> anyhow::Result<Option<i64>> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let row = sqlx::query("INSERT INTO ambient_transcript (text) VALUES (?1) RETURNING id")
        .bind(trimmed)
        .fetch_one(pool)
        .await?;
    let id: i64 = row.try_get(0)?;
    Ok(Some(id))
}

/// Fetch ambient transcripts captured within the last `minutes`.
/// Newest first. Caps at `limit` rows.
pub async fn recent(
    pool: &SqlitePool,
    minutes: i64,
    limit: i64,
) -> anyhow::Result<Vec<AmbientTranscript>> {
    let minutes = minutes.clamp(1, 24 * 60 * 7);
    let limit = limit.clamp(1, 500);
    let delta = format!("-{minutes} minutes");
    let rows = sqlx::query(
        "SELECT id, text, occurred_at
         FROM ambient_transcript
         WHERE occurred_at >= datetime('now', ?1)
         ORDER BY occurred_at DESC
         LIMIT ?2",
    )
    .bind(delta)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    let out = rows
        .into_iter()
        .map(|r| AmbientTranscript {
            id: r.try_get("id").unwrap_or(0),
            text: r.try_get("text").unwrap_or_default(),
            occurred_at: r.try_get("occurred_at").unwrap_or_default(),
        })
        .collect();
    Ok(out)
}

/// Delete transcripts older than `days`. Called by a housekeeping
/// background task to keep the table bounded.
pub async fn prune_older_than(pool: &SqlitePool, days: i64) -> anyhow::Result<u64> {
    let days = days.max(1);
    let delta = format!("-{days} days");
    let res = sqlx::query("DELETE FROM ambient_transcript WHERE occurred_at < datetime('now', ?1)")
        .bind(delta)
        .execute(pool)
        .await?;
    Ok(res.rows_affected())
}
