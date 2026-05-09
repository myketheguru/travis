//! Entity embedding pipeline (Phase 4 slice 7).
//!
//! Background sweeper that keeps the `entity.embedding_vector` column
//! current. Runs every 5 minutes; finds entities that are either
//! never indexed or stale (>7 days since last index), embeds them
//! using fastembed (already loaded by the journal-entry indexer), and
//! writes the vector back. Capped at 50 entities per sweep so a
//! cold-start backlog doesn't swallow CPU.
//!
//! Slice 8 will read these vectors for graph-aware retrieval. Until
//! then the column sits unused — that's fine, the indexer makes the
//! data available without a flag day.

use std::sync::Arc;
use std::time::Duration;

use sqlx::SqlitePool;

use crate::db::Db;
use crate::memory::{embed_one, vec_to_bytes};

const SWEEP_INTERVAL_SECS: u64 = 300;
const STARTUP_DELAY_SECS: u64 = 60;
const BATCH_SIZE: i64 = 50;
/// Re-embed entities whose last index is older than this. Stops the
/// indexer from churning on entities that haven't changed materially
/// while still catching slow-drift cases (renamed entities, attribute
/// changes).
const STALENESS_THRESHOLD: &str = "-7 days";

/// Spawn the indexer. Cheap to start — the first sweep waits 60s so
/// the rest of app startup can complete first; subsequent sweeps tick
/// every 5 minutes. Failure inside a sweep is logged and the loop
/// continues.
pub fn spawn(db: Arc<Db>) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(STARTUP_DELAY_SECS)).await;
        let mut ticker = tokio::time::interval(Duration::from_secs(SWEEP_INTERVAL_SECS));
        ticker.tick().await; // skip immediate first tick
        loop {
            ticker.tick().await;
            match sweep(&db.pool).await {
                Ok(0) => {}
                Ok(n) => tracing::info!("graph indexer: embedded {n} entities"),
                Err(e) => tracing::warn!("graph indexer sweep failed: {e}"),
            }
        }
    });
}

/// Pull a batch of stale or never-indexed entities, embed each, write
/// the vector back. Returns the number of rows updated.
async fn sweep(pool: &SqlitePool) -> anyhow::Result<u64> {
    // Candidates: non-archived entities that are either never-indexed
    // or whose last index is older than STALENESS_THRESHOLD. Order by
    // mentions_count DESC so the most-active entities (which dominate
    // retrieval relevance) refresh first.
    let sql = format!(
        "SELECT id, display_name, attributes_json, mentions_count
         FROM entity
         WHERE archived_at IS NULL
           AND (
             embedding_indexed_at IS NULL
             OR datetime(embedding_indexed_at) < datetime('now', '{STALENESS_THRESHOLD}')
           )
         ORDER BY mentions_count DESC, last_seen DESC
         LIMIT {BATCH_SIZE}"
    );
    let rows: Vec<(i64, String, Option<String>, i64)> =
        sqlx::query_as(&sql).fetch_all(pool).await?;

    let mut indexed = 0u64;
    for (id, name, attrs, mentions) in rows {
        let text = build_embedding_text(&name, attrs.as_deref(), mentions);
        let vector = match embed_one(&text) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("entity {id} embed failed: {e}");
                continue;
            }
        };
        let bytes = vec_to_bytes(&vector);
        let res = sqlx::query(
            "UPDATE entity
             SET embedding_vector = ?1,
                 embedding_indexed_at = CURRENT_TIMESTAMP
             WHERE id = ?2",
        )
        .bind(&bytes)
        .bind(id)
        .execute(pool)
        .await;
        match res {
            Ok(r) if r.rows_affected() > 0 => indexed += 1,
            Ok(_) => {}
            Err(e) => tracing::warn!("entity {id} embed write failed: {e}"),
        }
    }
    Ok(indexed)
}

/// Build the text we feed to fastembed for an entity. Combines the
/// display name with any attribute payload — keeps embeddings
/// semantically meaningful for entities with rich attrs (e.g. an
/// invoice with a recipient and a school) while staying useful for
/// bare-name entities.
fn build_embedding_text(name: &str, attrs: Option<&str>, mentions: i64) -> String {
    let mut s = name.trim().to_string();
    if let Some(a) = attrs.map(str::trim).filter(|s| !s.is_empty()) {
        // Strip JSON braces/quotes for cleaner embedding text — the
        // model doesn't need the syntax, just the values.
        let cleaned: String = a
            .chars()
            .filter(|c| !matches!(*c, '{' | '}' | '"'))
            .collect();
        s.push(' ');
        s.push_str(cleaned.trim());
    }
    // Tiny popularity hint helps the model nudge similar-name
    // disambiguation toward the more-mentioned entity. Negligible
    // tokens; harmless if fastembed treats it as gibberish.
    if mentions > 0 {
        s.push_str(&format!(" (mentions:{mentions})"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedding_text_handles_bare_names() {
        assert_eq!(build_embedding_text("Maria", None, 0), "Maria");
        assert_eq!(
            build_embedding_text("Maria", None, 12),
            "Maria (mentions:12)"
        );
    }

    #[test]
    fn embedding_text_strips_json_syntax() {
        let attrs = Some(r#"{"role":"coach","school":"PS 142"}"#);
        let out = build_embedding_text("Maria", attrs, 5);
        assert!(!out.contains('{'));
        assert!(!out.contains('"'));
        assert!(out.contains("Maria"));
        assert!(out.contains("coach"));
        assert!(out.contains("PS 142"));
    }
}
