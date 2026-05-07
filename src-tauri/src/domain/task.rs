use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use super::DomainError;
use crate::behavioral;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: i64,
    pub title: String,
    pub description: Option<String>,
    pub status: String,
    pub priority: i64,
    pub due_at: Option<String>,
    /// Forward-looking link into the spine `entity` table. New code
    /// should prefer this over the legacy `link_kind` / `link_id`
    /// pair, which stays populated for backwards compatibility until
    /// step 8 of the pack refactor (PACKS_AUDIT.md) backfills.
    pub entity_id: Option<i64>,
    pub link_kind: Option<String>,
    pub link_id: Option<i64>,
    pub source: String,
    pub completed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskInput {
    pub id: Option<i64>,
    pub title: String,
    pub description: Option<String>,
    pub priority: Option<i64>,
    pub due_at: Option<String>,
    #[serde(default)]
    pub entity_id: Option<i64>,
    pub link_kind: Option<String>,
    pub link_id: Option<i64>,
    pub source: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TaskFilter {
    pub status: Option<String>,
    pub link_kind: Option<String>,
    pub link_id: Option<i64>,
}

pub async fn list(pool: &SqlitePool, filter: TaskFilter) -> Result<Vec<Task>, DomainError> {
    let rows = sqlx::query_as::<_, Task>(
        "SELECT id, title, description, status, priority, due_at,
                entity_id, link_kind, link_id,
                source, completed_at, created_at, updated_at
         FROM task
         WHERE (?1 IS NULL OR status = ?1)
           AND (?2 IS NULL OR link_kind = ?2)
           AND (?3 IS NULL OR link_id = ?3)
         ORDER BY
           CASE status WHEN 'open' THEN 0 WHEN 'snoozed' THEN 1 WHEN 'done' THEN 2 ELSE 3 END,
           priority DESC,
           COALESCE(due_at, '9999'),
           id DESC",
    )
    .bind(&filter.status)
    .bind(&filter.link_kind)
    .bind(filter.link_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn upsert(pool: &SqlitePool, input: TaskInput) -> Result<Task, DomainError> {
    let title = input.title.trim().to_string();
    if title.is_empty() {
        return Err(DomainError::invalid("title is required"));
    }
    if input.link_kind.is_some() != input.link_id.is_some() {
        return Err(DomainError::invalid("link_kind and link_id must be set together"));
    }

    let priority = input.priority.unwrap_or(0);
    let source = input.source.unwrap_or_else(|| "manual".into());

    let was_new = input.id.is_none();
    let id = match input.id {
        Some(id) => {
            sqlx::query(
                "UPDATE task SET title=?1, description=?2, priority=?3, due_at=?4,
                    entity_id=?5, link_kind=?6, link_id=?7, source=?8,
                    updated_at=CURRENT_TIMESTAMP
                 WHERE id=?9",
            )
            .bind(&title)
            .bind(&input.description)
            .bind(priority)
            .bind(&input.due_at)
            .bind(input.entity_id)
            .bind(&input.link_kind)
            .bind(input.link_id)
            .bind(&source)
            .bind(id)
            .execute(pool)
            .await?;
            id
        }
        None => sqlx::query(
            "INSERT INTO task (title, description, priority, due_at,
                               entity_id, link_kind, link_id, source)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )
        .bind(&title)
        .bind(&input.description)
        .bind(priority)
        .bind(&input.due_at)
        .bind(input.entity_id)
        .bind(&input.link_kind)
        .bind(input.link_id)
        .bind(&source)
        .execute(pool)
        .await?
        .last_insert_rowid(),
    };

    let kind = if was_new { "task_created" } else { "task_updated" };
    let _ = behavioral::log_event(pool, kind, Some("task"), Some(id), None).await;

    fetch_one(pool, id).await
}

pub async fn fetch_one(pool: &SqlitePool, id: i64) -> Result<Task, DomainError> {
    let row = sqlx::query_as::<_, Task>(
        "SELECT id, title, description, status, priority, due_at,
                entity_id, link_kind, link_id,
                source, completed_at, created_at, updated_at
         FROM task WHERE id=?1",
    )
    .bind(id)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

pub async fn set_status(pool: &SqlitePool, id: i64, status: &str) -> Result<Task, DomainError> {
    if !["open", "done", "snoozed", "dropped"].contains(&status) {
        return Err(DomainError::invalid(format!("unknown task status: {status}")));
    }
    let completed_clause = if status == "done" {
        ", completed_at = COALESCE(completed_at, CURRENT_TIMESTAMP)"
    } else if status == "open" {
        ", completed_at = NULL"
    } else {
        ""
    };
    let sql = format!(
        "UPDATE task SET status = ?1, updated_at = CURRENT_TIMESTAMP{completed_clause}
         WHERE id = ?2"
    );
    sqlx::query(&sql).bind(status).bind(id).execute(pool).await?;

    let kind = if status == "done" { "task_completed" } else { "task_status_changed" };
    let _ = behavioral::log_event(pool, kind, Some("task"), Some(id), None).await;

    fetch_one(pool, id).await
}

pub async fn delete(pool: &SqlitePool, id: i64) -> Result<(), DomainError> {
    sqlx::query("DELETE FROM task WHERE id=?1").bind(id).execute(pool).await?;
    Ok(())
}
