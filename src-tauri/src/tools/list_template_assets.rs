//! `list_template_assets` and `find_template_assets` — surface the
//! global, deduped, categorized binary asset library to the LLM.
//!
//! `list_template_assets(documentId)` returns the per-document manifest
//! (every asset extracted FROM this sample, with kind + display_name +
//! bbox + file path).
//!
//! `find_template_assets({kind?, query?, sourceDocumentId?})` is the
//! library-wide search. The LLM uses it to grab assets EXTRACTED FROM
//! OTHER samples and reuse them — e.g. "find the L2E logo from any
//! prior sample" while generating a doc the user attached without one.
//!
//! Both tools pair with `analyze_document_styling` (layout JSON) to let
//! the LLM produce true 1:1 visual replicas instead of approximating
//! logos and headings in code.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::Manager;

use crate::llm::ToolDef;
use crate::tools::{Tool, ToolContext};
use crate::AppState;

// ---------- list_template_assets ----------

pub struct ListTemplateAssetsTool;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListInput {
    document_id: i64,
    /// If true and no extraction has been started, schedule one now.
    /// Default: true.
    #[serde(default = "default_true")]
    schedule_if_missing: bool,
}

fn default_true() -> bool {
    true
}

#[async_trait]
impl Tool for ListTemplateAssetsTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "list_template_assets".into(),
            description: "Return the binary asset manifest extracted from a sample/template \
                document — every embedded image (logo, header banner, signature, watermark) \
                plus full-page renders at 300 DPI. Each asset has a `kind`, `displayName`, \
                `absPath`, page index, and bbox in PDF points.\n\n\
                Use BEFORE writing Python that should reproduce a sample 1:1. In your \
                run_python script, open the absolute paths with `PIL.Image.open(path)` and \
                draw them at the original bbox — that's how you achieve a true replica \
                instead of approximating logos in code.\n\n\
                Status semantics: `ready` = manifest populated, paths usable. `extracting` / \
                `pending` = come back next turn or fall back to styling-only. `failed` = \
                use styling-only. `missing` = pass `scheduleIfMissing=true` to start.\n\n\
                Assets are GLOBAL and DEDUPED — the same logo lifted from prior samples is \
                ONE asset. To grab assets from other samples, use `find_template_assets`."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "documentId": {
                        "type": "integer",
                        "description": "The sample document's id."
                    },
                    "scheduleIfMissing": {
                        "type": "boolean",
                        "description": "Schedule extraction when no prior attempt exists. Default true."
                    }
                },
                "required": ["documentId"]
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: ListInput = serde_json::from_value(input)?;
        let state = ctx.app.state::<AppState>();

        if p.schedule_if_missing {
            crate::template_assets::schedule_extraction(
                ctx.app.clone(),
                state.db.pool.clone(),
                p.document_id,
            )
            .await;
        }

        let row = crate::template_assets::get_extraction(&state.db.pool, p.document_id)
            .await
            .map_err(|e| anyhow::anyhow!("template_assets lookup failed: {e}"))?;

        let result = match row {
            None => json!({
                "documentId": p.document_id,
                "status": "missing",
                "hint": "No extraction row exists. Pass scheduleIfMissing=true to start one.",
            }),
            Some(r) => {
                let manifest: Value =
                    r.manifest_json.as_deref()
                        .and_then(|s| serde_json::from_str(s).ok())
                        .unwrap_or(Value::Null);
                json!({
                    "documentId": r.document_id,
                    "status": r.status,
                    "imageCount": r.image_count,
                    "pageCount": r.page_count,
                    "error": r.error,
                    "extractedAt": r.extracted_at,
                    "manifest": manifest,
                })
            }
        };

        Ok(result.to_string())
    }
}

// ---------- find_template_assets ----------

pub struct FindTemplateAssetsTool;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FindInput {
    /// Filter by category. One of: logo, header_banner, signature,
    /// watermark, page_render, embedded_image. Omit to search all.
    #[serde(default)]
    kind: Option<String>,
    /// Case-insensitive substring against display_name.
    #[serde(default)]
    query: Option<String>,
    /// Restrict to assets sourced from a specific document.
    #[serde(default)]
    source_document_id: Option<i64>,
    /// Cap results. Default 50, max 200.
    #[serde(default)]
    limit: Option<i64>,
}

#[async_trait]
impl Tool for FindTemplateAssetsTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "find_template_assets".into(),
            description: "Search the global template asset library — every image extracted \
                from every prior sample/template the user has shown Travis. The same logo \
                seen across twenty invoices is ONE asset; this tool finds it regardless of \
                which sample is in the current chat.\n\n\
                Use when the user asks for a doc that should match the org's branding but \
                hasn't attached a sample in THIS turn. Example: 'invoice PS498 for May' \
                with no template attached → call find_template_assets({kind:'logo'}) to \
                pull the L2E logo from a prior sample, plus header_banner if you need it. \
                Then embed those PNG paths in run_python.\n\n\
                Returned fields per asset: `id`, `kind`, `displayName`, `absPath` (use \
                with PIL.Image.open), `widthPx`, `heightPx`. Filter by `kind` for \
                category, `query` for name substring, `sourceDocumentId` to constrain to \
                one sample. Empty result = nothing in the library yet for that filter."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "kind": {
                        "type": "string",
                        "description": "logo | header_banner | signature | watermark | page_render | embedded_image"
                    },
                    "query": {
                        "type": "string",
                        "description": "Substring match against displayName, case-insensitive."
                    },
                    "sourceDocumentId": {
                        "type": "integer",
                        "description": "Restrict to assets extracted from this source doc."
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Cap results. Default 50, max 200."
                    }
                }
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: FindInput = serde_json::from_value(input)?;
        let state = ctx.app.state::<AppState>();
        let ws_id = state
            .db
            .meta("active_workspace_id")
            .await
            .ok()
            .flatten()
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(1);

        let rows = crate::template_assets::find_assets(
            &state.db.pool,
            ws_id,
            p.kind.as_deref(),
            p.query.as_deref(),
            p.source_document_id,
            p.limit.unwrap_or(50),
        )
        .await
        .map_err(|e| anyhow::anyhow!("find_template_assets failed: {e}"))?;

        let assets: Vec<Value> = rows
            .iter()
            .map(|r| {
                json!({
                    "id": r.id,
                    "kind": r.kind,
                    "displayName": r.display_name,
                    "absPath": r.abs_path,
                    "widthPx": r.width_px,
                    "heightPx": r.height_px,
                })
            })
            .collect();

        Ok(json!({
            "count": assets.len(),
            "assets": assets,
        })
        .to_string())
    }
}
