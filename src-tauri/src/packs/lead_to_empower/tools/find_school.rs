//! `lte_find_or_create_school` — chat-first school resolution.
//!
//! Searches the L2E `school` table for a case-insensitive name match. If
//! found, returns the existing row's id + metadata (with a recency-ranked
//! short list when multiple match). If not found, creates a new row
//! silently — schools are observational, not relationship-committing, so
//! per the `feedback_track_everything` rule we record without asking.
//!
//! Breaks the "tools never write" convention deliberately: silent creates
//! belong here, not in the action mechanism (which surfaces confirmation
//! cards). Contract / engagement creates that DO commit to a relationship
//! still go through actions (see `actions::CreateContract` and
//! `CreateEngagement`).

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};

use tauri::Manager;

use crate::llm::ToolDef;
use crate::tools::{Tool, ToolContext};
use crate::AppState;

pub struct FindOrCreateSchoolTool;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    /// School name as Taylor referenced it. Required.
    name: String,
    /// Optional context for enrichment on a silent create.
    #[serde(default)]
    district: Option<String>,
    #[serde(default)]
    address: Option<String>,
    #[serde(default)]
    contact_name: Option<String>,
    #[serde(default)]
    contact_email: Option<String>,
    /// If `true`, never create — only search. Useful when you want to ask
    /// before adding. Defaults to `false` (silent create on miss).
    #[serde(default)]
    search_only: bool,
}

#[async_trait]
impl Tool for FindOrCreateSchoolTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "lte_find_or_create_school".into(),
            description: "Find an L2E school by name (case-insensitive, \
                substring match). Returns up to 5 ranked matches with id, \
                district, active engagement count, last activity. If the \
                top match is exact, use it without asking. If two or three \
                are close, list them and ask. If none match and the user \
                clearly intends a new school, this tool creates one \
                silently and returns the new id — schools are observational \
                and don't need confirmation. Pass searchOnly=true to skip \
                the silent-create behaviour."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "School name (full or partial). Required." },
                    "district": { "type": "string", "description": "Optional district number to enrich a created row." },
                    "address": { "type": "string", "description": "Optional street address." },
                    "contactName": { "type": "string" },
                    "contactEmail": { "type": "string" },
                    "searchOnly": { "type": "boolean", "description": "When true, never create — only return matches. Defaults to false." }
                },
                "required": ["name"]
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: Input = serde_json::from_value(input)?;
        let name = p.name.trim();
        if name.is_empty() {
            anyhow::bail!("name is required");
        }

        let state = ctx.app.state::<AppState>();
        let workspace_id = state.workspace.read().await.active_id;
        let like = format!("%{}%", name.to_lowercase());

        #[derive(sqlx::FromRow)]
        struct SchoolMatch {
            id: i64,
            name: String,
            district: Option<String>,
            address: Option<String>,
            engagements: i64,
            last_activity: Option<String>,
        }

        // Ranked: case-insensitive match, then by recent engagement
        // activity DESC, then by name proximity (exact first).
        let matches: Vec<SchoolMatch> = sqlx::query_as(
            "SELECT s.id AS id, s.name AS name, s.district AS district, s.address AS address,
                    (SELECT COUNT(*) FROM engagement e WHERE e.school_id = s.id) AS engagements,
                    (SELECT MAX(e.updated_at) FROM engagement e WHERE e.school_id = s.id) AS last_activity
             FROM school s
             WHERE s.workspace_id = ?1
               AND LOWER(s.name) LIKE ?2
             ORDER BY
               CASE WHEN LOWER(s.name) = LOWER(?3) THEN 0 ELSE 1 END,
               last_activity DESC NULLS LAST,
               engagements DESC,
               s.name ASC
             LIMIT 5",
        )
        .bind(workspace_id)
        .bind(&like)
        .bind(name)
        .fetch_all(&ctx.db.pool)
        .await?;

        if let Some(top) = matches.first() {
            if top.name.eq_ignore_ascii_case(name) {
                // Unambiguous hit — return the id and any enrichment fields
                // Taylor may have supplied (the LLM can decide to update_school
                // later if there's new info).
                return Ok(json!({
                    "result": "found",
                    "id": top.id,
                    "name": top.name,
                    "district": top.district,
                    "address": top.address,
                    "engagements": top.engagements,
                    "lastActivity": top.last_activity,
                    "rationale": "exact name match"
                })
                .to_string());
            }
        }

        // Multiple ambiguous matches → return them ranked, LLM asks.
        if matches.len() > 1 {
            let candidates: Vec<Value> = matches
                .iter()
                .map(|m| {
                    json!({
                        "id": m.id,
                        "name": m.name,
                        "district": m.district,
                        "engagements": m.engagements,
                        "lastActivity": m.last_activity
                    })
                })
                .collect();
            return Ok(json!({
                "result": "ambiguous",
                "candidates": candidates,
                "rationale": "multiple matches; ask Taylor which one she means"
            })
            .to_string());
        }

        // Single fuzzy match (substring but not exact). Surface it as
        // ambiguous so the LLM can verify before assuming.
        if let Some(only) = matches.first() {
            return Ok(json!({
                "result": "ambiguous",
                "candidates": [{
                    "id": only.id,
                    "name": only.name,
                    "district": only.district,
                    "engagements": only.engagements,
                    "lastActivity": only.last_activity
                }],
                "rationale": "one fuzzy match; confirm before using"
            })
            .to_string());
        }

        // No match — silent create unless caller said searchOnly.
        if p.search_only {
            return Ok(json!({
                "result": "not_found",
                "rationale": "no match; searchOnly=true so no row created"
            })
            .to_string());
        }

        let id: i64 = sqlx::query_scalar(
            "INSERT INTO school (workspace_id, name, district, address, contact_name, contact_email)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6) RETURNING id",
        )
        .bind(workspace_id)
        .bind(name)
        .bind(p.district.as_deref())
        .bind(p.address.as_deref())
        .bind(p.contact_name.as_deref())
        .bind(p.contact_email.as_deref())
        .fetch_one(&ctx.db.pool)
        .await?;

        Ok(json!({
            "result": "created",
            "id": id,
            "name": name,
            "rationale": "silent create — schools are observational"
        })
        .to_string())
    }
}
