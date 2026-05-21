//! Per-entity personality slots (BRAIN.md capability #3b).
//!
//! For frequently-mentioned `person:*` entities, derives:
//! - **contact_window**: when in the day mentions cluster — proxy for
//!   when the user thinks about / interacts with this person
//! - **style_hint**: light-touch tone label inferred from mention
//!   snippet phrasing (terse / chatty / mixed)
//! - **top_topics**: top co-mentioned entities (already in the graph
//!   via co_mention_count — we just bake the names in for cheap
//!   prompt access)
//!
//! Persisted under `entity.attributes_json` as a `personality`
//! sub-object so future readers don't conflict with existing
//! attribute keys. Surfaces in retrieval via GraphHit.
//!
//! Per BRAIN.md's privacy guardrails: never extracts "how to
//! influence" signals — only "how they prefer to be communicated
//! with". The fields are descriptive (when, how they sound, what
//! comes up around them), never prescriptive.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

const MIN_MENTIONS: i64 = 5;
const MAX_PER_TICK: i64 = 20;
const REFRESH_DAYS: i64 = 7;

/// Personality slot payload persisted under
/// `entity.attributes_json.personality`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PersonalitySlots {
    /// "9am-11am" / "afternoons" / "late evening" — when the user
    /// tends to mention this person.
    pub contact_window: Option<String>,
    /// Light tone hint: "terse" | "chatty" | "mixed".
    pub style_hint: Option<String>,
    /// Up to 3 top co-mentioned entity names (most-frequent first).
    #[serde(default)]
    pub top_topics: Vec<String>,
    /// Mention count this snapshot was built from.
    pub mention_sample_size: i64,
    /// When the slot was last refreshed.
    pub updated_at: String,
}

/// Run one tick. Selects up to [`MAX_PER_TICK`] qualifying entities
/// (person-kind, mentions ≥ [`MIN_MENTIONS`], slot stale or missing)
/// and refreshes their personality slots. Returns count updated.
pub async fn run_tick(pool: &SqlitePool) -> usize {
    #[derive(sqlx::FromRow)]
    struct Candidate {
        id: i64,
        attributes_json: Option<String>,
    }
    // Pick people the user mentions a lot, whose personality block
    // is either missing or older than REFRESH_DAYS.
    let rows: Vec<Candidate> = match sqlx::query_as(
        "SELECT id, attributes_json FROM entity
         WHERE archived_at IS NULL
           AND kind LIKE 'person%'
           AND mentions_count >= ?1
         ORDER BY mentions_count DESC, last_seen DESC
         LIMIT ?2",
    )
    .bind(MIN_MENTIONS)
    .bind(MAX_PER_TICK)
    .fetch_all(pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("entity_model candidate query failed: {e}");
            return 0;
        }
    };

    let mut updated = 0;
    for c in rows {
        if !is_stale(c.attributes_json.as_deref()) {
            continue;
        }
        if let Err(e) = refresh_one(pool, c.id, c.attributes_json.as_deref()).await {
            tracing::warn!("entity_model refresh for {}: {e}", c.id);
            continue;
        }
        updated += 1;
    }
    updated
}

fn is_stale(attributes_json: Option<&str>) -> bool {
    let Some(json) = attributes_json else { return true };
    let v: serde_json::Value = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(_) => return true,
    };
    let Some(slots) = v.get("personality").and_then(|p| p.as_object()) else {
        return true;
    };
    let Some(updated_at) = slots.get("updatedAt").and_then(|s| s.as_str()) else {
        return true;
    };
    let parsed = chrono::DateTime::parse_from_rfc3339(updated_at)
        .ok()
        .map(|d| d.with_timezone(&chrono::Utc))
        .or_else(|| {
            chrono::NaiveDateTime::parse_from_str(updated_at, "%Y-%m-%d %H:%M:%S")
                .ok()
                .map(|n| chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(n, chrono::Utc))
        });
    let Some(when) = parsed else { return true };
    let age = chrono::Utc::now() - when;
    age.num_days() >= REFRESH_DAYS
}

