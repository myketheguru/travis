use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use super::DomainError;
use crate::behavioral;
use crate::workspaces;

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
    pub workspace_id: i64,
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

const SELECT_FIELDS: &str =
    "id, title, description, status, priority, due_at, entity_id, \
     link_kind, link_id, source, completed_at, created_at, updated_at, \
     workspace_id";

fn ws_in_clause(start: usize, n: usize) -> String {
    (start..start + n)
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ")
}

pub async fn list(
    pool: &SqlitePool,
    workspace: &workspaces::State,
    filter: TaskFilter,
) -> Result<Vec<Task>, DomainError> {
    let n_ws = workspace.visible_ids.len();
    let ws_clause = ws_in_clause(1, n_ws);
    let f1 = n_ws + 1;
    let f2 = n_ws + 2;
    let f3 = n_ws + 3;

    let sql = format!(
        "SELECT {SELECT_FIELDS}
         FROM task
         WHERE workspace_id IN ({ws_clause})
           AND (?{f1} IS NULL OR status = ?{f1})
           AND (?{f2} IS NULL OR link_kind = ?{f2})
           AND (?{f3} IS NULL OR link_id = ?{f3})
         ORDER BY
           CASE status WHEN 'open' THEN 0 WHEN 'snoozed' THEN 1 WHEN 'done' THEN 2 ELSE 3 END,
           priority DESC,
           COALESCE(due_at, '9999'),
           id DESC"
    );
    let mut q = sqlx::query_as::<_, Task>(&sql);
    for id in &workspace.visible_ids {
        q = q.bind(id);
    }
    q = q
        .bind(&filter.status)
        .bind(&filter.link_kind)
        .bind(filter.link_id);
    let rows = q.fetch_all(pool).await?;
    Ok(rows)
}

pub async fn upsert(
    pool: &SqlitePool,
    workspace: &workspaces::State,
    input: TaskInput,
) -> Result<Task, DomainError> {
    let title = input.title.trim().to_string();
    if title.is_empty() {
        return Err(DomainError::invalid("title is required"));
    }
    if input.link_kind.is_some() != input.link_id.is_some() {
        return Err(DomainError::invalid(
            "link_kind and link_id must be set together",
        ));
    }

    let priority = input.priority.unwrap_or(0);
    let source = input.source.unwrap_or_else(|| "manual".into());

    let was_new = input.id.is_none();
    let id = match input.id {
        Some(id) => {
            let n_ws = workspace.visible_ids.len();
            let ws_clause = ws_in_clause(10, n_ws);
            let sql = format!(
                "UPDATE task SET title=?1, description=?2, priority=?3, due_at=?4,
                    entity_id=?5, link_kind=?6, link_id=?7, source=?8,
                    updated_at=CURRENT_TIMESTAMP
                 WHERE id=?9 AND workspace_id IN ({ws_clause})"
            );
            let mut q = sqlx::query(&sql)
                .bind(&title)
                .bind(&input.description)
                .bind(priority)
                .bind(&input.due_at)
                .bind(input.entity_id)
                .bind(&input.link_kind)
                .bind(input.link_id)
                .bind(&source)
                .bind(id);
            for ws_id in &workspace.visible_ids {
                q = q.bind(ws_id);
            }
            let res = q.execute(pool).await?;
            if res.rows_affected() == 0 {
                return Err(DomainError::invalid(format!(
                    "task #{id} not found in any visible workspace"
                )));
            }
            id
        }
        None => sqlx::query(
            "INSERT INTO task (title, description, priority, due_at,
                               entity_id, link_kind, link_id, source, workspace_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )
        .bind(&title)
        .bind(&input.description)
        .bind(priority)
        .bind(&input.due_at)
        .bind(input.entity_id)
        .bind(&input.link_kind)
        .bind(input.link_id)
        .bind(&source)
        .bind(workspace.active_id)
        .execute(pool)
        .await?
        .last_insert_rowid(),
    };

    let kind = if was_new { "task_created" } else { "task_updated" };
    let _ = behavioral::log_event(pool, kind, Some("task"), Some(id), None).await;

    fetch_one(pool, workspace, id).await
}

pub async fn fetch_one(
    pool: &SqlitePool,
    workspace: &workspaces::State,
    id: i64,
) -> Result<Task, DomainError> {
    let n_ws = workspace.visible_ids.len();
    let ws_clause = ws_in_clause(2, n_ws);
    let sql = format!(
        "SELECT {SELECT_FIELDS} FROM task
         WHERE id=?1 AND workspace_id IN ({ws_clause})"
    );
    let mut q = sqlx::query_as::<_, Task>(&sql).bind(id);
    for ws_id in &workspace.visible_ids {
        q = q.bind(ws_id);
    }
    let row = q.fetch_one(pool).await?;
    Ok(row)
}

pub async fn set_status(
    pool: &SqlitePool,
    workspace: &workspaces::State,
    id: i64,
    status: &str,
) -> Result<Task, DomainError> {
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

    let n_ws = workspace.visible_ids.len();
    let ws_clause = ws_in_clause(3, n_ws);
    let sql = format!(
        "UPDATE task SET status = ?1, updated_at = CURRENT_TIMESTAMP{completed_clause}
         WHERE id = ?2 AND workspace_id IN ({ws_clause})"
    );
    let mut q = sqlx::query(&sql).bind(status).bind(id);
    for ws_id in &workspace.visible_ids {
        q = q.bind(ws_id);
    }
    q.execute(pool).await?;

    let kind = if status == "done" {
        "task_completed"
    } else {
        "task_status_changed"
    };
    let _ = behavioral::log_event(pool, kind, Some("task"), Some(id), None).await;

    fetch_one(pool, workspace, id).await
}

pub async fn delete(
    pool: &SqlitePool,
    workspace: &workspaces::State,
    id: i64,
) -> Result<(), DomainError> {
    let n_ws = workspace.visible_ids.len();
    let ws_clause = ws_in_clause(2, n_ws);
    let sql = format!(
        "DELETE FROM task WHERE id=?1 AND workspace_id IN ({ws_clause})"
    );
    let mut q = sqlx::query(&sql).bind(id);
    for ws_id in &workspace.visible_ids {
        q = q.bind(ws_id);
    }
    q.execute(pool).await?;
    Ok(())
}
