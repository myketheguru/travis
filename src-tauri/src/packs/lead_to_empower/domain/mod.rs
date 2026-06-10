//! Domain types and CRUD for the Lead to Empower pack.
//!
//! Five typed tables: coach, school, coach_hours, signing_sheet,
//! invoice. The corresponding SQL tables are created by core's
//! `0003_domain.sql` migration (kept in core for migration-history
//! continuity); this module owns the Rust-level types and queries.
//!
//! The pack's [`super::PackHandle::register_actions`] also lives in
//! the sibling [`super::actions`] module.
//!
//! Backwards-compat note: `crate::domain` re-exports these submodules
//! when the `pack-lead-to-empower` feature is enabled, so existing
//! call sites (e.g. `crate::domain_cmd`, `crate::pdf`) keep working
//! unchanged. Step 8c will move `domain_cmd` and `pdf` into the pack
//! and remove the re-exports.

pub mod coach;
pub mod coach_hours;
pub mod engagement;
pub mod invoice;
pub mod school;
pub mod signing_sheet;

// `super::DomainError` references inside the moved modules resolve here.
pub use crate::domain::DomainError;

use serde::Serialize;

/// Snapshot of L2E + core counts for the manage-tab status header.
/// `tasks_open` and `tasks_total` are derived from core's `task`
/// table; the rest are L2E-typed counts joined with the spine
/// `entity` table for journal-mention dedupe.
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
    // journal extraction (the spine `entity` table). UNION dedupes on the
    // lowercase name so a coach added both ways is only counted once.
    let (coaches,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM (
            SELECT normalized_name AS n FROM entity WHERE kind = 'coach'
            UNION
            SELECT LOWER(TRIM(name)) AS n FROM coach
        )",
    )
    .fetch_one(pool)
    .await?;
    let (schools,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM (
            SELECT normalized_name AS n FROM entity WHERE kind = 'school'
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
