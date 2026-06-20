//! Inferred picture of the user's world. v2 Phase 1.5+ replacement
//! for the v1 onboarding blurb.
//!
//! Where `user_model.rs` captures *how* the user works (rhythm,
//! cadence, peak hours), this module captures *what* the user works
//! on — which orgs they're active in, which people are recurring,
//! which projects are alive. None of it is asked at onboarding;
//! everything is derived from the entity graph that gets populated
//! as the user lives with Travis.
//!
//! Pipeline:
//!   1. Read `entity` rows: kind = org / person / project, not
//!      archived, confidence above noise threshold.
//!   2. Sort by recency (last_seen) + frequency (mentions_count).
//!   3. Bucket by kind. Keep the top N of each.
//!   4. Persist as JSON under meta.user_world_model so the prompt
//!      assembly can read it cheaply on every call.
//!
//! Output is intentionally a flat structured summary, not the raw
//! graph — the prompt sees "Active at: Acme (47), Henderson Trust
//! (12)" rather than every entity attribute.

use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

const META_KEY: &str = "user_world_model";
const MIN_CONFIDENCE: f64 = 0.55;
const MIN_MENTIONS: i64 = 2;
const TOP_ORGS: usize = 5;
const TOP_PEOPLE: usize = 10;
const TOP_PROJECTS: usize = 8;

/// What the prompt sees about who/what the user works with. Persisted
/// as JSON in meta. Stable enough to read across versions; new fields
/// land additively.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WorldModel {
    pub orgs: Vec<WorldEntry>,
    pub people: Vec<WorldEntry>,
    pub projects: Vec<WorldEntry>,
    /// ISO-8601 timestamp the model was rebuilt.
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WorldEntry {
    pub name: String,
    pub mentions: i64,
    /// ISO-8601 timestamp of the last mention.
    pub last_seen: String,
    /// 0.0–1.0 confidence in the kind tag. Low-confidence entries are
    /// already filtered out at build time but keeping the value lets
    /// us surface uncertainty in the UI later.
    pub confidence: f64,
}

/// Rebuild the world model from the entity graph and persist it. Idempotent;
/// safe to call on every tick of a daily background loop.
pub async fn refresh(pool: &SqlitePool) -> anyhow::Result<Option<WorldModel>> {
    let orgs = top_entries(pool, &["org", "company", "client"], TOP_ORGS).await?;
    let people = top_entries(pool, &["person", "contact"], TOP_PEOPLE).await?;
    let projects =
        top_entries(pool, &["project", "initiative", "deal"], TOP_PROJECTS).await?;

    if orgs.is_empty() && people.is_empty() && projects.is_empty() {
        return Ok(None);
    }

    let model = WorldModel {
        orgs,
        people,
        projects,
        updated_at: chrono::Utc::now().to_rfc3339(),
    };
    let json = serde_json::to_string(&model)?;
    // Use set_meta_from_remote so the result doesn't enqueue into
    // sync_outbox — the world model is a per-install derivation that
    // each device should rebuild from its own (synced) entity graph,
    // not push as a settings.set event.
    sqlx::query(
        "INSERT INTO meta (key, value, updated_at) VALUES (?1, ?2, CURRENT_TIMESTAMP)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = CURRENT_TIMESTAMP",
    )
    .bind(META_KEY)
    .bind(&json)
    .execute(pool)
    .await?;
    Ok(Some(model))
}

/// Read the most-recently-persisted world model. Returns None if it
/// hasn't been built yet (fresh install, no captures, etc.).
pub async fn load(pool: &SqlitePool) -> anyhow::Result<Option<WorldModel>> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT value FROM meta WHERE key = ?1")
            .bind(META_KEY)
            .fetch_optional(pool)
            .await?;
    Ok(row.and_then(|r| serde_json::from_str::<WorldModel>(&r.0).ok()))
}

/// Format the model as a short fragment for system-prompt injection.
/// Kept terse — Travis does NOT need a paragraph per entity, just
/// enough to ground "who/what is in this user's world right now."
///
/// Returns an empty string when the model has nothing useful to say
/// yet (cold start). Persona block treats that as a no-op append.
pub fn format_for_prompt(model: &WorldModel) -> String {
    let mut out = String::new();
    let any_orgs = !model.orgs.is_empty();
    let any_people = !model.people.is_empty();
    let any_projects = !model.projects.is_empty();
    if !any_orgs && !any_people && !any_projects {
        return out;
    }

    out.push_str("INFERRED CONTEXT (what Travis has learned about this user's world by working with them — never asked, always derived from use; treat as probabilistic, not authoritative):\n");

    if any_orgs {
        out.push_str("- Active orgs: ");
        out.push_str(&join_top(&model.orgs, 5));
        out.push('\n');
    }
    if any_people {
        out.push_str("- Recurring people: ");
        out.push_str(&join_top(&model.people, 8));
        out.push('\n');
    }
    if any_projects {
        out.push_str("- Live projects: ");
        out.push_str(&join_top(&model.projects, 6));
        out.push('\n');
    }
    out.push_str(
        "When the user's reference is ambiguous against this picture (\"Anderson\" \
         when two Andersons are in scope, \"the team\" when two orgs are active), \
         ASK a specific clarifying question rather than guessing — name the candidates \
         (\"Did you mean Anderson at Acme, or the Henderson Trust board?\") so the \
         user just picks one. Trust explicit pushback over this inferred picture: \
         when the user corrects you, update for the rest of the turn immediately.\n",
    );
    out
}

fn join_top(entries: &[WorldEntry], limit: usize) -> String {
    entries
        .iter()
        .take(limit)
        .map(|e| format!("{} ({})", e.name, e.mentions))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Query the entity table for the top entries of a set of kinds,
/// ordered by recency × frequency. Sub-confidence and archived rows
/// drop. Sub-mention rows drop. Returns up to `limit` entries.
async fn top_entries(
    pool: &SqlitePool,
    kinds: &[&str],
    limit: usize,
) -> anyhow::Result<Vec<WorldEntry>> {
    let placeholders = std::iter::repeat("?")
        .take(kinds.len())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT display_name, mentions_count, last_seen, confidence
         FROM entity
         WHERE archived_at IS NULL
           AND confidence >= ?
           AND mentions_count >= ?
           AND kind IN ({placeholders})
         ORDER BY datetime(last_seen) DESC, mentions_count DESC
         LIMIT ?"
    );
    let mut q = sqlx::query(&sql).bind(MIN_CONFIDENCE).bind(MIN_MENTIONS);
    for k in kinds {
        q = q.bind(*k);
    }
    q = q.bind(limit as i64);

    let rows = q.fetch_all(pool).await?;
    let mut out: Vec<WorldEntry> = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(WorldEntry {
            name: row.get(0),
            mentions: row.get(1),
            last_seen: row.get(2),
            confidence: row.get(3),
        });
    }
    Ok(out)
}
