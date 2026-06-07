//! Database access for travis_case + case_artifact tables.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Case {
    pub id: i64,
    pub workspace_id: i64,
    pub name: String,
    pub summary: Option<String>,
    pub status: String,
    pub parent_case_id: Option<i64>,
    pub conversation_ids_json: String,
    pub started_at: String,
    pub last_activity_at: String,
    pub closed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct CaseArtifact {
    pub id: i64,
    pub case_id: i64,
    pub kind: String,
    pub payload_json: String,
    pub document_id: Option<i64>,
    pub created_at: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseInput {
    pub name: String,
    pub summary: Option<String>,
    pub parent_case_id: Option<i64>,
}

/// Open a new case OR resume an existing one with the same name.
pub async fn upsert_open(
    pool: &SqlitePool,
    workspace_id: i64,
    input: CaseInput,
) -> anyhow::Result<Case> {
    let name = input.name.trim();
    if name.is_empty() {
        anyhow::bail!("case name required");
    }

    if let Some(existing) = find_by_name(pool, workspace_id, name).await {
        // Update summary if provided; reopen if closed
        if existing.status == "closed" {
            sqlx::query(
                "UPDATE travis_case
                 SET status = 'open',
                     summary = COALESCE(?1, summary),
                     last_activity_at = CURRENT_TIMESTAMP,
                     closed_at = NULL
                 WHERE id = ?2",
            )
            .bind(input.summary.as_deref())
            .bind(existing.id)
            .execute(pool)
            .await?;
        } else if let Some(s) = input.summary.as_deref() {
            sqlx::query(
                "UPDATE travis_case
                 SET summary = ?1, last_activity_at = CURRENT_TIMESTAMP
                 WHERE id = ?2",
            )
            .bind(s)
            .bind(existing.id)
            .execute(pool)
            .await?;
        }
        return get_one(pool, existing.id).await;
    }

    let id: i64 = sqlx::query_scalar(
        "INSERT INTO travis_case
            (workspace_id, name, summary, parent_case_id)
         VALUES (?1, ?2, ?3, ?4)
         RETURNING id",
    )
    .bind(workspace_id)
    .bind(name)
    .bind(input.summary.as_deref())
    .bind(input.parent_case_id)
    .fetch_one(pool)
    .await?;
    get_one(pool, id).await
}

pub async fn find_by_name(
    pool: &SqlitePool,
    workspace_id: i64,
    name: &str,
) -> Option<Case> {
    sqlx::query_as::<_, Case>(
        "SELECT id, workspace_id, name, summary, status, parent_case_id,
                conversation_ids_json, started_at, last_activity_at, closed_at
         FROM travis_case
         WHERE workspace_id = ?1 AND LOWER(TRIM(name)) = LOWER(TRIM(?2))
         ORDER BY status = 'open' DESC, last_activity_at DESC
         LIMIT 1",
    )
    .bind(workspace_id)
    .bind(name)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
}

pub async fn get_one(pool: &SqlitePool, id: i64) -> anyhow::Result<Case> {
    sqlx::query_as::<_, Case>(
        "SELECT id, workspace_id, name, summary, status, parent_case_id,
                conversation_ids_json, started_at, last_activity_at, closed_at
         FROM travis_case WHERE id = ?1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .map_err(Into::into)
}

pub async fn list_open(
    pool: &SqlitePool,
    visible_ws_ids: &[i64],
    limit: i64,
) -> Vec<Case> {
    if visible_ws_ids.is_empty() {
        return Vec::new();
    }
    let placeholders = (1..=visible_ws_ids.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT id, workspace_id, name, summary, status, parent_case_id,
                conversation_ids_json, started_at, last_activity_at, closed_at
         FROM travis_case
         WHERE status = 'open' AND workspace_id IN ({placeholders})
         ORDER BY last_activity_at DESC
         LIMIT ?{}",
        visible_ws_ids.len() + 1
    );
    let mut q = sqlx::query_as::<_, Case>(&sql);
    for ws in visible_ws_ids {
        q = q.bind(*ws);
    }
    q = q.bind(limit);
    q.fetch_all(pool).await.unwrap_or_default()
}

pub async fn close(pool: &SqlitePool, id: i64) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE travis_case
         SET status = 'closed',
             closed_at = CURRENT_TIMESTAMP,
             last_activity_at = CURRENT_TIMESTAMP
         WHERE id = ?1",
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn add_artifact(
    pool: &SqlitePool,
    case_id: i64,
    kind: &str,
    payload_json: &str,
    document_id: Option<i64>,
) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO case_artifact
            (case_id, kind, payload_json, document_id)
         VALUES (?1, ?2, ?3, ?4)
         RETURNING id",
    )
    .bind(case_id)
    .bind(kind)
    .bind(payload_json)
    .bind(document_id)
    .fetch_one(pool)
    .await?;
    // Bump activity on the parent case
    let _ = sqlx::query(
        "UPDATE travis_case
         SET last_activity_at = CURRENT_TIMESTAMP
         WHERE id = ?1",
    )
    .bind(case_id)
    .execute(pool)
    .await;
    Ok(id)
}

pub async fn recent_artifacts(
    pool: &SqlitePool,
    case_id: i64,
    limit: i64,
) -> Vec<CaseArtifact> {
    sqlx::query_as::<_, CaseArtifact>(
        "SELECT id, case_id, kind, payload_json, document_id, created_at
         FROM case_artifact
         WHERE case_id = ?1
         ORDER BY created_at DESC
         LIMIT ?2",
    )
    .bind(case_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
}

/// Format active cases for the system prompt block. Matches the
/// initiatives block style.
pub fn format_for_prompt(cases: &[Case]) -> String {
    if cases.is_empty() {
        return String::new();
    }
    let mut s = String::from(
        "ACTIVE CASES (multi-session work units — if the user references one of these by name or topic, you're continuing work on it; recall the summary and any recent decisions before responding):\n",
    );
    for c in cases {
        s.push_str(&format!("- {} (#{})", c.name, c.id));
        if let Some(summary) = c.summary.as_deref() {
            if !summary.trim().is_empty() {
                s.push_str(&format!(": {}", summary.trim()));
            }
        }
        s.push('\n');
    }
    s
}
