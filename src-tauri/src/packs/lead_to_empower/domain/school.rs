use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use super::DomainError;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct School {
    pub id: i64,
    pub name: String,
    pub district: Option<String>,
    pub contact_name: Option<String>,
    pub contact_email: Option<String>,
    pub notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchoolInput {
    pub id: Option<i64>,
    pub name: String,
    pub district: Option<String>,
    pub contact_name: Option<String>,
    pub contact_email: Option<String>,
    pub notes: Option<String>,
}

pub async fn list(pool: &SqlitePool) -> Result<Vec<School>, DomainError> {
    let rows = sqlx::query_as::<_, School>(
        "SELECT id, name, district, contact_name, contact_email, notes, created_at, updated_at
         FROM school ORDER BY name COLLATE NOCASE",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn upsert(pool: &SqlitePool, input: SchoolInput) -> Result<School, DomainError> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(DomainError::invalid("name is required"));
    }

    let id = match input.id {
        Some(id) => {
            sqlx::query(
                "UPDATE school SET name=?1, district=?2, contact_name=?3, contact_email=?4,
                     notes=?5, updated_at=CURRENT_TIMESTAMP WHERE id=?6",
            )
            .bind(&name)
            .bind(&input.district)
            .bind(&input.contact_name)
            .bind(&input.contact_email)
            .bind(&input.notes)
            .bind(id)
            .execute(pool)
            .await?;
            id
        }
        None => sqlx::query(
            "INSERT INTO school (name, district, contact_name, contact_email, notes)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind(&name)
        .bind(&input.district)
        .bind(&input.contact_name)
        .bind(&input.contact_email)
        .bind(&input.notes)
        .execute(pool)
        .await?
        .last_insert_rowid(),
    };

    let row = sqlx::query_as::<_, School>(
        "SELECT id, name, district, contact_name, contact_email, notes, created_at, updated_at
         FROM school WHERE id=?1",
    )
    .bind(id)
    .fetch_one(pool)
    .await?;

    // Spine sync — register the school as an entity so it shows up in
    // cross-pack retrieval. Best-effort; failures don't fail the upsert.
    if let Err(e) = crate::spine::entity::upsert(
        pool,
        crate::spine::entity::UpsertParams {
            kind: "school",
            display_name: &row.name,
            pack_slug: Some("lead-to-empower"),
            attributes_json: None,
        },
    )
    .await
    {
        tracing::warn!("spine entity sync (school upsert): {e}");
    }

    Ok(row)
}

pub async fn delete(pool: &SqlitePool, id: i64) -> Result<(), DomainError> {
    sqlx::query("DELETE FROM school WHERE id=?1").bind(id).execute(pool).await?;
    Ok(())
}
