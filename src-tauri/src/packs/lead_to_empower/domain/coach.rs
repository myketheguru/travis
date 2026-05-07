use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use super::DomainError;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Coach {
    pub id: i64,
    pub name: String,
    pub email: Option<String>,
    pub rate_cents: Option<i64>,
    pub notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoachInput {
    pub id: Option<i64>,
    pub name: String,
    pub email: Option<String>,
    pub rate_cents: Option<i64>,
    pub notes: Option<String>,
}

pub async fn list(pool: &SqlitePool) -> Result<Vec<Coach>, DomainError> {
    let rows = sqlx::query_as::<_, Coach>(
        "SELECT id, name, email, rate_cents, notes, created_at, updated_at
         FROM coach ORDER BY name COLLATE NOCASE",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn upsert(pool: &SqlitePool, input: CoachInput) -> Result<Coach, DomainError> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(DomainError::invalid("name is required"));
    }

    let id = match input.id {
        Some(id) => {
            sqlx::query(
                "UPDATE coach SET name=?1, email=?2, rate_cents=?3, notes=?4,
                     updated_at=CURRENT_TIMESTAMP WHERE id=?5",
            )
            .bind(&name)
            .bind(&input.email)
            .bind(input.rate_cents)
            .bind(&input.notes)
            .bind(id)
            .execute(pool)
            .await?;
            id
        }
        None => {
            sqlx::query(
                "INSERT INTO coach (name, email, rate_cents, notes) VALUES (?1, ?2, ?3, ?4)",
            )
            .bind(&name)
            .bind(&input.email)
            .bind(input.rate_cents)
            .bind(&input.notes)
            .execute(pool)
            .await?
            .last_insert_rowid()
        }
    };

    let row = sqlx::query_as::<_, Coach>(
        "SELECT id, name, email, rate_cents, notes, created_at, updated_at FROM coach WHERE id=?1",
    )
    .bind(id)
    .fetch_one(pool)
    .await?;

    // Spine sync — register the coach as an entity so it shows up in
    // cross-pack retrieval. Best-effort; failures don't fail the upsert.
    if let Err(e) = crate::spine::entity::upsert(
        pool,
        crate::spine::entity::UpsertParams {
            kind: "coach",
            display_name: &row.name,
            pack_slug: Some("lead-to-empower"),
            attributes_json: None,
        },
    )
    .await
    {
        tracing::warn!("spine entity sync (coach upsert): {e}");
    }

    Ok(row)
}

pub async fn delete(pool: &SqlitePool, id: i64) -> Result<(), DomainError> {
    sqlx::query("DELETE FROM coach WHERE id=?1").bind(id).execute(pool).await?;
    Ok(())
}
