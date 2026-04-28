pub mod http_sink;
pub mod sender;

use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;
use sqlx::SqlitePool;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct QueuedEvent {
    pub id: i64,
    pub kind: String,
    pub payload_json: String,
    pub created_at: String,
    pub sent_at: Option<String>,
    pub attempts: i64,
    pub last_error: Option<String>,
}

#[async_trait]
pub trait TelemetrySink: Send + Sync {
    async fn send(&self, events: &[QueuedEvent]) -> anyhow::Result<()>;
}

/// Best-effort enqueue. Never fails the caller — telemetry must not break the app.
pub async fn emit(pool: &SqlitePool, kind: &str, payload: Value) {
    let payload_str = payload.to_string();
    if let Err(e) = sqlx::query("INSERT INTO telemetry_event (kind, payload_json) VALUES (?1, ?2)")
        .bind(kind)
        .bind(&payload_str)
        .execute(pool)
        .await
    {
        tracing::warn!("telemetry emit failed: {e}");
    }
}

pub async fn pending(pool: &SqlitePool, limit: i64) -> Result<Vec<QueuedEvent>, sqlx::Error> {
    sqlx::query_as::<_, QueuedEvent>(
        "SELECT id, kind, payload_json, created_at, sent_at, attempts, last_error
         FROM telemetry_event
         WHERE sent_at IS NULL AND attempts < 5
         ORDER BY id ASC
         LIMIT ?1",
    )
    .bind(limit)
    .fetch_all(pool)
    .await
}

pub async fn mark_sent(pool: &SqlitePool, ids: &[i64]) -> Result<(), sqlx::Error> {
    for id in ids {
        sqlx::query("UPDATE telemetry_event SET sent_at = CURRENT_TIMESTAMP WHERE id = ?1")
            .bind(id)
            .execute(pool)
            .await?;
    }
    Ok(())
}

pub async fn mark_failed(
    pool: &SqlitePool,
    ids: &[i64],
    err: &str,
) -> Result<(), sqlx::Error> {
    for id in ids {
        sqlx::query(
            "UPDATE telemetry_event SET attempts = attempts + 1, last_error = ?1 WHERE id = ?2",
        )
        .bind(err)
        .bind(id)
        .execute(pool)
        .await?;
    }
    Ok(())
}
