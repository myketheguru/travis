use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::llm::ToolDef;
use crate::memory;

use super::{Tool, ToolContext};

pub struct SearchMemoryTool;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    query: String,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    entities: Option<Vec<String>>,
}

#[async_trait]
impl Tool for SearchMemoryTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "search_memory".into(),
            description: "Semantic-search the user's past journal notes for snippets relevant to a focused query. Use when you need more context than what was auto-injected — e.g. when answering a follow-up question or when the current note references something that may have been mentioned weeks ago.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Natural-language search phrase." },
                    "limit": { "type": "integer", "description": "Max hits (default 5, cap 15)." },
                    "entities": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional entity names (coaches, schools, depts) to boost matching for. Pass an empty array if none."
                    }
                },
                "required": ["query"]
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: Input = serde_json::from_value(input)?;
        let q = p.query.trim();
        if q.is_empty() {
            anyhow::bail!("query is required");
        }
        let limit = p.limit.unwrap_or(5).min(15);
        let entities = p.entities.unwrap_or_default();
        let hits = memory::retrieve(&ctx.db.pool, q, &entities, limit)
            .await
            .map_err(|e| anyhow::anyhow!("retrieve: {e}"))?;
        if hits.is_empty() {
            return Ok(format!("(no memory hits for '{q}')"));
        }
        let mut out = String::new();
        for (i, h) in hits.iter().enumerate() {
            let date = h.created_at.split('T').next().unwrap_or(&h.created_at);
            let snippet = h.text.chars().take(220).collect::<String>();
            out.push_str(&format!(
                "[{idx}] {date} · {kind}#{sid} · score {score:.2}\n{snippet}\n\n",
                idx = i + 1,
                date = date,
                kind = h.kind,
                sid = h.source_id,
                score = h.score,
                snippet = snippet,
            ));
        }
        Ok(out.trim_end().to_string())
    }
}
