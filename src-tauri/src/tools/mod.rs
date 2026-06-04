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
pub mod search_memory;
pub mod update_document;
pub mod web_fetch;

/// Per-call shared services available to tool implementations.
pub struct ToolContext {
    pub http: reqwest::Client,
    pub app: AppHandle,
    pub db: Arc<Db>,
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
                return t.execute(ctx, input).await;
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
    for pack in packs {
        pack.register_tools(&mut reg);
    }
    reg
}
