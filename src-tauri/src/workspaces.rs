//! Workspaces — scoped namespaces for operational data
//! (WORKSPACES.md). Every scoped table has a `workspace_id` column
//! tying its rows to one workspace; reads expand across the
//! "visible" set (active + cross-visible non-sensitive ones); writes
//! stamp the active workspace.
//!
//! `Workspace` is the typed row. The runtime state — active id +
//! visible ids — lives on [`crate::AppState::workspace`] so every
//! Tauri command can read it without per-call DB hits. The state
//! refreshes from DB on switch + on workspace-property changes.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

pub const SENSITIVE_CATEGORIES: &[&str] = &["health", "therapy", "legal", "finance"];

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub id: i64,
    pub slug: String,
    pub name: String,
    pub category: String,
    /// SQLite stores BOOL as INTEGER 0/1.
    pub cross_visible: i64,
    pub archived_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl Workspace {
    pub fn cross_visible_bool(&self) -> bool {
        self.cross_visible != 0
    }

    pub fn is_archived(&self) -> bool {
        self.archived_at.is_some()
    }

    /// Health, Therapy, Legal, Finance — these never auto-receive
    /// captures and never contribute to non-sensitive contexts'
    /// reads. The asymmetric isolation rule (WORKSPACES.md).
    pub fn is_sensitive(&self) -> bool {
        SENSITIVE_CATEGORIES.contains(&self.category.as_str())
    }

    /// Default `cross_visible` for a freshly-created workspace given
    /// its category. Sensitive categories default off; everything
    /// else defaults on. The user can toggle in Settings.
    pub fn default_cross_visible(category: &str) -> bool {
        !SENSITIVE_CATEGORIES.contains(&category)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceInput {
    /// Required at create. On update, ignored — slug is stable
    /// post-create (used in cross-references and telemetry).
    #[serde(default)]
    pub slug: Option<String>,
    pub name: String,
    /// Defaults to `personal` if not provided.
    #[serde(default)]
    pub category: Option<String>,
    /// Defaults from `Workspace::default_cross_visible(category)`.
    #[serde(default)]
    pub cross_visible: Option<bool>,
}

// ---------------------------------------------------------------------------
// State that lives on AppState — refreshed on switch / workspace-property
// change. Reads happen on every Tauri command path; writes only on switch.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct State {
    pub active_id: i64,
    pub visible_ids: Vec<i64>,
}

impl State {
    pub async fn load(pool: &SqlitePool) -> anyhow::Result<Self> {
        let active_id = read_active_id(pool).await?;
        let visible_ids = compute_visible_ids(pool, active_id).await?;
        Ok(Self {
            active_id,
            visible_ids,
        })
    }
}

// ---------------------------------------------------------------------------
// Lookups
// ---------------------------------------------------------------------------

const SELECT_FIELDS: &str =
    "id, slug, name, category, cross_visible, archived_at, created_at, updated_at";

pub async fn list_all(pool: &SqlitePool) -> anyhow::Result<Vec<Workspace>> {
    let sql = format!(
        "SELECT {SELECT_FIELDS} FROM workspace
         ORDER BY (archived_at IS NOT NULL), name COLLATE NOCASE"
    );
    let rows = sqlx::query_as::<_, Workspace>(&sql)
        .fetch_all(pool)
        .await?;
    Ok(rows)
}

pub async fn fetch_one(pool: &SqlitePool, id: i64) -> anyhow::Result<Workspace> {
    let sql = format!("SELECT {SELECT_FIELDS} FROM workspace WHERE id = ?1");
    let row = sqlx::query_as::<_, Workspace>(&sql)
        .bind(id)
        .fetch_one(pool)
        .await?;
    Ok(row)
}

pub async fn fetch_optional(
    pool: &SqlitePool,
    id: i64,
) -> anyhow::Result<Option<Workspace>> {
    let sql = format!("SELECT {SELECT_FIELDS} FROM workspace WHERE id = ?1");
    let row = sqlx::query_as::<_, Workspace>(&sql)
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row)
}

// ---------------------------------------------------------------------------
// Prompt context — surfaces the active workspace to the LLM so it can
// frame answers in the right "world" and avoid bleeding personal-life
// detail into a work workspace's responses.
// ---------------------------------------------------------------------------

/// Render the active-workspace block injected into system prompts.
/// Empty string when the workspace can't be fetched (extremely rare —
/// startup guarantees the row exists). Sensitive workspaces get an
/// extra line so the model knows to treat the context tighter.
pub async fn prompt_context_block(pool: &SqlitePool, active_id: i64) -> String {
    let Ok(ws) = fetch_one(pool, active_id).await else {
        return String::new();
    };
    let mut out = format!(
        "ACTIVE WORKSPACE: {} ({})",
        ws.name.trim(),
        ws.category
    );
    if ws.is_sensitive() {
        out.push_str(
            "\n  This is a sensitive workspace — keep responses scoped to it. \
             Don't mix in details from other workspaces, and don't speculate \
             outside what's been shared here.",
        );
    }
    out
}

// ---------------------------------------------------------------------------
// Active workspace + visibility
// ---------------------------------------------------------------------------

pub async fn read_active_id(pool: &SqlitePool) -> anyhow::Result<i64> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT value FROM meta WHERE key = 'active_workspace_id'")
            .fetch_optional(pool)
            .await?;
    let parsed = row
        .and_then(|(v,)| v.parse::<i64>().ok())
        .ok_or_else(|| anyhow::anyhow!("meta.active_workspace_id missing or invalid"))?;
    Ok(parsed)
}