async fn refresh_one(
    pool: &SqlitePool,
    entity_id: i64,
    existing_attrs: Option<&str>,
) -> anyhow::Result<()> {
    // Pull mention timestamps + snippets for histogram + style hint.
    #[derive(sqlx::FromRow)]
    struct MentionRow {
        occurred_at: String,
        attributes_json: Option<String>,
    }
    let mentions: Vec<MentionRow> = sqlx::query_as(
        "SELECT occurred_at, attributes_json FROM event
         WHERE entity_id = ?1 AND kind = 'mentioned'
         ORDER BY occurred_at DESC
         LIMIT 50",
    )
    .bind(entity_id)
    .fetch_all(pool)
    .await?;
    if mentions.is_empty() {
        return Ok(());
    }

    // Hour histogram → contact_window.
    let mut hours = [0i64; 24];
    let mut snippets: Vec<String> = Vec::new();
    for m in &mentions {
        if let Some(h) = parse_hour(&m.occurred_at) {
            if h < 24 {
                hours[h as usize] += 1;
            }
        }
        if let Some(attrs) = m.attributes_json.as_deref() {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(attrs) {
                if let Some(s) = v.get("snippet").and_then(|s| s.as_str()) {
                    snippets.push(s.to_string());
                }
            }
        }
    }
    let contact_window = window_label(&hours);

    // Style hint: terse if avg-snippet-words < 12; chatty if > 25;
    // mixed otherwise. Snippets are the surrounding capture excerpt,
    // not the entity's own voice — but it correlates well with how
    // the user tends to talk *about* them.
    let style_hint = if snippets.is_empty() {
        None
    } else {
        let avg = snippets
            .iter()
            .map(|s| s.split_whitespace().count())
            .sum::<usize>() as f64
            / snippets.len() as f64;
        Some(if avg < 12.0 {
            "terse".to_string()
        } else if avg > 25.0 {
            "chatty".to_string()
        } else {
            "mixed".to_string()
        })
    };

    // Top topics: top 3 co-mentioned by count.
    let top_topics: Vec<String> = sqlx::query_as::<_, (String, i64)>(
        "SELECT e.display_name,
                COALESCE(json_extract(r.attributes_json, '$.co_mention_count'), 1) AS c
         FROM relation r
         JOIN entity e
           ON e.id = CASE WHEN r.from_entity = ?1 THEN r.to_entity ELSE r.from_entity END
         WHERE r.kind = 'mentioned_with'
           AND (r.from_entity = ?1 OR r.to_entity = ?1)
           AND e.archived_at IS NULL
         ORDER BY c DESC
         LIMIT 3",
    )
    .bind(entity_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|(n, _)| n)
    .collect();

    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let slots = PersonalitySlots {
        contact_window,
        style_hint,
        top_topics,
        mention_sample_size: mentions.len() as i64,
        updated_at: now,
    };
    let slot_value = serde_json::to_value(&slots)?;

    // Merge into existing attributes_json. Preserve keys we didn't write.
    let mut existing: serde_json::Value = existing_attrs
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
    if !existing.is_object() {
        existing = serde_json::Value::Object(serde_json::Map::new());
    }
    if let Some(obj) = existing.as_object_mut() {
        obj.insert("personality".to_string(), slot_value);
    }
    let merged = serde_json::to_string(&existing)?;

    sqlx::query(
        "UPDATE entity SET attributes_json = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
    )
    .bind(&merged)
    .bind(entity_id)
    .execute(pool)
    .await?;

    Ok(())
}

fn parse_hour(s: &str) -> Option<i64> {
    let bytes = s.as_bytes();
    if bytes.len() < 13 {
        return None;
    }
    let sep = bytes.get(10).copied()?;
    if sep != b' ' && sep != b'T' {
        return None;
    }
    std::str::from_utf8(&bytes[11..13]).ok()?.parse().ok()
}

fn window_label(hours: &[i64; 24]) -> Option<String> {
    let total: i64 = hours.iter().sum();
    if total == 0 {
        return None;
    }
    let mut buckets = [
        ("mornings (6am-noon)", 0i64),
        ("afternoons (noon-5pm)", 0i64),
        ("evenings (5pm-9pm)", 0i64),
        ("late hours (9pm-6am)", 0i64),
    ];
    for h in 0..24 {
        let n = hours[h];
        if (6..12).contains(&h) {
            buckets[0].1 += n;
        } else if (12..17).contains(&h) {
            buckets[1].1 += n;
        } else if (17..21).contains(&h) {
            buckets[2].1 += n;
        } else {
            buckets[3].1 += n;
        }
    }
    let mut sorted: Vec<_> = buckets.iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));
    let top = sorted[0];
    if top.1 * 100 / total >= 50 {
        Some(top.0.to_string())
    } else {
        // No dominant bucket — combine top 2.
        Some(format!("{} or {}", sorted[0].0, sorted[1].0))
    }
}

/// Read just the personality slot from an entity's attributes_json
/// blob (None when absent or invalid).
pub fn extract(attributes_json: Option<&str>) -> Option<PersonalitySlots> {
    let json = attributes_json?;
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let slot = v.get("personality")?;
    serde_json::from_value(slot.clone()).ok()
}
