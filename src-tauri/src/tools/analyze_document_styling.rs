//! `analyze_document_styling` LLM tool — extracts visual styling
//! features (colours, fonts, layout, signature) from a sample
//! document so the LLM can write Python that matches the sample.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::Manager;

use crate::documents::cmd::{analyze_document_styling as cmd, AnalyzeStylingParams};
use crate::llm::ToolDef;
use crate::tools::{Tool, ToolContext};
use crate::AppState;

pub struct AnalyzeDocumentStylingTool;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    document_id: i64,
    #[serde(default)]
    force: bool,
}

#[async_trait]
impl Tool for AnalyzeDocumentStylingTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "analyze_document_styling".into(),
            description: "Analyze a sample document's VISUAL styling — header colours, body font \
                family + size, table colours and border weights, signature column presence and \
                stroke type, layout features, margins, page size. Returns structured JSON.\n\n\
                Use BEFORE writing Python in run_python to generate a document that matches a \
                sample the user supplied (\"make this invoice look like that one\"). The output \
                is cached on the document, so subsequent calls return the same JSON instantly \
                unless `force` is true.\n\n\
                Use this only on user-supplied samples. Don't analyze Travis-generated documents \
                — the styling is already known."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "documentId": {
                        "type": "integer",
                        "description": "The sample document's id (from find_documents or a freshly attached chip)."
                    },
                    "force": {
                        "type": "boolean",
                        "description": "Force re-analysis even if cached. Default false."
                    }
                },
                "required": ["documentId"]
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: Input = serde_json::from_value(input)?;
        let state = ctx.app.state::<AppState>();
        let styling = cmd(
            ctx.app.clone(),
            state,
            AnalyzeStylingParams {
                document_id: p.document_id,
                force: p.force,
            },
        )
        .await
        .map_err(|e| anyhow::anyhow!("styling analysis failed: {e}"))?;

        Ok(serde_json::to_string(&json!({
            "documentId": p.document_id,
            "styling": styling,
        }))?)
    }
}
