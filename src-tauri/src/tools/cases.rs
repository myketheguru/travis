//! Case-management LLM tools — open / note / close / find.
//!
//! Cases survive across conversations and let multi-day work
//! ("PS 89 invoice #3 reconciliation") stay coherent. Travis can
//! resume by name and inject the case's rolling summary into the
//! prompt.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::Manager;

use crate::cases::db::{self as cases_db, CaseInput};
use crate::llm::ToolDef;
use crate::tools::{Tool, ToolContext};
use crate::AppState;

pub struct OpenCaseTool;
pub struct NoteCaseTool;
pub struct CloseCaseTool;
pub struct FindCaseTool;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenInput {
    name: String,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    parent_case_id: Option<i64>,
}

#[async_trait]
impl Tool for OpenCaseTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "open_case".into(),
            description: "Open or resume a long-running work unit by name. Cases survive \
                across conversations and let multi-day work stay coherent. The name should \
                be specific enough to find later — 'PS 89 invoice #3', 'IS 217 sample-match \
                invoice', not just 'invoice'. Returns the case id. \
                Idempotent: opening a case with an existing name resumes it instead of \
                creating a duplicate. Update the summary by passing a new summary string."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string" },
                    "summary": { "type": "string", "description": "2-4 sentences of state-of-play." },
                    "parentCaseId": { "type": "integer" }
                },
                "required": ["name"]
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: OpenInput = serde_json::from_value(input)?;
        let state = ctx.app.state::<AppState>();
        let workspace_id = state.workspace.read().await.active_id;
        let case = cases_db::upsert_open(
            &ctx.db.pool,
            workspace_id,
            CaseInput {
                name: p.name,
                summary: p.summary,
                parent_case_id: p.parent_case_id,
            },
        )
        .await?;
        Ok(serde_json::to_string(&case)?)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NoteInput {
    case_id: i64,
    /// 'decision' | 'note' | 'reconciliation' | 'document' | 'output'
    kind: String,
    /// JSON-serializable artifact payload — decision text, table data, etc.
    payload: Value,
    #[serde(default)]
    document_id: Option<i64>,
}

#[async_trait]
impl Tool for NoteCaseTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "note_case".into(),
            description: "Record an artifact under a case — a decision made, a \
                reconciliation table, a generated output, or a free-form note. The \
                payload is JSON that future Travis turns can recall. Use this when you \
                want a fact to survive past the current conversation."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "caseId": { "type": "integer" },
                    "kind": {
                        "type": "string",
                        "enum": ["decision", "note", "reconciliation", "document", "output"]
                    },
                    "payload": { "description": "JSON value — shape depends on kind." },
                    "documentId": { "type": "integer", "description": "When kind=document/output." }
                },
                "required": ["caseId", "kind", "payload"]
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: NoteInput = serde_json::from_value(input)?;
        let payload_json = serde_json::to_string(&p.payload)?;
        let id = cases_db::add_artifact(
            &ctx.db.pool,
            p.case_id,
            &p.kind,
            &payload_json,
            p.document_id,
        )
        .await?;
        Ok(serde_json::to_string(&json!({
            "ok": true,
            "artifactId": id
        }))?)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CloseInput {
    case_id: i64,
}

#[async_trait]
impl Tool for CloseCaseTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "close_case".into(),
            description: "Close a case when the work it tracks is complete. The case stays \
                in the DB and can be reopened later if needed; closed cases don't surface in \
                the active-cases prompt block."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": { "caseId": { "type": "integer" } },
                "required": ["caseId"]
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: CloseInput = serde_json::from_value(input)?;
        cases_db::close(&ctx.db.pool, p.case_id).await?;
        Ok(serde_json::to_string(&json!({ "ok": true, "caseId": p.case_id }))?)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FindInput {
    /// Substring or partial name match. Case-insensitive.
    query: String,
    /// Include closed cases too.
    #[serde(default)]
    include_closed: bool,
}

#[async_trait]
impl Tool for FindCaseTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "find_case".into(),
            description: "Look up cases by name substring. Useful when the user references \
                prior work ('back to the PS 89 reconciliation') — find the case, recall the \
                summary, continue. Returns up to 5 matches ranked by recency."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "includeClosed": { "type": "boolean" }
                },
                "required": ["query"]
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: FindInput = serde_json::from_value(input)?;
        let state = ctx.app.state::<AppState>();
        let ws = state.workspace.read().await.visible_ids.clone();
        let like = format!("%{}%", p.query.to_lowercase());
        let placeholders = (1..=ws.len())
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>()
            .join(", ");
        let status_filter = if p.include_closed {
            ""
        } else {
            "AND status = 'open'"
        };
        let sql = format!(
            "SELECT id, workspace_id, name, summary, status, parent_case_id,
                    conversation_ids_json, started_at, last_activity_at, closed_at
             FROM travis_case
             WHERE workspace_id IN ({placeholders})
               AND LOWER(name) LIKE ?{}
               {status_filter}
             ORDER BY last_activity_at DESC LIMIT 5",
            ws.len() + 1
        );
        let mut q = sqlx::query_as::<_, cases_db::Case>(&sql);
        for w in &ws {
            q = q.bind(*w);
        }
        q = q.bind(&like);
        let cases = q.fetch_all(&ctx.db.pool).await?;
        Ok(serde_json::to_string(&json!({ "cases": cases }))?)
    }
}
