use serde::Serialize;
use sqlx::SqlitePool;

use crate::db::UserProfile;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct EntityIndex {
    pub id: i64,
    pub kind: String,
    pub normalized_name: String,
    pub display_name: String,
    pub mentions_count: i64,
    pub first_seen: String,
    pub last_seen: String,
    pub attributes_json: Option<String>,
}

/// Lowercase, collapse internal whitespace, and strip basic punctuation.
pub fn normalize(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut prev_space = true;
    for ch in name.chars() {
        let lower = ch.to_lowercase().next().unwrap_or(ch);
        if lower.is_alphanumeric() {
            out.push(lower);
            prev_space = false;
        } else if lower.is_whitespace() || matches!(lower, '-' | '_' | '/' | '\\') {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        }
        // drop other punctuation entirely
    }
    out.trim().to_string()
}

/// Confidence levels used by ambient (LLM-driven) entity extraction.
/// Pack code that knows it's writing a hard record bypasses this and
/// calls `spine::entity::upsert` directly.
pub mod confidence {
    /// LLM extracted a name into a pack-declared kind bucket
    /// (coach, school, dept, tutor, student). The LLM is confident
    /// about the role from journal context, but no pack-table row
    /// exists yet — that's the difference from a typed CRUD upsert.
    pub const PACK_KINDED_AMBIENT: f64 = 0.7;

    /// LLM extracted a name without a pack-declared kind, into one
    /// of the generic person:unknown / place:unknown / org:unknown
    /// buckets. We saw a name; we don't know what role it plays.
    pub const GENERIC: f64 = 0.5;
}

/// Best-effort upsert of a mention. Errors are logged but not
/// propagated. Returns the entity row id on success so callers can
/// link a `mentioned` event back to it (Phase 4 slice 3).
///
/// `kind` is a soft string — packs declare what kinds they care
/// about. Anything goes through; junk kinds will just sit in the
/// spine until someone queries for them. The validation cost of an
/// allowlist isn't worth the loss of pack flexibility.
///
/// `initial_confidence` is used only on INSERT. ON CONFLICT we leave
/// confidence alone so an existing 1.0 (from a pack-projected row)
/// isn't downgraded by a later ambient mention. Slice 9 will manage
/// upgrades when the user answers categorisation prompts.
pub async fn record_mention(
    pool: &SqlitePool,
    workspace_id: i64,
    kind: &str,
    display_name: &str,
    initial_confidence: f64,
) -> Option<i64> {
    let trimmed = display_name.trim();
    if trimmed.is_empty() {
        return None;
    }
    let normalized = normalize(trimmed);
    if normalized.is_empty() {
        return None;
    }
    if kind.trim().is_empty() {
        tracing::warn!("identity::record_mention: empty kind");
        return None;
    }

    // RETURNING gives us the id whether the upsert path was INSERT
    // or ON CONFLICT update. workspace_id and confidence are written
    // on insert; on conflict we only bump the count + last_seen.
    let res: Result<(i64,), _> = sqlx::query_as(
        "INSERT INTO entity
             (kind, normalized_name, display_name, workspace_id,
              confidence, mentions_count, first_seen, last_seen)
         VALUES (?1, ?2, ?3, ?4, ?5, 1, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
         ON CONFLICT(kind, normalized_name) DO UPDATE SET
            mentions_count = mentions_count + 1,
            last_seen = CURRENT_TIMESTAMP
         RETURNING id",
    )
    .bind(kind)
    .bind(&normalized)
    .bind(trimmed)
    .bind(workspace_id)
    .bind(initial_confidence.clamp(0.0, 1.0))
    .fetch_one(pool)
    .await;

    match res {
        Ok((id,)) => Some(id),
        Err(e) => {
            tracing::warn!("identity::record_mention failed for {kind}/{trimmed}: {e}");
            None
        }
    }
}

pub async fn list_top(
    pool: &SqlitePool,
    kind: Option<&str>,
    limit: i64,
) -> anyhow::Result<Vec<EntityIndex>> {
    let limit = limit.clamp(1, 500);
    let rows = sqlx::query_as::<_, EntityIndex>(
        "SELECT id, kind, normalized_name, display_name, mentions_count, first_seen, last_seen, attributes_json
         FROM entity
         WHERE (?1 IS NULL OR kind = ?1)
         ORDER BY mentions_count DESC, last_seen DESC, id DESC
         LIMIT ?2",
    )
    .bind(kind)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

async fn top_names(pool: &SqlitePool, kind: &str, limit: i64) -> anyhow::Result<Vec<String>> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT display_name FROM entity
         WHERE kind = ?1
         ORDER BY mentions_count DESC, last_seen DESC, id DESC
         LIMIT ?2",
    )
    .bind(kind)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(n,)| n).collect())
}

/// Build a short user blurb to inject into LLM system prompts.
pub async fn build_profile_blurb(
    pool: &SqlitePool,
    profile: &UserProfile,
) -> anyhow::Result<String> {
    let coaches = top_names(pool, "coach", 5).await.unwrap_or_default();
    let schools = top_names(pool, "school", 5).await.unwrap_or_default();
    let depts = top_names(pool, "dept", 5).await.unwrap_or_default();

    let mut out = format!(
        "User: {name}, {role} at {org}.",
        name = profile.name,
        role = profile.role,
        org = profile.org,
    );
    if !coaches.is_empty() {
        out.push_str(&format!(" Known coaches: {}.", coaches.join(", ")));
    }
    if !schools.is_empty() {
        out.push_str(&format!(" Schools: {}.", schools.join(", ")));
    }
    if !depts.is_empty() {
        out.push_str(&format!(" Depts: {}.", depts.join(", ")));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_basic() {
        assert_eq!(normalize("PS 142"), "ps 142");
        assert_eq!(normalize("  John   Doe  "), "john doe");
        assert_eq!(normalize("Dept. of Finance"), "dept of finance");
        assert_eq!(normalize("MS-88"), "ms 88");
        assert_eq!(normalize("O'Brien"), "obrien");
    }
}
