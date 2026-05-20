//! `lte_summarize_context` — read-only one-paragraph context snapshot.
//!
//! When Taylor names something ambiguously ("the math contract", "PS498",
//! "the engagement from last week"), the LLM calls this to ground the
//! reply in current state without paraphrasing the schema. Returns a
//! human-shaped paragraph the LLM can quote or restate.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::Manager;

use crate::llm::ToolDef;
use crate::tools::{Tool, ToolContext};
use crate::AppState;

pub struct SummarizeContextTool;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    /// Entity kind: "school" | "contract" | "engagement".
    kind: String,
    /// Numeric id (preferred) or string identifier (school name, contract ref).
    id: Option<i64>,
    #[serde(default)]
    name_or_ref: Option<String>,
}

#[async_trait]
impl Tool for SummarizeContextTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "lte_summarize_context".into(),
            description: "Get a one-paragraph context summary for a school, \
                contract, or engagement. Useful when the user references \
                something ambiguously and you want to ground your reply in \
                what Travis actually knows about it. Pass either `id` \
                (numeric, preferred) or `nameOrRef` (string)."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "kind": { "type": "string", "enum": ["school","contract","engagement"] },
                    "id": { "type": "integer" },
                    "nameOrRef": { "type": "string" }
                },
                "required": ["kind"]
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: Input = serde_json::from_value(input)?;
        let state = ctx.app.state::<AppState>();
        let workspace_id = state.workspace.read().await.active_id;

        let summary = match p.kind.as_str() {
            "school" => summarize_school(&ctx.db.pool, workspace_id, p.id, p.name_or_ref).await?,
            "contract" => summarize_contract(&ctx.db.pool, workspace_id, p.id, p.name_or_ref).await?,
            "engagement" => summarize_engagement(&ctx.db.pool, workspace_id, p.id, p.name_or_ref).await?,
            other => anyhow::bail!("unknown kind: {other}"),
        };

        Ok(json!({ "summary": summary }).to_string())
    }
}

async fn summarize_school(
    pool: &sqlx::SqlitePool,
    workspace_id: i64,
    id: Option<i64>,
    name_or_ref: Option<String>,
) -> anyhow::Result<String> {
    #[derive(sqlx::FromRow)]
    struct Row {
        id: i64,
        name: String,
        district: Option<String>,
        address: Option<String>,
        contact_name: Option<String>,
        engagements: i64,
        active_engagements: i64,
        last_activity: Option<String>,
    }
    let q = "SELECT s.id, s.name, s.district, s.address, s.contact_name,
                    (SELECT COUNT(*) FROM engagement e WHERE e.school_id = s.id) AS engagements,
                    (SELECT COUNT(*) FROM engagement e WHERE e.school_id = s.id AND e.stage IN ('accountable','action_planning')) AS active_engagements,
                    (SELECT MAX(updated_at) FROM engagement e WHERE e.school_id = s.id) AS last_activity
             FROM school s
             WHERE s.workspace_id = ?1
               AND (?2 IS NULL OR s.id = ?2)
               AND (?3 IS NULL OR LOWER(s.name) LIKE ?3)
             ORDER BY engagements DESC LIMIT 1";
    let like = name_or_ref.as_deref().map(|s| format!("%{}%", s.to_lowercase()));
    let row: Option<Row> = sqlx::query_as(q)
        .bind(workspace_id)
        .bind(id)
        .bind(like.as_deref())
        .fetch_optional(pool)
        .await?;
    let Some(r) = row else {
        return Ok("No matching school found.".into());
    };
    let mut s = format!("{} (school #{})", r.name, r.id);
    if let Some(d) = r.district.as_deref() {
        s.push_str(&format!(", district {d}"));
    }
    if let Some(a) = r.address.as_deref() {
        s.push_str(&format!(", at {a}"));
    }
    if let Some(c) = r.contact_name.as_deref() {
        s.push_str(&format!(". Contact: {c}"));
    }
    s.push_str(&format!(
        ". {} engagement{} on file ({} active)",
        r.engagements,
        if r.engagements == 1 { "" } else { "s" },
        r.active_engagements
    ));
    if let Some(la) = r.last_activity.as_deref() {
        s.push_str(&format!("; last activity {la}"));
    }
    s.push('.');
    Ok(s)
}

