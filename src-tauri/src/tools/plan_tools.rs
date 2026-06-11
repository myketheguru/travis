//! v0.20.12 — LLM-facing tools for the plan + step substrate.
//!
//! Three tools:
//! - `create_plan` — declare a goal-scoped sequence of named steps
//! - `record_step_result` — cache a step's output by key
//! - `get_step_result` — fast cache lookup. If status='done', skip
//!   the expensive work and reuse the prior result.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::Manager;

use crate::llm::ToolDef;
use crate::plans::{
    self, active_plan_for_conversation, create_plan, get_step, record_step, PlanStepInput,
};
use crate::tools::{Tool, ToolContext};
use crate::AppState;

// ---------- create_plan ----------

pub struct CreatePlanTool;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateInput {
    goal: String,
    steps: Vec<StepInputJson>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StepInputJson {
    key: String,
    purpose: String,
    #[serde(default)]
    depends_on: Vec<String>,
}

#[async_trait]
impl Tool for CreatePlanTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "create_plan".into(),
            description: "Declare a goal-scoped sequence of named steps at the start of a \
                complex turn. Steps you record become CACHED — calling get_step_result later \
                returns the cached output without redoing the work. This is how Travis avoids \
                re-reading the same spreadsheet 5 times across one turn or re-generating an \
                identical PDF in a follow-up turn.\n\n\
                When to use: any time you're about to do multi-step work (read inputs, filter, \
                compute, generate) that takes more than 1-2 tool calls. Skip it for trivial \
                queries.\n\n\
                Step keys are short snake_case identifiers you pick: `read_signin_log`, \
                `filter_is217_dates`, `generate_invoice_pdf`. The (planId, key) pair is unique \
                — calling create_plan again with the same key returns the same plan.\n\n\
                After create_plan returns a planId, run each step normally (read_document, \
                run_python, etc.), then call `record_step_result(planId, key, result)` to \
                cache the output. On the next turn, call `get_step_result(planId, key)` FIRST \
                — if it returns a cached value, use it directly and skip the expensive call.\n\n\
                Returns: { planId: integer }"
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "goal": {
                        "type": "string",
                        "description": "One-line description of what this plan accomplishes (e.g. 'Generate IS 217 invoice LTE2026217002 from PO + sign-in log')."
                    },
                    "steps": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "key": { "type": "string", "description": "Short snake_case identifier." },
                                "purpose": { "type": "string", "description": "One-line human description (shows in the chat as the step label)." },
                                "dependsOn": {
                                    "type": "array",
                                    "items": { "type": "string" },
                                    "description": "Optional list of step keys this depends on."
                                }
                            },
                            "required": ["key", "purpose"]
                        }
                    }
                },
                "required": ["goal", "steps"]
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: CreateInput = serde_json::from_value(input)?;
        let state = ctx.app.state::<AppState>();
        let conv_id = ctx
            .conversation_id
            .ok_or_else(|| anyhow::anyhow!("create_plan needs a conversation context"))?;

        // Reuse the conversation's active plan if one already exists
        // with the same goal. Avoids accidental fan-out when the LLM
        // calls create_plan twice in the same flow.
        if let Some(existing) = active_plan_for_conversation(&state.db.pool, conv_id).await? {
            if existing.goal.eq_ignore_ascii_case(p.goal.trim()) {
                return Ok(json!({
                    "planId": existing.id,
                    "reused": true,
                    "goal": existing.goal,
                })
                .to_string());
            }
        }

        let steps: Vec<PlanStepInput> = p
            .steps
            .into_iter()
            .map(|s| PlanStepInput {
                key: s.key,
                purpose: s.purpose,
                depends_on: s.depends_on,
            })
            .collect();
        let plan_id = create_plan(&state.db.pool, conv_id, &p.goal, &steps).await?;
        Ok(json!({"planId": plan_id, "reused": false}).to_string())
    }
}

// ---------- record_step_result ----------

pub struct RecordStepResultTool;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecordInput {
    plan_id: i64,
    key: String,
    /// 'done' | 'failed' | 'skipped'. Default 'done'.
    #[serde(default = "default_done")]
    status: String,
    /// Free-form structured result the step produced. Persists as
    /// the cache value.
    #[serde(default)]
    result: Option<Value>,
    /// Document ids the step produced (when applicable).
    #[serde(default)]
    document_ids: Vec<i64>,
    #[serde(default)]
    error: Option<String>,
}

fn default_done() -> String {
    "done".into()
}

