use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use super::DomainError;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Tutor {
    pub id: i64,
    pub name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub rate_cents: Option<i64>,
    pub subjects: Option<String>,
    pub notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TutorInput {
    pub id: Option<i64>,
    pub name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub rate_cents: Option<i64>,
    pub subjects: Option<String>,
    pub notes: Option<String>,
}

pub async fn list(pool: &SqlitePool) -> Result<Vec<Tutor>, DomainError> {
    let rows = sqlx::query_as::<_, Tutor>(
        "SELECT id, name, email, phone, rate_cents, subjects, notes, created_at, updated_at
         FROM tutor ORDER BY name COLLATE NOCASE",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn upsert(pool: &SqlitePool, input: TutorInput) -> Result<Tutor, DomainError> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(DomainError::invalid("name is required"));
    }

    let id = match input.id {
        Some(id) => {
            sqlx::query(
                "UPDATE tutor SET name=?1, email=?2, phone=?3, rate_cents=?4, subjects=?5,
                     notes=?6, updated_at=CURRENT_TIMESTAMP WHERE id=?7",
            )
            .bind(&name)
            .bind(&input.email)
            .bind(&input.phone)
            .bind(input.rate_cents)
            .bind(&input.subjects)
            .bind(&input.notes)
            .bind(id)
            .execute(pool)
            .await?;
            id
        }
        None => sqlx::query(
            "INSERT INTO tutor (name, email, phone, rate_cents, subjects, notes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind(&name)
        .bind(&input.email)
        .bind(&input.phone)
        .bind(input.rate_cents)
        .bind(&input.subjects)
        .bind(&input.notes)
        .execute(pool)
        .await?
        .last_insert_rowid(),
    };

    let row = sqlx::query_as::<_, Tutor>(
        "SELECT id, name, email, phone, rate_cents, subjects, notes, created_at, updated_at
         FROM tutor WHERE id=?1",
    )
    .bind(id)
    .fetch_one(pool)
    .await?;

    // Spine sync — register the tutor as an entity for cross-pack retrieval.
    if let Err(e) = crate::spine::entity::upsert(
        pool,
        crate::spine::entity::UpsertParams {
            kind: "tutor",
            display_name: &row.name,
            pack_slug: Some("tutoring"),
            attributes_json: None,
        },
    )
    .await
    {
        tracing::warn!("spine entity sync (tutor upsert): {e}");
    }

    Ok(row)
}

pub async fn delete(pool: &SqlitePool, id: i64) -> Result<(), DomainError> {
    sqlx::query("DELETE FROM tutor WHERE id=?1").bind(id).execute(pool).await?;
    Ok(())
}
