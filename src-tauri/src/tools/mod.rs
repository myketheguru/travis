use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use tauri::AppHandle;

use crate::db::Db;
use crate::llm::ToolDef;

pub mod clipboard;
pub mod find_documents;
pub mod graph_neighbors;
pub mod list_open_tasks;
pub mod open_url;
pub mod pack_introspect;
pub mod pack_query;
pub mod preview_document;
pub mod read_document;
pub mod reconcile_documents;
pub mod run_python;
pub mod search_memory;
pub mod update_document;
pub mod web_fetch;

/// Per-call shared services available to tool implementations.
pub struct ToolContext {
    pub http: reqwest::Client,
    pub app: AppHandle,
    pub db: Arc<Db>,
    /// v0.14.0 — conversation this tool call belongs to. When set,
    /// the registry wraps the tool execution in a [`crate::steps::Step`]
    /// so the chat UI shows a named substep. None when the tool is
    /// being invoked outside a conversation context.
    pub conversation_id: Option<i64>,
    /// v0.14.0 — parent step id if this tool call is itself a
    /// sub-step of a larger operation (e.g. run_python invoked inside
    /// a workflow finalize).
    pub parent_step_id: Option<String>,
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn definition(&self) -> ToolDef;
    /// Execute the tool with raw JSON input (already validated against the
    /// definition's input_schema by the LLM provider). Return a string the
    /// LLM will see as the tool result content.
    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String>;
}

pub struct ToolRegistry {
    tools: Vec<Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self { tools: Vec::new() }
    }

    pub fn register(&mut self, t: Box<dyn Tool>) {
        self.tools.push(t);
    }

    pub fn definitions(&self) -> Vec<ToolDef> {
        self.tools.iter().map(|t| t.definition()).collect()
    }

    pub async fn execute(
        &self,
        ctx: &ToolContext,
        name: &str,
        input: Value,
    ) -> anyhow::Result<String> {
        for t in &self.tools {
            if t.definition().name == name {
                // v0.14.0 step-streaming wrapper. When a conversation
                // context is set, every tool call surfaces in the
                // chat UI as a named substep with checkmark + summary
                // + duration. When no conversation is set (e.g.
                // ask_travis from a settings panel), we bypass.
                let Some(conv_id) = ctx.conversation_id else {
                    return t.execute(ctx, input).await;
                };
                let step_name = human_label_for_tool(name);
                let step = match crate::steps::Step::start(
                    &ctx.app,
                    &ctx.db.pool,
                    conv_id,
                    crate::steps::StepKind::ToolCall,
                    step_name,
                    summarize_input(&input),
                    ctx.parent_step_id.clone(),
                )
                .await
                {
                    Ok(s) => Some(s),
                    Err(e) => {
                        tracing::warn!("could not start step for tool {name}: {e}");
                        None
                    }
                };

                let result = t.execute(ctx, input).await;
                if let Some(step) = step {
                    match &result {
                        Ok(_) => {
                            let _ = step.complete_ok(&ctx.app, &ctx.db.pool, None).await;
                        }
                        Err(e) => {
                            let _ = step
                                .complete_err(&ctx.app, &ctx.db.pool, e.to_string())
                                .await;
                        }
                    }
                }
                return result;
            }
        }
        anyhow::bail!("unknown tool: {name}")
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Map a tool's internal name to a user-friendly step label.
/// Used to title the step Claude-style ("Reading PO doc" not "read_document").
fn human_label_for_tool(tool_name: &str) -> String {
    match tool_name {
        "read_document" => "Reading document",
        "find_documents" => "Finding matching documents",
        "reconcile_documents" => "Reconciling across documents",
        "update_document_field" => "Updating extracted field",
        "preview_document" => "Opening document",
        "run_python" => "Running Python",
        "search_memory" => "Searching memory",
        "list_open_tasks" => "Reading open tasks",
        "web_fetch" => "Fetching web page",
        "open_url" => "Opening URL",
        "pack_introspect" => "Inspecting pack schema",
        "pack_query" => "Querying pack table",
        "graph_neighbors" => "Walking the entity graph",
        // L2E pack tools
        "lte_find_or_create_school" => "Finding or creating school",
        "lte_find_contract" => "Finding contract",
        "lte_find_engagement" => "Finding contract",
        "lte_summarize_context" => "Summarizing context",
        "lte_quote_margin" => "Computing quote margin",
        "lte_validate_invoice" => "Validating invoice",
        _ => return format!("Calling {tool_name}"),
    }
    .to_string()
}

/// Best-effort one-line summary of a tool's input for the step detail row.
/// Avoids dumping the whole JSON; surfaces the most useful identifying field.
fn summarize_input(input: &Value) -> Option<String> {
    let obj = input.as_object()?;
    // Common identifiers that make great detail lines
    for key in [
        "documentId",
        "documentIds",
        "engagementId",
        "schoolId",
        "contractId",
        "purpose",
        "query",
        "name",
    ] {
        if let Some(v) = obj.get(key) {
            let preview = match v {
                Value::String(s) if s.len() > 60 => format!("{}…", &s[..60]),
                Value::String(s) => s.clone(),
                Value::Number(n) => n.to_string(),
                Value::Array(arr) if arr.len() <= 5 => {
                    let parts: Vec<String> = arr
                        .iter()
                        .map(|x| match x {
                            Value::Number(n) => n.to_string(),
                            Value::String(s) => s.clone(),
                            _ => "?".into(),
                        })
                        .collect();
                    format!("[{}]", parts.join(", "))
                }
                Value::Array(arr) => format!("[{} items]", arr.len()),
                _ => continue,
            };
            return Some(format!("{key}={preview}"));
        }
    }
    None
}

/// The standard read-only registry: tools the LLM can invoke autonomously
/// without a user-confirmation gate. Write tools (defer_task, set_reminder,
/// draft_invoice, write_clipboard) flow through the proposed_action path so
/// the user keeps a veto.
///
/// `packs` is the list of enabled packs that may contribute additional
/// read-only tools. Pass [`crate::packs::enabled_packs()`] for the default
/// build.
pub fn read_only_registry(packs: &[&dyn crate::packs::PackHandle]) -> ToolRegistry {
    let mut reg = ToolRegistry::new();
    reg.register(Box::new(web_fetch::WebFetchTool));
    reg.register(Box::new(search_memory::SearchMemoryTool));
    reg.register(Box::new(list_open_tasks::ListOpenTasksTool));
    reg.register(Box::new(clipboard::ReadClipboardTool));
    reg.register(Box::new(open_url::OpenUrlTool));
    reg.register(Box::new(pack_introspect::PackIntrospectTool));
    reg.register(Box::new(pack_query::PackQueryTool));
    reg.register(Box::new(graph_neighbors::GraphNeighborsTool));
    reg.register(Box::new(read_document::ReadDocumentTool));
    reg.register(Box::new(find_documents::FindDocumentsTool));
    reg.register(Box::new(reconcile_documents::ReconcileDocumentsTool));
    reg.register(Box::new(update_document::UpdateDocumentFieldTool));
    reg.register(Box::new(preview_document::PreviewDocumentTool));
    reg.register(Box::new(run_python::RunPythonTool));
    for pack in packs {
        pack.register_tools(&mut reg);
    }
    reg
}
