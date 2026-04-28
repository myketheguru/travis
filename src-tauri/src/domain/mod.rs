pub mod coach;
pub mod coach_hours;
pub mod invoice;
pub mod school;
pub mod signing_sheet;
pub mod task;

use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum DomainError {
    #[error("validation: {0}")]
    Validation(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl DomainError {
    pub fn invalid<S: Into<String>>(s: S) -> Self {
        DomainError::Validation(s.into())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub coaches: i64,
    pub schools: i64,
    pub coach_hours: i64,
    pub signing_sheets: i64,
    pub invoices: i64,
    pub tasks_open: i64,
    pub tasks_total: i64,
}

pub async fn stats(pool: &sqlx::SqlitePool) -> Result<Stats, sqlx::Error> {
    // Coaches/schools include both explicit CRUD rows AND mentions captured from
    // journal extraction (entity_index). UNION dedupes on the lowercase name so
    // a coach added both ways is only counted once.
    let (coaches,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM (
            SELECT normalized_name AS n FROM entity_index WHERE kind = 'coach'
            UNION
            SELECT LOWER(TRIM(name)) AS n FROM coach
        )",
    )
    .fetch_one(pool)
    .await?;
    let (schools,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM (
            SELECT normalized_name AS n FROM entity_index WHERE kind = 'school'
            UNION
            SELECT LOWER(TRIM(name)) AS n FROM school
        )",
    )
    .fetch_one(pool)
    .await?;
    let (coach_hours,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM coach_hours").fetch_one(pool).await?;
    let (signing_sheets,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM signing_sheet").fetch_one(pool).await?;
    let (invoices,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM invoice").fetch_one(pool).await?;
    let (tasks_open,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM task WHERE status = 'open'").fetch_one(pool).await?;
    let (tasks_total,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM task").fetch_one(pool).await?;
    Ok(Stats { coaches, schools, coach_hours, signing_sheets, invoices, tasks_open, tasks_total })
}
