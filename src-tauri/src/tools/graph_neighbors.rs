//! `graph_neighbors` — multi-hop entity traversal (BRAIN.md Phase 4.5 #4).
//!
//! Lets the LLM answer questions like "who's connected to PS 142?"
//! or "what entities cluster around the audit topic?" without writing
//! SQL. Walks the `mentioned_with` edges up to 3 hops out from a
//! start entity, returning entities ordered by hop distance then by
//! co-mention strength.
//!
//! Read-only. Workspace-clamped via the active workspace set.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::Manager;

use crate::llm::ToolDef;
use crate::memory;
use crate::tools::{Tool, ToolContext};
use crate::AppState;

pub struct GraphNeighborsTool;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    /// Entity to start from. Pass id directly, or name (resolves
    /// case-insensitively in the active workspace).
    #[serde(default)]
    entity_id: Option<i64>,
    #[serde(default)]
    name: Option<String>,
    /// Max hops to traverse (1-3). Default 2.
    #[serde(default)]
    max_hops: Option<i64>,
    /// Max neighbors to return (1-50). Default 10.
    #[serde(default)]
    limit: Option<i64>,
}

#[async_trait]
impl Tool for GraphNeighborsTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "graph_neighbors".into(),
            description: "Walk the entity graph from a start entity up to \
                3 hops out, following mentioned_with edges. Returns \
                neighbors ordered by hop distance then by co-mention \
                strength. Use to answer 'who's connected to X' or 'what \
                clusters around Y' style questions. Pass either entityId \
                or name (case-insensitive lookup in the active \
                workspace)."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "entityId": { "type": "integer" },
                    "name": { "type": "string", "description": "Display name (case-insensitive)." },
                    "maxHops": { "type": "integer", "description": "1-3. Default 2." },
                    "limit": { "type": "integer", "description": "1-50. Default 10." }
                }
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: Input = serde_json::from_value(input)?;
        let state = ctx.app.state::<AppState>();
        let snapshot = state.workspace.read().await.clone();
        let visible = snapshot.visible_ids.clone();

        let start_id = if let Some(id) = p.entity_id {
            id
        } else if let Some(name) = p.name.as_deref() {
            let trimmed = name.trim();
            if trimmed.is_empty() {
                anyhow::bail!("name cannot be empty");
            }
            let mut found: Option<i64> = None;
            for ws in &visible {
                if let Some((eid, _kind, _pack)) =
                    crate::identity::find_by_normalized_name(&ctx.db.pool, *ws, trimmed).await
                {
                    found = Some(eid);
                    break;
                }
            }
            found.ok_or_else(|| anyhow::anyhow!("no entity named \"{trimmed}\" in visible workspaces"))?
        } else {
            anyhow::bail!("entityId or name is required");
        };

        let neighbors = memory::graph::neighbors(
            &ctx.db.pool,
            &visible,
            start_id,
            p.max_hops.unwrap_or(2),
            p.limit.unwrap_or(10),
        )
        .await;

        let out: Vec<Value> = neighbors
            .iter()
            .map(|n| {
                json!({
                    "entityId": n.entity_id,
                    "name": n.display_name,
                    "kind": n.kind,
                    "hops": n.hops,
                    "strength": n.strength,
                })
            })
            .collect();

        Ok(json!({
            "startEntityId": start_id,
            "neighbors": out,
            "count": out.len(),
        })
        .to_string())
    }
}