async fn write_active_id(pool: &SqlitePool, id: i64) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO meta(key, value, updated_at)
         VALUES ('active_workspace_id', ?1, CURRENT_TIMESTAMP)
         ON CONFLICT(key) DO UPDATE SET
            value = excluded.value,
            updated_at = CURRENT_TIMESTAMP",
    )
    .bind(id.to_string())
    .execute(pool)
    .await?;
    Ok(())
}

/// Compute the visible-workspace set given the active id. Implements
/// the asymmetric isolation rule (WORKSPACES.md):
///
/// - Active is *sensitive* → only the active workspace is visible.
/// - Active is *non-sensitive* → active + every non-archived
///   non-sensitive workspace with cross_visible = 1.
///
/// Sensitive workspaces with cross_visible = true do not contribute
/// reads to non-sensitive active contexts. The flag only governs
/// non-sensitive ↔ non-sensitive sharing.
pub async fn compute_visible_ids(
    pool: &SqlitePool,
    active_id: i64,
) -> anyhow::Result<Vec<i64>> {
    let active = fetch_one(pool, active_id).await?;
    if active.is_archived() || active.is_sensitive() {
        return Ok(vec![active_id]);
    }

    let rows: Vec<(i64,)> = sqlx::query_as(
        "SELECT id FROM workspace
         WHERE archived_at IS NULL
           AND category NOT IN ('health','therapy','legal','finance')
           AND (id = ?1 OR cross_visible = 1)
         ORDER BY id",
    )
    .bind(active_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}

// ---------------------------------------------------------------------------
// Mutations
// ---------------------------------------------------------------------------

pub async fn create(pool: &SqlitePool, input: WorkspaceInput) -> anyhow::Result<Workspace> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        anyhow::bail!("workspace name is required");
    }
    let slug = input
        .slug
        .as_deref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| slugify(&name));
    if slug.is_empty() {
        anyhow::bail!("workspace slug normalises to empty — choose a different name");
    }
    let category = input
        .category
        .as_deref()
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "personal".into());
    validate_category(&category)?;
    let cross_visible = input
        .cross_visible
        .unwrap_or_else(|| Workspace::default_cross_visible(&category));

    let id = sqlx::query(
        "INSERT INTO workspace (slug, name, category, cross_visible)
         VALUES (?1, ?2, ?3, ?4)",
    )
    .bind(&slug)
    .bind(&name)
    .bind(&category)
    .bind(if cross_visible { 1_i64 } else { 0 })
    .execute(pool)
    .await?
    .last_insert_rowid();

    fetch_one(pool, id).await
}

pub async fn update(
    pool: &SqlitePool,
    id: i64,
    input: WorkspaceInput,
) -> anyhow::Result<Workspace> {
    let existing = fetch_one(pool, id).await?;
    let name = input.name.trim().to_string();
    if name.is_empty() {
        anyhow::bail!("workspace name is required");
    }
    let category = input
        .category
        .as_deref()
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| existing.category.clone());
    validate_category(&category)?;
    let cross_visible = input
        .cross_visible
        .unwrap_or_else(|| existing.cross_visible_bool());

    sqlx::query(
        "UPDATE workspace
         SET name = ?1, category = ?2, cross_visible = ?3,
             updated_at = CURRENT_TIMESTAMP
         WHERE id = ?4",
    )
    .bind(&name)
    .bind(&category)
    .bind(if cross_visible { 1_i64 } else { 0 })
    .bind(id)
    .execute(pool)
    .await?;

    fetch_one(pool, id).await
}

pub async fn archive(pool: &SqlitePool, id: i64) -> anyhow::Result<Workspace> {
    if id == 1 {
        anyhow::bail!("the default Personal workspace can't be archived");
    }
    sqlx::query(
        "UPDATE workspace
         SET archived_at = CURRENT_TIMESTAMP, updated_at = CURRENT_TIMESTAMP
         WHERE id = ?1 AND archived_at IS NULL",
    )
    .bind(id)
    .execute(pool)
    .await?;
    fetch_one(pool, id).await
}

pub async fn unarchive(pool: &SqlitePool, id: i64) -> anyhow::Result<Workspace> {
    sqlx::query(
        "UPDATE workspace
         SET archived_at = NULL, updated_at = CURRENT_TIMESTAMP
         WHERE id = ?1",
    )
    .bind(id)
    .execute(pool)
    .await?;
    fetch_one(pool, id).await
}

/// Set the active workspace. Returns the new state (active +
/// visible) so the caller can update AppState in lockstep.
pub async fn switch_active(
    pool: &SqlitePool,
    new_active_id: i64,
) -> anyhow::Result<State> {
    let target = fetch_optional(pool, new_active_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("workspace #{new_active_id} does not exist"))?;
    if target.is_archived() {
        anyhow::bail!("workspace '{}' is archived", target.name);
    }
    write_active_id(pool, new_active_id).await?;
    State::load(pool).await
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn validate_category(category: &str) -> anyhow::Result<()> {
    let valid = ["work", "personal", "health", "therapy", "legal", "finance", "other"];
    if !valid.contains(&category) {
        anyhow::bail!(
            "invalid category '{category}' — must be one of: {}",
            valid.join(", ")
        );
    }
    Ok(())
}

/// Lowercase, alphanumeric + hyphen, collapse whitespace runs, trim.
fn slugify(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_dash = true;
    for ch in s.chars() {
        let lower = ch.to_lowercase().next().unwrap_or(ch);
        if lower.is_ascii_alphanumeric() {
            out.push(lower);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}
