use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::domain::task::{self, TaskFilter};
use crate::llm::ToolDef;

use super::{Tool, ToolContext};

pub struct ListOpenTasksTool;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    link_kind: Option<String>,
    #[serde(default)]
    link_id: Option<i64>,
    #[serde(default)]
    limit: Option<usize>,
}

#[async_trait]
impl Tool for ListOpenTasksTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "list_open_tasks".into(),
            description: "Look up tasks with optional filters. The full open-tasks list is already injected into your context, so only call this when you need a different filter (e.g. tasks linked to a specific coach) or a status other than 'open'.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "status": {
                        "type": "string",
                        "enum": ["open", "done", "snoozed", "dropped"],
                        "description": "Filter by status. Omit for all."
                    },
                    "linkKind": {
                        "type": "string",
                        "enum": ["coach", "school", "coach_hours", "signing_sheet", "invoice"],
                        "description": "Filter by linked entity kind."
                    },
                    "linkId": {
                        "type": "integer",
                        "description": "Filter by the linked entity id (must accompany linkKind)."
                    },
                    "limit": { "type": "integer", "description": "Max rows (default 20, cap 100)." }
                }
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        use tauri::Manager;
        let p: Input = serde_json::from_value(input)?;
        let limit = p.limit.unwrap_or(20).min(100);
        let app_state = ctx.app.state::<crate::AppState>();
        let ws = app_state.workspace.read().await.clone();
        let rows = task::list(
            &ctx.db.pool,
            &ws,
            TaskFilter {
                status: p.status,
                link_kind: p.link_kind,
                link_id: p.link_id,
            },
        )
        .await
        .map_err(|e| anyhow::anyhow!("list tasks: {e}"))?;
        if rows.is_empty() {
            return Ok("(no tasks matched)".into());
        }
        let mut out = String::new();
        for t in rows.iter().take(limit) {
            let due = t.due_at.as_deref().unwrap_or("—");
            let link = match (&t.link_kind, t.link_id) {
                (Some(k), Some(id)) => format!(" [{k}#{id}]"),
                _ => String::new(),
            };
            out.push_str(&format!(
                "- [{id}] ({status}, p={pri}, due {due}{link}) {title}\n",
                id = t.id,
                status = t.status,
                pri = t.priority,
                due = due,
                link = link,
                title = t.title,
            ));
        }
        if rows.len() > limit {
            out.push_str(&format!("\n…+{} more rows.", rows.len() - limit));
        }
        Ok(out.trim_end().to_string())
    }
}
