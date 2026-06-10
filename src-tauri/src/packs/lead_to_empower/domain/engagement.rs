//! v0.19.2 — minimal Rust domain module for the `engagement` table.
//!
//! The table itself was authored as auto-CRUD-only (served by
//! [`crate::packs_cmd`]); for the rest of LTE that's enough. This
//! module exists solely so the journal agent loop can silently
//! `ensure()` an engagement row whenever the LLM extraction names
//! one — matching the pattern shipped for `school` and `coach`.
//!
//! No CRUD here; the user keeps editing via the auto-CRUD UI.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use super::DomainError;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Engagement {
    pub id: i64,
    pub workspace_id: i64,
    pub name: String,
    pub school_id: Option<i64>,
    pub stage: String,
    pub contract_ref: Option<String>,
    pub school_year: Option<String>,
    pub metrics_agreement_signed: i64,
    pub metrics_signed_on: Option<String>,
    pub summary: Option<String>,
    /// v0.20.0 — activity window from the PO/WO doc.
    pub period_start: Option<String>,
    pub period_end: Option<String>,
    /// v0.20.0 — PO ceiling in cents.
    pub ceiling_cents: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
}

/// Case-insensitive name lookup, scoped to a single workspace.
pub async fn find_by_name(
    pool: &SqlitePool,
    workspace_id: i64,
    name: &str,
) -> Result<Option<Engagement>, DomainError> {
    let n = name.trim();
    if n.is_empty() {
        return Ok(None);
    }
    let row = sqlx::query_as::<_, Engagement>(
        "SELECT id, workspace_id, name, school_id, stage, contract_ref, school_year,
                metrics_agreement_signed, metrics_signed_on, summary,
                period_start, period_end, ceiling_cents,
                created_at, updated_at
         FROM engagement
         WHERE workspace_id = ?1 AND LOWER(name) = LOWER(?2)
         LIMIT 1",
    )
    .bind(workspace_id)
    .bind(n)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Silent auto-create on mention. Stage defaults to 'assessment' —
/// the 3 A's' starting bucket. The row is flagged in `summary` so
/// the user can see in the Manage tab that this row came from chat
/// not from a manual create.
pub async fn ensure(
    pool: &SqlitePool,
    workspace_id: i64,
    name: &str,
    school_id_hint: Option<i64>,
) -> Result<Engagement, DomainError> {
    if let Some(existing) = find_by_name(pool, workspace_id, name).await? {
        return Ok(existing);
    }
    let id = sqlx::query(
        "INSERT INTO engagement (workspace_id, name, school_id, stage, summary)
         VALUES (?1, ?2, ?3, 'assessment', 'Auto-created from chat mention.')",
    )
    .bind(workspace_id)
    .bind(name.trim())
    .bind(school_id_hint)
    .execute(pool)
    .await?
    .last_insert_rowid();

    let row = sqlx::query_as::<_, Engagement>(
        "SELECT id, workspace_id, name, school_id, stage, contract_ref, school_year,
                metrics_agreement_signed, metrics_signed_on, summary,
                period_start, period_end, ceiling_cents,
                created_at, updated_at
         FROM engagement WHERE id=?1",
    )
    .bind(id)
    .fetch_one(pool)
    .await?;

    // Spine sync — mirrors the pattern in school::upsert / coach::upsert.
    if let Err(e) = crate::spine::entity::upsert(
        pool,
        crate::spine::entity::UpsertParams {
            kind: "engagement",
            display_name: &row.name,
            pack_slug: Some("lead-to-empower"),
            attributes_json: None,
            workspace_id: row.workspace_id,
            pack_table_id: Some(row.id),
        },
    )
    .await
    {
        tracing::warn!("spine entity sync (engagement ensure): {e}");
    }

    Ok(row)
}
