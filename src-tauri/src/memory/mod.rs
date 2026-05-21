pub mod claims;
pub mod consolidate;
pub mod embedder;
pub mod graph;
pub mod working;

use serde::Serialize;
use sqlx::SqlitePool;

pub use embedder::{bytes_to_vec, embed_one, vec_to_bytes};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryHit {
    pub kind: String,
    pub source_id: i64,
    pub text: String,
    pub score: f64,
    pub similarity: f64,
    pub recency: f64,
    pub entity_match: f64,
    pub created_at: String,
}

/// Embed `text` and store it under (source_kind, source_id), replacing any prior row.
///
/// `workspace_id` is denormalised onto the embedding row so retrieval
/// can scan-and-filter by workspace without joining back to the
/// source table. Pass the workspace the source row belongs to.
pub async fn upsert_embedding(
    pool: &SqlitePool,
    workspace_id: i64,
    source_kind: &str,
    source_id: i64,
    text: &str,
) -> anyhow::Result<()> {
    // Embed first so we don't drop the existing row if embedding fails.
    let vector = embed_one(text)?;
    let bytes = vec_to_bytes(&vector);

    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM embedding WHERE source_kind = ?1 AND source_id = ?2")
        .bind(source_kind)
        .bind(source_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "INSERT INTO embedding (workspace_id, source_kind, source_id, text, vector)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(workspace_id)
    .bind(source_kind)
    .bind(source_id)
    .bind(text)
    .bind(bytes)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Index a journal entry's raw text under kind="journal".
pub async fn index_journal_entry(
    pool: &SqlitePool,
    workspace_id: i64,
    entry_id: i64,
    raw_text: &str,
) -> anyhow::Result<()> {
    upsert_embedding(pool, workspace_id, "journal", entry_id, raw_text).await
}

#[derive(sqlx::FromRow)]
struct EmbeddingRow {
    source_kind: String,
    source_id: i64,
    text: String,
    vector: Vec<u8>,
    created_at: String,
    #[allow(dead_code)]
    workspace_id: i64,
}

fn cosine(a: &[f32], b: &[f32]) -> f64 {
    if a.is_empty() || b.is_empty() || a.len() != b.len() {
        return 0.0;
    }
    let mut dot: f64 = 0.0;
    let mut na: f64 = 0.0;
    let mut nb: f64 = 0.0;
    for i in 0..a.len() {
        let x = a[i] as f64;
        let y = b[i] as f64;
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

/// Parse a SQLite-ish TEXT timestamp ("YYYY-MM-DD HH:MM:SS" or RFC3339-ish) to epoch seconds.
/// Best-effort: returns None if it can't parse.
fn parse_ts(s: &str) -> Option<i64> {
    // Try "YYYY-MM-DD HH:MM:SS" or "YYYY-MM-DDTHH:MM:SS".
    let s = s.trim();
    if s.len() < 10 {
        return None;
    }
    let bytes = s.as_bytes();
    let year: i32 = std::str::from_utf8(&bytes[0..4]).ok()?.parse().ok()?;
    if bytes[4] != b'-' {
        return None;
    }
    let month: u32 = std::str::from_utf8(&bytes[5..7]).ok()?.parse().ok()?;
    if bytes[7] != b'-' {
        return None;
    }
    let day: u32 = std::str::from_utf8(&bytes[8..10]).ok()?.parse().ok()?;

    let (hour, minute, second) = if s.len() >= 19 {
        let sep = bytes[10];
        if sep != b' ' && sep != b'T' {
            (0u32, 0u32, 0u32)
        } else {
            let h: u32 = std::str::from_utf8(&bytes[11..13]).ok()?.parse().ok()?;
            let m: u32 = std::str::from_utf8(&bytes[14..16]).ok()?.parse().ok()?;
            let sec: u32 = std::str::from_utf8(&bytes[17..19]).ok()?.parse().ok()?;
            (h, m, sec)
        }
    } else {
        (0, 0, 0)
    };

    Some(ymd_hms_to_epoch(year, month, day, hour, minute, second))
}

/// Convert UTC y/m/d h:m:s to epoch seconds (no timezone math; treats input as UTC).
fn ymd_hms_to_epoch(y: i32, m: u32, d: u32, h: u32, mi: u32, s: u32) -> i64 {
    // Days from civil date (Howard Hinnant's algorithm).
    let y = y as i64 - if (m as i64) <= 2 { 1 } else { 0 };
    let m = m as i64;
    let d = d as i64;
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as i64; // [0, 399]
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    days * 86400 + (h as i64) * 3600 + (mi as i64) * 60 + (s as i64)
}

fn now_epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Retrieve top-`limit` memory hits scored by 0.4*similarity + 0.3*recency + 0.3*entity_match.
///
/// `workspace_ids` scopes the candidate pool: pass the visible-set
/// from `AppState::workspace`. Sensitive workspaces collapse to a
/// single id; non-sensitive ones expand across cross-visible peers.
pub async fn retrieve(
    pool: &SqlitePool,
    workspace_ids: &[i64],
    query: &str,
    query_entities: &[String],
    limit: usize,
) -> anyhow::Result<Vec<MemoryHit>> {
    if limit == 0 || workspace_ids.is_empty() {
        return Ok(Vec::new());
    }

    let q_vec = embed_one(query)?;

    let placeholders = (1..=workspace_ids.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT source_kind, source_id, text, vector, created_at, workspace_id
         FROM embedding
         WHERE workspace_id IN ({placeholders})"
    );
    let mut q = sqlx::query_as::<_, EmbeddingRow>(&sql);
    for ws in workspace_ids {
        q = q.bind(ws);
    }
    let rows: Vec<EmbeddingRow> = q.fetch_all(pool).await?;

    if rows.is_empty() {
        return Ok(Vec::new());
    }

    let now = now_epoch();
    let entities_lower: Vec<String> = query_entities
        .iter()
        .map(|e| e.to_lowercase())
        .filter(|e| !e.is_empty())
        .collect();

    let mut hits: Vec<MemoryHit> = Vec::with_capacity(rows.len());
    for r in rows {
        let v = bytes_to_vec(&r.vector);
        let sim = cosine(&q_vec, &v);
        // Map cosine [-1, 1] -> [0, 1] for blending (negative similarity shouldn't help).
        let sim_norm = ((sim + 1.0) / 2.0).clamp(0.0, 1.0);

        let recency = match parse_ts(&r.created_at) {
            Some(ts) => {
                let age_secs = (now - ts).max(0) as f64;
                let age_days = age_secs / 86400.0;
                (-age_days / 14.0).exp().clamp(0.0, 1.0)
            }
            None => 0.0,
        };

        let text_lower = r.text.to_lowercase();
        let entity_match = if entities_lower.iter().any(|e| text_lower.contains(e)) {
            1.0
        } else {
            0.0
        };

        let score = 0.4 * sim_norm + 0.3 * recency + 0.3 * entity_match;

        hits.push(MemoryHit {
            kind: r.source_kind,
            source_id: r.source_id,
            text: r.text,
            score,
            similarity: sim,
            recency,
            entity_match,
            created_at: r.created_at,
        });
    }

    hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    hits.truncate(limit);
    Ok(hits)
}
