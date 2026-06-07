//! pack_template DB ops.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct PackTemplate {
    pub id: i64,
    pub workspace_id: i64,
    pub pack_slug: String,
    pub kind: String,
    pub label: String,
    pub counterparty_hint: Option<String>,
    pub styling_json: String,
    pub generation_code: String,
    pub sample_document_id: Option<i64>,
    pub used_count: i64,
    pub last_used_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackTemplateInput {
    pub pack_slug: String,
    pub kind: String,
    pub label: String,
    pub counterparty_hint: Option<String>,
    pub styling_json: String,
    pub generation_code: String,
    pub sample_document_id: Option<i64>,
}

pub async fn save(
    pool: &SqlitePool,
    workspace_id: i64,
    input: PackTemplateInput,
) -> anyhow::Result<PackTemplate> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO pack_template
            (workspace_id, pack_slug, kind, label, counterparty_hint,
             styling_json, generation_code, sample_document_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT (workspace_id, pack_slug, kind, label) DO UPDATE SET
             counterparty_hint = excluded.counterparty_hint,
             styling_json = excluded.styling_json,
             generation_code = excluded.generation_code,
             sample_document_id = excluded.sample_document_id,
             updated_at = CURRENT_TIMESTAMP
         RETURNING id",
    )
    .bind(workspace_id)
    .bind(&input.pack_slug)
    .bind(&input.kind)
    .bind(&input.label)
    .bind(input.counterparty_hint.as_deref())
    .bind(&input.styling_json)
    .bind(&input.generation_code)
    .bind(input.sample_document_id)
    .fetch_one(pool)
    .await?;
    get_one(pool, id).await
}

pub async fn get_one(pool: &SqlitePool, id: i64) -> anyhow::Result<PackTemplate> {
    sqlx::query_as::<_, PackTemplate>(
        "SELECT id, workspace_id, pack_slug, kind, label, counterparty_hint,
                styling_json, generation_code, sample_document_id, used_count,
                last_used_at, created_at, updated_at
         FROM pack_template WHERE id = ?1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .map_err(Into::into)
}

pub async fn find(
    pool: &SqlitePool,
    workspace_id: i64,
    pack_slug: &str,
    kind: &str,
    counterparty_hint: Option<&str>,
) -> Vec<PackTemplate> {
    let hint_like = counterparty_hint
        .map(|s| format!("%{}%", s.to_lowercase()))
        .unwrap_or_else(|| "%".to_string());
    sqlx::query_as::<_, PackTemplate>(
        "SELECT id, workspace_id, pack_slug, kind, label, counterparty_hint,
                styling_json, generation_code, sample_document_id, used_count,
                last_used_at, created_at, updated_at
         FROM pack_template
         WHERE workspace_id = ?1 AND pack_slug = ?2 AND kind = ?3
           AND (?4 = '%' OR LOWER(COALESCE(counterparty_hint, '')) LIKE ?4)
         ORDER BY
            CASE WHEN counterparty_hint IS NOT NULL THEN 0 ELSE 1 END,
            last_used_at DESC NULLS LAST,
            updated_at DESC
         LIMIT 5",
    )
    .bind(workspace_id)
    .bind(pack_slug)
    .bind(kind)
    .bind(&hint_like)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
}

pub async fn mark_used(pool: &SqlitePool, id: i64) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE pack_template
         SET used_count = used_count + 1,
             last_used_at = CURRENT_TIMESTAMP
         WHERE id = ?1",
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete(pool: &SqlitePool, id: i64) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM pack_template WHERE id = ?1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}
