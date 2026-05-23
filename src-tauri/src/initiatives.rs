//! Initiatives — collaboration layer (BRAIN.md capability #4).
//!
//! An initiative is a typed cluster of related work that Travis and
//! the user navigate together across sessions: "April invoicing
//! push", "audit response", "NYCPS HS Math bid". Tasks and
//! conversations can optionally tag one; the journal prompt injects
//! the active list so Travis picks up where the user left off
//! without re-deriving context every turn.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Initiative {
    pub id: i64,
    pub workspace_id: i64,
    pub name: String,
    pub summary: Option<String>,
    pub status: String,
    pub owner_kind: Option<String>,
    pub owner_label: Option<String>,
    pub entity_id: Option<i64>,
    pub last_decision: Option<String>,
    pub open_questions: Option<String>,
    pub last_activity_at: Option<String>,
    pub closed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitiativeInput {
    pub id: Option<i64>,
    pub name: String,
    pub summary: Option<String>,
    pub owner_kind: Option<String>,
    pub owner_label: Option<String>,
    pub entity_id: Option<i64>,
    pub last_decision: Option<String>,
    pub open_questions: Option<String>,
}

/// List active initiatives for the visible workspace set, ordered by
/// most-recent activity. Caller passes the same `visible_ids` the
/// rest of the journal pipeline uses.
pub async fn list_active(pool: &SqlitePool, visible_ids: &[i64], limit: i64) -> Vec<Initiative> {
    if visible_ids.is_empty() {
        return Vec::new();
    }
    let placeholders = (1..=visible_ids.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT id, workspace_id, name, summary, status, owner_kind, owner_label,
                entity_id, last_decision, open_questions,
                last_activity_at, closed_at, created_at, updated_at
         FROM initiative
         WHERE status = 'active' AND workspace_id IN ({placeholders})
         ORDER BY COALESCE(last_activity_at, updated_at) DESC
         LIMIT ?{}",
        visible_ids.len() + 1
    );
    let mut q = sqlx::query_as::<_, Initiative>(&sql);
    for ws in visible_ids {
        q = q.bind(*ws);
    }
    q = q.bind(limit);
    q.fetch_all(pool).await.unwrap_or_default()
}

/// Find an initiative by name (case-insensitive) within a workspace.
/// Used by the resolver in the create_initiative action so re-issues
/// don't double-create.
pub async fn find_by_name(
    pool: &SqlitePool,
    workspace_id: i64,
    name: &str,
) -> Option<Initiative> {
    sqlx::query_as::<_, Initiative>(
        "SELECT id, workspace_id, name, summary, status, owner_kind, owner_label,
                entity_id, last_decision, open_questions,
                last_activity_at, closed_at, created_at, updated_at
         FROM initiative
         WHERE workspace_id = ?1 AND LOWER(TRIM(name)) = LOWER(TRIM(?2))
         LIMIT 1",
    )
    .bind(workspace_id)
    .bind(name)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
}

/// Insert if no name match, otherwise enrich the existing row with
/// any new fields. Returns the resulting row.
pub async fn upsert(
    pool: &SqlitePool,
    workspace_id: i64,
    input: InitiativeInput,
) -> anyhow::Result<Initiative> {
    let name = input.name.trim();
    if name.is_empty() {
        anyhow::bail!("initiative name is required");
    }

    if let Some(existing) = find_by_name(pool, workspace_id, name).await {
        sqlx::query(
            "UPDATE initiative
             SET summary = COALESCE(?1, summary),
                 owner_kind = COALESCE(?2, owner_kind),
                 owner_label = COALESCE(?3, owner_label),
                 entity_id = COALESCE(?4, entity_id),
                 last_decision = COALESCE(?5, last_decision),
                 open_questions = COALESCE(?6, open_questions),
                 last_activity_at = CURRENT_TIMESTAMP,
                 updated_at = CURRENT_TIMESTAMP
             WHERE id = ?7",
        )
        .bind(input.summary.as_deref())
        .bind(input.owner_kind.as_deref())
        .bind(input.owner_label.as_deref())
        .bind(input.entity_id)
        .bind(input.last_decision.as_deref())
        .bind(input.open_questions.as_deref())
        .bind(existing.id)
        .execute(pool)
        .await?;
        return Ok(get_one(pool, existing.id).await?);
    }

    let id: i64 = sqlx::query_scalar(
        "INSERT INTO initiative
            (workspace_id, name, summary, owner_kind, owner_label, entity_id,
             last_decision, open_questions, last_activity_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, CURRENT_TIMESTAMP) RETURNING id",
    )
    .bind(workspace_id)
    .bind(name)
    .bind(input.summary.as_deref())
    .bind(input.owner_kind.as_deref())
    .bind(input.owner_label.as_deref())
    .bind(input.entity_id)
    .bind(input.last_decision.as_deref())
    .bind(input.open_questions.as_deref())
    .fetch_one(pool)
    .await?;
    get_one(pool, id).await
}

pub async fn get_one(pool: &SqlitePool, id: i64) -> anyhow::Result<Initiative> {
    let row: Option<Initiative> = sqlx::query_as(
        "SELECT id, workspace_id, name, summary, status, owner_kind, owner_label,
                entity_id, last_decision, open_questions,
                last_activity_at, closed_at, created_at, updated_at
         FROM initiative WHERE id = ?1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    row.ok_or_else(|| anyhow::anyhow!("initiative {id} not found"))
}

pub async fn close(pool: &SqlitePool, id: i64) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE initiative
         SET status = 'closed',
             closed_at = CURRENT_TIMESTAMP,
             updated_at = CURRENT_TIMESTAMP
         WHERE id = ?1",
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Bump last_activity_at — called when a journal turn touches the
/// initiative (e.g. references the same entity or thread).
pub async fn touch(pool: &SqlitePool, id: i64) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE initiative
         SET last_activity_at = CURRENT_TIMESTAMP, updated_at = CURRENT_TIMESTAMP
         WHERE id = ?1",
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Render the ACTIVE INITIATIVES block for the journal prompt.
/// Empty when no active initiatives — caller appends without
/// separator.
pub fn format_for_prompt(initiatives: &[Initiative]) -> String {
    if initiatives.is_empty() {
        return String::new();
    }
    let mut s = String::from(
        "ACTIVE INITIATIVES (resume context — if the user's note touches one of these, weave it into your reply without restating; if not, ignore):\n",
    );
    for i in initiatives {
        s.push_str(&format!("- {}", i.name));
        if let Some(o) = i.owner_kind.as_deref() {
            let label = match (o, i.owner_label.as_deref()) {
                ("user", _) => "you holding".to_string(),
                ("travis", _) => "I'm holding".to_string(),
                ("external", Some(who)) if !who.is_empty() => format!("waiting on {who}"),
                ("external", _) => "waiting on external".into(),
                _ => String::new(),
            };
            if !label.is_empty() {
                s.push_str(&format!(" — {label}"));
            }
        }
        if let Some(d) = i.last_decision.as_deref() {
            let d = d.trim();
            if !d.is_empty() {
                s.push_str(&format!(". Last: {d}"));
            }
        }
        if let Some(q) = i.open_questions.as_deref() {
            let q = q.trim();
            if !q.is_empty() {
                s.push_str(&format!(". Open: {q}"));
            }
        }
        s.push('\n');
    }
    s
}
