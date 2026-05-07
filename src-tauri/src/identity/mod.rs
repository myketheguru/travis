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

/// Best-effort upsert of a mention. Errors are logged but not propagated.
///
/// `kind` is a soft string — packs declare what kinds they care about.
/// Anything goes through; junk kinds will just sit in the spine until
/// someone queries for them. The validation cost of an allowlist isn't
/// worth the loss of pack flexibility.
pub async fn record_mention(pool: &SqlitePool, kind: &str, display_name: &str) {
    let trimmed = display_name.trim();
    if trimmed.is_empty() {
        return;
    }
    let normalized = normalize(trimmed);
    if normalized.is_empty() {
        return;
    }
    if kind.trim().is_empty() {
        tracing::warn!("identity::record_mention: empty kind");
        return;
    }

    let res = sqlx::query(
        "INSERT INTO entity (kind, normalized_name, display_name, mentions_count, first_seen, last_seen)
         VALUES (?1, ?2, ?3, 1, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
         ON CONFLICT(kind, normalized_name) DO UPDATE SET
            mentions_count = mentions_count + 1,
            last_seen = CURRENT_TIMESTAMP",
    )
    .bind(kind)
    .bind(&normalized)
    .bind(trimmed)
    .execute(pool)
    .await;

    if let Err(e) = res {
        tracing::warn!("identity::record_mention failed for {kind}/{trimmed}: {e}");
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
