//! v0.20.0 — relationship-aware drill-down for the LTE pack.
//!
//! `school_detail(school_id)` returns the school row alongside every
//! direct relationship needed for the Manage > Schools detail panel:
//! engagements, recent coach_hours, invoices, and documents linked
//! to the school via document_link.
//!
//! Single query roundtrip vs the frontend doing N queries; UI stays
//! responsive even with several months of activity on a school.

use serde::Serialize;
use sqlx::SqlitePool;
use tauri::State;

use crate::documents::db::Document as DocumentRow;
use crate::AppState;

use super::domain::engagement::Engagement;
use super::domain::school;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct CoachHoursRow {
    pub id: i64,
    pub coach_id: i64,
    pub coach_name: Option<String>,
    pub school_id: i64,
    pub session_date: String,
    pub hours: f64,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceRow {
    pub id: i64,
    pub number: String,
    pub recipient: String,
    pub period_start: String,
    pub period_end: String,
    pub amount_cents: i64,
    pub status: String,
    pub created_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchoolDetail {
    pub school: school::School,
    pub engagements: Vec<Engagement>,
    pub coach_hours: Vec<CoachHoursRow>,
    pub invoices: Vec<InvoiceRow>,
    pub documents: Vec<DocumentRow>,
}

#[tauri::command]
pub async fn lte_school_detail(
    state: State<'_, AppState>,
    school_id: i64,
) -> Result<SchoolDetail, String> {
    let pool = &state.db.pool;
    let workspace_id = state.workspace.read().await.active_id;

    let school = sqlx::query_as::<_, school::School>(
        "SELECT id, workspace_id, name, district, contact_name, contact_email,
                notes, created_at, updated_at
         FROM school WHERE id = ?1 AND workspace_id = ?2",
    )
    .bind(school_id)
    .bind(workspace_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| format!("school #{school_id} not found in this workspace"))?;

    let engagements = sqlx::query_as::<_, Engagement>(
        "SELECT id, workspace_id, name, school_id, stage, contract_ref, school_year,
                metrics_agreement_signed, metrics_signed_on, summary,
                period_start, period_end, ceiling_cents,
                created_at, updated_at
         FROM engagement
         WHERE school_id = ?1 AND workspace_id = ?2
         ORDER BY updated_at DESC",
    )
    .bind(school_id)
    .bind(workspace_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    let coach_hours = sqlx::query_as::<_, CoachHoursRow>(
        "SELECT h.id, h.coach_id, c.name AS coach_name, h.school_id,
                h.session_date, h.hours, h.description
         FROM coach_hours h
         LEFT JOIN coach c ON c.id = h.coach_id
         WHERE h.school_id = ?1 AND h.workspace_id = ?2
         ORDER BY h.session_date DESC LIMIT 100",
    )
    .bind(school_id)
    .bind(workspace_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    let invoices = sqlx::query_as::<_, InvoiceRow>(
        "SELECT id, number, recipient, period_start, period_end,
                amount_cents, status, created_at
         FROM invoice
         WHERE school_id = ?1 AND workspace_id = ?2
         ORDER BY created_at DESC LIMIT 100",
    )
    .bind(school_id)
    .bind(workspace_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    // Documents linked to the school via document_link. Schools live
    // on the spine as kind='school'; find the spine entity id then
    // pull docs through document_link.
    let documents = if let Some((entity_id,)) = sqlx::query_as::<_, (i64,)>(
        "SELECT id FROM entity
         WHERE kind = 'school' AND workspace_id = ?1
           AND (pack_table_id = ?2 OR LOWER(display_name) = LOWER(?3))
         LIMIT 1",
    )
    .bind(workspace_id)
    .bind(school_id)
    .bind(&school.name)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?
    {
        crate::documents::db::list(
            pool,
            crate::documents::db::ListFilter {
                workspace_id: Some(workspace_id),
                entity_id: Some(entity_id),
                limit: Some(100),
                ..Default::default()
            },
        )
        .await
    } else {
        Vec::new()
    };

    Ok(SchoolDetail {
        school,
        engagements,
        coach_hours,
        invoices,
        documents,
    })
}

// ---------------------------------------------------------------------------
// Engagement drill-down — v0.20.0.
//
// One engagement = one contract at one school. The drill-down shows the
// engagement's typed terms + its school + every coach who's logged hours at
// that school during the engagement window + invoices drawn against this
// contract + linked docs.
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoachContribution {
    pub coach_id: i64,
    pub coach_name: String,
    pub hours_total: f64,
    pub sessions: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngagementDetail {
    pub engagement: Engagement,
    pub school: Option<school::School>,
    pub coach_contributions: Vec<CoachContribution>,
    pub coach_hours: Vec<CoachHoursRow>,
    pub invoices: Vec<InvoiceRow>,
    pub documents: Vec<DocumentRow>,
    /// Sibling engagements at the same school — context for "how many
    /// contracts has this school had with us".
    pub sibling_engagements_count: i64,
}

#[tauri::command]
pub async fn lte_engagement_detail(
    state: State<'_, AppState>,
    engagement_id: i64,
) -> Result<EngagementDetail, String> {
    let pool = &state.db.pool;
    let workspace_id = state.workspace.read().await.active_id;

    let engagement = sqlx::query_as::<_, Engagement>(
        "SELECT id, workspace_id, name, school_id, stage, contract_ref, school_year,
                metrics_agreement_signed, metrics_signed_on, summary,
                period_start, period_end, ceiling_cents,
                created_at, updated_at
         FROM engagement WHERE id = ?1 AND workspace_id = ?2",
    )
    .bind(engagement_id)
    .bind(workspace_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| format!("engagement #{engagement_id} not found"))?;

    let school = if let Some(sid) = engagement.school_id {
        sqlx::query_as::<_, school::School>(
            "SELECT id, workspace_id, name, district, contact_name, contact_email,
                    notes, created_at, updated_at
             FROM school WHERE id = ?1",
        )
        .bind(sid)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?
    } else {
        None
    };

    // Coach hours filtered by the engagement's school + activity window
    // (when set). Without engagement_id on coach_hours we can't bind
    // hours to a specific engagement directly; the school+window filter
    // is the most accurate proxy.
    let (coach_contributions, coach_hours) = if let Some(school_id) = engagement.school_id {
        let contributions = sqlx::query_as::<_, (i64, String, f64, i64)>(
            "SELECT h.coach_id, c.name, SUM(h.hours), COUNT(*)
             FROM coach_hours h
             JOIN coach c ON c.id = h.coach_id
             WHERE h.school_id = ?1
               AND h.workspace_id = ?2
               AND (?3 IS NULL OR h.session_date >= ?3)
               AND (?4 IS NULL OR h.session_date <= ?4)
             GROUP BY h.coach_id, c.name
             ORDER BY SUM(h.hours) DESC",
        )
        .bind(school_id)
        .bind(workspace_id)
        .bind(engagement.period_start.as_deref())
        .bind(engagement.period_end.as_deref())
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|(coach_id, coach_name, hours_total, sessions)| CoachContribution {
            coach_id,
            coach_name,
            hours_total,
            sessions,
        })
        .collect();

        let hours = sqlx::query_as::<_, CoachHoursRow>(
            "SELECT h.id, h.coach_id, c.name AS coach_name, h.school_id,
                    h.session_date, h.hours, h.description
             FROM coach_hours h
             LEFT JOIN coach c ON c.id = h.coach_id
             WHERE h.school_id = ?1
               AND h.workspace_id = ?2
               AND (?3 IS NULL OR h.session_date >= ?3)
               AND (?4 IS NULL OR h.session_date <= ?4)
             ORDER BY h.session_date DESC LIMIT 100",
        )
        .bind(school_id)
        .bind(workspace_id)
        .bind(engagement.period_start.as_deref())
        .bind(engagement.period_end.as_deref())
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;

        (contributions, hours)
    } else {
        (Vec::new(), Vec::new())
    };

    // Invoices that draw against this contract — best matched by
    // contract_ref when both sides have it, else by school + period.
    let invoices = sqlx::query_as::<_, InvoiceRow>(
        "SELECT id, number, recipient, period_start, period_end,
                amount_cents, status, created_at
         FROM invoice
         WHERE workspace_id = ?1
           AND (
             (?2 IS NOT NULL AND notes LIKE '%' || ?2 || '%')
             OR (?3 IS NOT NULL AND school_id = ?3)
           )
         ORDER BY created_at DESC LIMIT 100",
    )
    .bind(workspace_id)
    .bind(engagement.contract_ref.as_deref())
    .bind(engagement.school_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    // Linked docs via spine entity for this engagement.
    let documents = if let Some((entity_id,)) = sqlx::query_as::<_, (i64,)>(
        "SELECT id FROM entity
         WHERE kind = 'engagement' AND workspace_id = ?1
           AND (pack_table_id = ?2 OR LOWER(display_name) = LOWER(?3))
         LIMIT 1",
    )
    .bind(workspace_id)
    .bind(engagement_id)
    .bind(&engagement.name)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?
    {
        crate::documents::db::list(
            pool,
            crate::documents::db::ListFilter {
                workspace_id: Some(workspace_id),
                entity_id: Some(entity_id),
                limit: Some(100),
                ..Default::default()
            },
        )
        .await
    } else {
        Vec::new()
    };

    let sibling_engagements_count: i64 = if let Some(sid) = engagement.school_id {
        sqlx::query_as::<_, (i64,)>(
            "SELECT COUNT(*) FROM engagement
             WHERE school_id = ?1 AND workspace_id = ?2 AND id != ?3",
        )
        .bind(sid)
        .bind(workspace_id)
        .bind(engagement_id)
        .fetch_one(pool)
        .await
        .map(|(c,)| c)
        .unwrap_or(0)
    } else {
        0
    };

    Ok(EngagementDetail {
        engagement,
        school,
        coach_contributions,
        coach_hours,
        invoices,
        documents,
        sibling_engagements_count,
    })
}

// ---------------------------------------------------------------------------
// Coach drill-down — v0.20.0.
//
// Coaches link to schools + engagements indirectly via coach_hours.school_id.
// We aggregate those to show "schools supported, contracts touched, total
// hours."
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoachSchoolStint {
    pub school_id: i64,
    pub school_name: String,
    pub hours_total: f64,
    pub sessions: i64,
    pub first_session_date: Option<String>,
    pub last_session_date: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoachDetail {
    pub coach: super::domain::coach::Coach,
    pub schools_supported_count: i64,
    pub engagements_count: i64,
    pub total_hours: f64,
    pub sessions_count: i64,
    pub schools: Vec<CoachSchoolStint>,
    pub engagements: Vec<Engagement>,
    pub recent_hours: Vec<CoachHoursRow>,
}

#[tauri::command]
pub async fn lte_coach_detail(
    state: State<'_, AppState>,
    coach_id: i64,
) -> Result<CoachDetail, String> {
    let pool = &state.db.pool;
    let workspace_id = state.workspace.read().await.active_id;

    let coach = super::domain::coach::find_by_name_or_id(pool, workspace_id, coach_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("coach #{coach_id} not found"))?;

    let schools = sqlx::query_as::<_, (i64, String, f64, i64, Option<String>, Option<String>)>(
        "SELECT s.id, s.name, SUM(h.hours), COUNT(*),
                MIN(h.session_date), MAX(h.session_date)
         FROM coach_hours h
         JOIN school s ON s.id = h.school_id
         WHERE h.coach_id = ?1 AND h.workspace_id = ?2
         GROUP BY s.id, s.name
         ORDER BY SUM(h.hours) DESC",
    )
    .bind(coach.id)
    .bind(workspace_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?
    .into_iter()
    .map(
        |(school_id, school_name, hours_total, sessions, first, last)| CoachSchoolStint {
            school_id,
            school_name,
            hours_total,
            sessions,
            first_session_date: first,
            last_session_date: last,
        },
    )
    .collect::<Vec<_>>();

    let school_ids: Vec<i64> = schools.iter().map(|s| s.school_id).collect();
    let engagements: Vec<Engagement> = if school_ids.is_empty() {
        Vec::new()
    } else {
        let placeholders: String = (2..2 + school_ids.len())
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "SELECT id, workspace_id, name, school_id, stage, contract_ref, school_year,
                    metrics_agreement_signed, metrics_signed_on, summary,
                    period_start, period_end, ceiling_cents,
                    created_at, updated_at
             FROM engagement
             WHERE workspace_id = ?1 AND school_id IN ({placeholders})
             ORDER BY updated_at DESC"
        );
        let mut q = sqlx::query_as::<_, Engagement>(&sql).bind(workspace_id);
        for id in &school_ids {
            q = q.bind(id);
        }
        q.fetch_all(pool)
            .await
            .map_err(|e| e.to_string())?
    };

    let recent_hours = sqlx::query_as::<_, CoachHoursRow>(
        "SELECT h.id, h.coach_id, c.name AS coach_name, h.school_id,
                h.session_date, h.hours, h.description
         FROM coach_hours h
         LEFT JOIN coach c ON c.id = h.coach_id
         WHERE h.coach_id = ?1 AND h.workspace_id = ?2
         ORDER BY h.session_date DESC LIMIT 100",
    )
    .bind(coach.id)
    .bind(workspace_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    let (total_hours, sessions_count): (f64, i64) = sqlx::query_as(
        "SELECT IFNULL(SUM(hours),0), COUNT(*)
         FROM coach_hours
         WHERE coach_id = ?1 AND workspace_id = ?2",
    )
    .bind(coach.id)
    .bind(workspace_id)
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(CoachDetail {
        coach,
        schools_supported_count: schools.len() as i64,
        engagements_count: engagements.len() as i64,
        total_hours,
        sessions_count,
        schools,
        engagements,
        recent_hours,
    })
}
