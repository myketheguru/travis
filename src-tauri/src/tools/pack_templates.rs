//! LLM tools for pack_template — save, find, get.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::Manager;

use crate::llm::ToolDef;
use crate::templates::db as templates_db;
use crate::tools::{Tool, ToolContext};
use crate::AppState;

pub struct SavePackTemplateTool;
pub struct FindPackTemplateTool;
pub struct GetPackTemplateTool;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveInput {
    pack_slug: String,
    kind: String,
    label: String,
    #[serde(default)]
    counterparty_hint: Option<String>,
    styling_json: String,
    generation_code: String,
    #[serde(default)]
    sample_document_id: Option<i64>,
}

#[async_trait]
impl Tool for SavePackTemplateTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "save_pack_template".into(),
            description: "Save successful styling + generation code as a reusable template. \
                Call this AFTER Taylor confirms a custom-generated document (via run_python + \
                analyze_document_styling) looks right, so future requests for the same \
                counterparty use the saved code instantly. \n\n\
                Label the template clearly: 'IS 217 invoice layout' not 'invoice template'. \
                Set counterparty_hint to the school / customer name so future find_pack_template \
                calls can match. Upsert by (workspace, pack_slug, kind, label) — re-saving with \
                the same label updates instead of duplicating."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "packSlug": { "type": "string", "description": "e.g. 'lead-to-empower'" },
                    "kind": { "type": "string", "enum": ["invoice", "sign_in_sheet", "work_order", "other"] },
                    "label": { "type": "string", "description": "User-friendly name." },
                    "counterpartyHint": { "type": "string", "description": "School / customer hint for future matching." },
                    "stylingJson": { "type": "string", "description": "JSON.stringify of the analyze_document_styling output." },
                    "generationCode": { "type": "string", "description": "The Python code that produced the working document." },
                    "sampleDocumentId": { "type": "integer" }
                },
                "required": ["packSlug", "kind", "label", "stylingJson", "generationCode"]
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: SaveInput = serde_json::from_value(input)?;
        let state = ctx.app.state::<AppState>();
        let workspace_id = state.workspace.read().await.active_id;
        let tpl = templates_db::save(
            &ctx.db.pool,
            workspace_id,
            templates_db::PackTemplateInput {
                pack_slug: p.pack_slug,
                kind: p.kind,
                label: p.label,
                counterparty_hint: p.counterparty_hint,
                styling_json: p.styling_json,
                generation_code: p.generation_code,
                sample_document_id: p.sample_document_id,
            },
        )
        .await?;
        Ok(serde_json::to_string(&tpl)?)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FindInput {
    pack_slug: String,
    kind: String,
    #[serde(default)]
    counterparty_hint: Option<String>,
}

#[async_trait]
impl Tool for FindPackTemplateTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "find_pack_template".into(),
            description: "Look up saved templates by (pack_slug, kind, optional counterparty_hint). \
                Returns up to 5 matches, counterparty-specific ones first, then generic. Call this \
                BEFORE writing run_python from scratch — if a template already exists for this \
                customer + kind, reuse its generation_code directly with the relevant variables."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "packSlug": { "type": "string" },
                    "kind": { "type": "string" },
                    "counterpartyHint": { "type": "string" }
                },
                "required": ["packSlug", "kind"]
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: FindInput = serde_json::from_value(input)?;
        let state = ctx.app.state::<AppState>();
        let workspace_id = state.workspace.read().await.active_id;
        let tpls = templates_db::find(
            &ctx.db.pool,
            workspace_id,
            &p.pack_slug,
            &p.kind,
            p.counterparty_hint.as_deref(),
        )
        .await;
        Ok(serde_json::to_string(&json!({ "templates": tpls }))?)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetInput {
    template_id: i64,
}

#[async_trait]
impl Tool for GetPackTemplateTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "get_pack_template".into(),
            description: "Fetch a template by id and increment its used_count. Use after \
                find_pack_template to load the full styling + generation_code for execution. \
                The used_count helps surface the most-reused templates over time."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": { "templateId": { "type": "integer" } },
                "required": ["templateId"]
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: GetInput = serde_json::from_value(input)?;
        let tpl = templates_db::get_one(&ctx.db.pool, p.template_id).await?;
        templates_db::mark_used(&ctx.db.pool, p.template_id).await.ok();
        Ok(serde_json::to_string(&tpl)?)
    }
}
