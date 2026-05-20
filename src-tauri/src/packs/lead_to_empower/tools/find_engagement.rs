//! `lte_find_engagement` — chat-first engagement resolution.
//!
//! Read-only. Returns ranked engagements matched by school, contract,
//! school year, or name. The LLM uses this BEFORE proposing
//! `create_engagement`. Same shape as `lte_find_contract`.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::Manager;

use crate::llm::ToolDef;
use crate::tools::{Tool, ToolContext};
use crate::AppState;

pub struct FindEngagementTool;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    #[serde(default)]
    school: Option<String>,
    #[serde(default)]
    contract: Option<String>,
    #[serde(default)]
    school_year: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    stage: Option<String>,
}

#[async_trait]
impl Tool for FindEngagementTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "lte_find_engagement".into(),
            description: "Find an L2E engagement by school name, contract \
                ref, school year, or engagement name (any combination). \
                Returns up to 5 ranked matches with school, contract, \
                stage, school_year, last activity, and whether there's \
                an active draft invoice. Use BEFORE proposing \
                create_engagement so you don't double-create. Ranking: \
                recency of activity DESC, with active stage prioritised."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "school": { "type": "string", "description": "School name fragment." },
                    "contract": { "type": "string", "description": "Contract ref or name fragment." },
                    "schoolYear": { "type": "string", "description": "e.g. '2026-2027'." },
                    "name": { "type": "string", "description": "Engagement name fragment." },
                    "stage": { "type": "string", "enum": ["assessment","action_planning","accountable","closed"] }
                }
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: Input = serde_json::from_value(input)?;
        let state = ctx.app.state::<AppState>();
        let workspace_id = state.workspace.read().await.active_id;

        let school_like = p
            .school
            .as_deref()
            .map(|s| format!("%{}%", s.to_lowercase()));
        let contract_like = p
            .contract
            .as_deref()
            .map(|s| format!("%{}%", s.to_lowercase()));
        let name_like = p
            .name
            .as_deref()
            .map(|s| format!("%{}%", s.to_lowercase()));

        #[derive(sqlx::FromRow)]
        struct Row {
            id: i64,
            name: String,
            stage: String,
            school_year: Option<String>,
            school_id: Option<i64>,
            school_name: Option<String>,
            contract_id: Option<i64>,
            contract_ref: Option<String>,
            updated_at: String,
            draft_invoice_count: i64,
        }

        let rows: Vec<Row> = sqlx::query_as(
            "SELECT e.id, e.name, e.stage, e.school_year, e.school_id,
                    s.name AS school_name,
                    e.contract_id, c.ref AS contract_ref,
                    e.updated_at,
                    (SELECT COUNT(*) FROM invoice i WHERE i.engagement_id = e.id AND i.status = 'draft') AS draft_invoice_count
             FROM engagement e
             LEFT JOIN school s ON s.id = e.school_id
             LEFT JOIN contract c ON c.id = e.contract_id
             WHERE e.workspace_id = ?1
               AND (?2 IS NULL OR LOWER(s.name) LIKE ?2)
               AND (?3 IS NULL OR LOWER(c.ref) LIKE ?3 OR LOWER(COALESCE(c.name,'')) LIKE ?3)
               AND (?4 IS NULL OR e.school_year = ?4)
               AND (?5 IS NULL OR LOWER(e.name) LIKE ?5)
               AND (?6 IS NULL OR e.stage = ?6)
             ORDER BY
               CASE e.stage
                 WHEN 'accountable' THEN 0
                 WHEN 'action_planning' THEN 1
                 WHEN 'assessment' THEN 2
                 ELSE 3
               END,
               e.updated_at DESC
             LIMIT 5",
        )
        .bind(workspace_id)
        .bind(school_like.as_deref())
        .bind(contract_like.as_deref())
        .bind(p.school_year.as_deref())
        .bind(name_like.as_deref())
        .bind(p.stage.as_deref())
        .fetch_all(&ctx.db.pool)
        .await?;

        let candidates: Vec<Value> = rows
            .iter()
            .map(|r| {
                json!({
                    "id": r.id,
                    "name": r.name,
                    "stage": r.stage,
                    "schoolYear": r.school_year,
                    "schoolId": r.school_id,
                    "schoolName": r.school_name,
                    "contractId": r.contract_id,
                    "contractRef": r.contract_ref,
                    "updatedAt": r.updated_at,
                    "hasDraftInvoice": r.draft_invoice_count > 0,
                })
            })
            .collect();

        let result = if rows.is_empty() {
            "no_matches"
        } else if rows.len() == 1 {
            "single"
        } else {
            "multiple"
        };

        Ok(json!({
            "result": result,
            "candidates": candidates,
            "rationale": "ranked by stage priority (accountable first), then recency"
        })
        .to_string())
    }
}