#[async_trait]
impl Tool for RecordStepResultTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "record_step_result".into(),
            description: "Cache a plan step's result so future turns can reuse it without \
                redoing the work. Call this AFTER you've successfully completed a step \
                (read a document, parsed a spreadsheet, generated a PDF). Pass the produced \
                value as `result` — typically the JSON of whatever run_python returned, or a \
                summary of what was extracted.\n\n\
                For PDF-generation steps, pass `documentIds: [N]` so the cache also knows \
                which file the step produced.\n\n\
                Status defaults to 'done'. Use 'failed' with `error` when a step couldn't \
                complete. Use 'skipped' when the step turned out to be unnecessary.\n\n\
                Once a step is 'done', subsequent get_step_result calls return the cached \
                value instantly. Re-running record_step_result on a done step OVERWRITES the \
                cache — useful when you re-do a step with refined inputs."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "planId":     { "type": "integer" },
                    "key":        { "type": "string", "description": "Step key from create_plan." },
                    "status":     { "type": "string", "description": "'done' | 'failed' | 'skipped'. Default 'done'." },
                    "result":     { "description": "The step's output as JSON (object, string, array, number). Cached for reuse." },
                    "documentIds":{ "type": "array", "items": { "type": "integer" }, "description": "Doc ids this step produced." },
                    "error":      { "type": ["string", "null"], "description": "Error message when status='failed'." }
                },
                "required": ["planId", "key"]
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: RecordInput = serde_json::from_value(input)?;
        let state = ctx.app.state::<AppState>();
        let result_json: Option<String> = p.result.map(|v| v.to_string());
        let docs: Option<&[i64]> = if p.document_ids.is_empty() {
            None
        } else {
            Some(&p.document_ids)
        };
        record_step(
            &state.db.pool,
            p.plan_id,
            &p.key,
            &p.status,
            result_json.as_deref(),
            docs,
            p.error.as_deref(),
        )
        .await?;
        Ok(json!({"ok": true, "planId": p.plan_id, "key": p.key, "status": p.status}).to_string())
    }
}

// ---------- get_step_result ----------

pub struct GetStepResultTool;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetInput {
    plan_id: i64,
    key: String,
}

#[async_trait]
impl Tool for GetStepResultTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "get_step_result".into(),
            description: "Fetch a plan step's cached result. CALL THIS FIRST before redoing \
                expensive work — if the prior turn (or a prior attempt this turn) already \
                completed the step, you get back the cached output and save 30-60s of wall \
                time per skipped step.\n\n\
                Returned shape: { found: bool, status: string, result: any, documentIds: [int] }. \
                When status='done', `result` and `documentIds` contain the cached values. When \
                status='pending' or 'running', the step hasn't completed yet — actually do the \
                work, then call record_step_result.\n\n\
                Example flow for an invoice regen:\n\
                1. create_plan with steps [read_log, filter_dates, generate_pdf]\n\
                2. get_step_result(planId, 'read_log') → found=false → run_python, then record_step_result\n\
                3. get_step_result(planId, 'filter_dates') → found=false → run_python, then record\n\
                4. get_step_result(planId, 'generate_pdf') → found=false → run_python, then record with docId\n\
                Next turn the same plan exists. get_step_result hits the cache for 1 and 2; \
                only generate_pdf re-runs because the user asked for a tweak."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "planId": { "type": "integer" },
                    "key":    { "type": "string" }
                },
                "required": ["planId", "key"]
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: GetInput = serde_json::from_value(input)?;
        let state = ctx.app.state::<AppState>();
        let step = get_step(&state.db.pool, p.plan_id, &p.key).await?;
        let result = match step {
            None => json!({"found": false}),
            Some(s) => {
                let result_value: Value = s
                    .result_json
                    .as_deref()
                    .and_then(|j| serde_json::from_str(j).ok())
                    .unwrap_or(Value::Null);
                let doc_ids: Vec<i64> = s
                    .document_ids
                    .as_deref()
                    .map(|raw| {
                        raw.split(',')
                            .filter_map(|x| x.trim().parse::<i64>().ok())
                            .collect()
                    })
                    .unwrap_or_default();
                json!({
                    "found": true,
                    "status": s.status,
                    "purpose": s.purpose,
                    "result": result_value,
                    "documentIds": doc_ids,
                    "error": s.error,
                    "completedAt": s.completed_at,
                })
            }
        };
        Ok(result.to_string())
    }
}

// ---------- list_plan_steps ----------

pub struct ListPlanStepsTool;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListInput {
    plan_id: i64,
}

#[async_trait]
impl Tool for ListPlanStepsTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "list_plan_steps".into(),
            description: "List every step in a plan with its status + cached result. Use to \
                check progress mid-turn or to recover the plan state when resuming a \
                multi-turn flow."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "planId": { "type": "integer" }
                },
                "required": ["planId"]
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: ListInput = serde_json::from_value(input)?;
        let state = ctx.app.state::<AppState>();
        let steps = plans::list_steps(&state.db.pool, p.plan_id).await?;
        Ok(json!({"steps": steps}).to_string())
    }
}