async fn summarize_contract(
    pool: &sqlx::SqlitePool,
    workspace_id: i64,
    id: Option<i64>,
    name_or_ref: Option<String>,
) -> anyhow::Result<String> {
    #[derive(sqlx::FromRow)]
    struct Row {
        id: i64,
        ref_: String,
        name: Option<String>,
        counterparty: Option<String>,
        parent_solicitation: Option<String>,
        status: String,
        term_start: Option<String>,
        term_end: Option<String>,
        ceiling_cents: i64,
        invoiced_cents: i64,
        engagements: i64,
    }
    let like = name_or_ref.as_deref().map(|s| format!("%{}%", s.to_lowercase()));
    let row: Option<Row> = sqlx::query_as(
        "SELECT c.id, c.ref AS ref_, c.name, c.counterparty, c.parent_solicitation,
                c.status, c.term_start, c.term_end, c.ceiling_cents,
                COALESCE((SELECT SUM(i.amount_cents) FROM invoice i
                            JOIN engagement e ON e.id = i.engagement_id
                            WHERE e.contract_id = c.id AND i.status != 'void'), 0) AS invoiced_cents,
                (SELECT COUNT(*) FROM engagement e WHERE e.contract_id = c.id) AS engagements
         FROM contract c
         WHERE c.workspace_id = ?1
           AND (?2 IS NULL OR c.id = ?2)
           AND (?3 IS NULL OR LOWER(c.ref) LIKE ?3 OR LOWER(COALESCE(c.counterparty,'')) LIKE ?3)
         LIMIT 1",
    )
    .bind(workspace_id)
    .bind(id)
    .bind(like.as_deref())
    .fetch_optional(pool)
    .await?;
    let Some(r) = row else {
        return Ok("No matching contract found.".into());
    };
    let mut s = format!("Contract {} (#{}, {})", r.ref_, r.id, r.status);
    if let Some(n) = r.name.as_deref() {
        if !n.is_empty() && n != r.ref_ {
            s.push_str(&format!(" — \"{n}\""));
        }
    }
    if let Some(cp) = r.counterparty.as_deref() {
        s.push_str(&format!(", with {cp}"));
    }
    if let Some(ps) = r.parent_solicitation.as_deref() {
        s.push_str(&format!(" (from {ps})"));
    }
    match (r.term_start.as_deref(), r.term_end.as_deref()) {
        (Some(a), Some(b)) => s.push_str(&format!(". Term {a}..{b}")),
        (None, Some(b)) => s.push_str(&format!(". Ends {b}")),
        _ => {}
    }
    if r.ceiling_cents > 0 {
        let pct = (r.invoiced_cents as f64 / r.ceiling_cents as f64 * 100.0).round() as i64;
        s.push_str(&format!(
            ". Ceiling ${:.2}, invoiced ${:.2} ({}% burn)",
            r.ceiling_cents as f64 / 100.0,
            r.invoiced_cents as f64 / 100.0,
            pct
        ));
    }
    s.push_str(&format!(
        ". {} engagement{} link{} this contract.",
        r.engagements,
        if r.engagements == 1 { "" } else { "s" },
        if r.engagements == 1 { "s" } else { "" }
    ));
    Ok(s)
}

async fn summarize_engagement(
    pool: &sqlx::SqlitePool,
    workspace_id: i64,
    id: Option<i64>,
    name_or_ref: Option<String>,
) -> anyhow::Result<String> {
    #[derive(sqlx::FromRow)]
    struct Row {
        id: i64,
        name: String,
        stage: String,
        school_year: Option<String>,
        school_name: Option<String>,
        contract_ref: Option<String>,
        scope_items: i64,
        hours_logged: f64,
        invoiced_cents: i64,
    }
    let like = name_or_ref.as_deref().map(|s| format!("%{}%", s.to_lowercase()));
    let row: Option<Row> = sqlx::query_as(
        "SELECT e.id, e.name, e.stage, e.school_year,
                s.name AS school_name, c.ref AS contract_ref,
                (SELECT COUNT(*) FROM engagement_module em WHERE em.engagement_id = e.id) AS scope_items,
                COALESCE((SELECT SUM(ch.hours) FROM coach_hours ch WHERE ch.engagement_id = e.id), 0) AS hours_logged,
                COALESCE((SELECT SUM(i.amount_cents) FROM invoice i WHERE i.engagement_id = e.id AND i.status != 'void'), 0) AS invoiced_cents
         FROM engagement e
         LEFT JOIN school s ON s.id = e.school_id
         LEFT JOIN contract c ON c.id = e.contract_id
         WHERE e.workspace_id = ?1
           AND (?2 IS NULL OR e.id = ?2)
           AND (?3 IS NULL OR LOWER(e.name) LIKE ?3)
         ORDER BY e.updated_at DESC LIMIT 1",
    )
    .bind(workspace_id)
    .bind(id)
    .bind(like.as_deref())
    .fetch_optional(pool)
    .await?;
    let Some(r) = row else {
        return Ok("No matching engagement found.".into());
    };
    let mut s = format!("{} (engagement #{}, stage {})", r.name, r.id, r.stage);
    if let Some(school) = r.school_name.as_deref() {
        s.push_str(&format!(" at {school}"));
    }
    if let Some(c) = r.contract_ref.as_deref() {
        s.push_str(&format!(", under contract {c}"));
    }
    if let Some(y) = r.school_year.as_deref() {
        s.push_str(&format!(" ({y})"));
    }
    s.push_str(&format!(
        ". {} scope item{}, {:.1} hours delivered",
        r.scope_items,
        if r.scope_items == 1 { "" } else { "s" },
        r.hours_logged
    ));
    if r.invoiced_cents > 0 {
        s.push_str(&format!(
            ", ${:.2} invoiced",
            r.invoiced_cents as f64 / 100.0
        ));
    }
    s.push('.');
    Ok(s)
}
