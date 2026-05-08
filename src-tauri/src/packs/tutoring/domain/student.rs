use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use super::DomainError;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Student {
    pub id: i64,
    pub name: String,
    pub grade: Option<String>,
    pub parent_name: Option<String>,
    pub parent_email: Option<String>,
    pub parent_phone: Option<String>,
    pub notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudentInput {
    pub id: Option<i64>,
    pub name: String,
    pub grade: Option<String>,
    pub parent_name: Option<String>,
    pub parent_email: Option<String>,
    pub parent_phone: Option<String>,
    pub notes: Option<String>,
}

pub async fn list(pool: &SqlitePool) -> Result<Vec<Student>, DomainError> {
    let rows = sqlx::query_as::<_, Student>(
        "SELECT id, name, grade, parent_name, parent_email, parent_phone, notes,
                created_at, updated_at
         FROM student ORDER BY name COLLATE NOCASE",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn upsert(pool: &SqlitePool, input: StudentInput) -> Result<Student, DomainError> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(DomainError::invalid("name is required"));
    }

    let id = match input.id {
        Some(id) => {
            sqlx::query(
                "UPDATE student SET name=?1, grade=?2, parent_name=?3, parent_email=?4,
                     parent_phone=?5, notes=?6, updated_at=CURRENT_TIMESTAMP WHERE id=?7",
            )
            .bind(&name)
            .bind(&input.grade)
            .bind(&input.parent_name)
            .bind(&input.parent_email)
            .bind(&input.parent_phone)
            .bind(&input.notes)
            .bind(id)
            .execute(pool)
            .await?;
            id
        }
        None => sqlx::query(
            "INSERT INTO student (name, grade, parent_name, parent_email, parent_phone, notes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind(&name)
        .bind(&input.grade)
        .bind(&input.parent_name)
        .bind(&input.parent_email)
        .bind(&input.parent_phone)
        .bind(&input.notes)
        .execute(pool)
        .await?
        .last_insert_rowid(),
    };

    let row = sqlx::query_as::<_, Student>(
        "SELECT id, name, grade, parent_name, parent_email, parent_phone, notes,
                created_at, updated_at
         FROM student WHERE id=?1",
    )
    .bind(id)
    .fetch_one(pool)
    .await?;

    // Spine sync.
    if let Err(e) = crate::spine::entity::upsert(
        pool,
        crate::spine::entity::UpsertParams {
            kind: "student",
            display_name: &row.name,
            pack_slug: Some("tutoring"),
            attributes_json: None,
        },
    )
    .await
    {
        tracing::warn!("spine entity sync (student upsert): {e}");
    }

    Ok(row)
}

pub async fn delete(pool: &SqlitePool, id: i64) -> Result<(), DomainError> {
    sqlx::query("DELETE FROM student WHERE id=?1").bind(id).execute(pool).await?;
    Ok(())
}
