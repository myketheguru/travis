//! `search_conversations` — v0.19.0 LLM tool for pulling context from
//! prior conversations. Lets Travis say "the IS 217 work we did last
//! week — what was the rate again?" and actually find the answer by
//! searching across every thread in the workspace.
//!
//! Returns up to N matching threads with a relevance snippet (the
//! matching message excerpt with the query term highlighted by
//! position), conversation id, message id, role, created_at, and the
//! parent conversation's first user message as a thread label.
//!
//! Distinct from `search_memory`: that hits the embeddings index for
//! semantic recall of facts/claims/journal-extracted summaries. This
//! is a literal full-text search across raw message bodies — for
//! retrieving the original words the user or worker used.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::Manager;

use crate::llm::ToolDef;
use crate::tools::{Tool, ToolContext};
use crate::AppState;

pub struct SearchConversationsTool;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    /// Phrase to search for. Substring match (case-insensitive).
    query: String,
    /// Optional: exclude the active conversation from results (default
    /// true — usually you want to search OTHER threads, not the one
    /// you're already in).
    #[serde(default = "default_exclude_active")]
    exclude_active: bool,
    /// Max hits to return. Default 10, capped 30.
    #[serde(default)]
    limit: Option<i64>,
}

fn default_exclude_active() -> bool {
    true
}

#[async_trait]
impl Tool for SearchConversationsTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "search_conversations".into(),
            description: "Search across all prior conversation threads in this workspace for a \
                phrase. Use when the user asks about something handled in a previous thread, \
                or when current context references work done elsewhere ('last week's IS 217 \
                invoice', 'the rate we settled on with the DoF', etc.). \n\n\
                Returns up to N hits: [{conversationId, conversationLabel, messageId, role, \
                snippet, createdAt}] ordered by recency. The `snippet` is a ~200-char excerpt \
                centred on the matching phrase. The `conversationLabel` is the thread's title \
                or its first user-message preview, so the LLM can refer back to the user with \
                something they recognise (\"in the IS 217 thread from last Friday…\").\n\n\
                Distinct from `search_memory` (which hits the semantic embeddings index for \
                facts/claims). Use search_memory when you want WHAT was said; use \
                search_conversations when you want WHERE it was said and the exact words."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The phrase to search for. Case-insensitive substring match."
                    },
                    "excludeActive": {
                        "type": "boolean",
                        "description": "Default true — exclude the current conversation from results. Set false to include it (rarely useful)."
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Max hits, capped at 30. Default 10."
                    }
                },
                "required": ["query"]
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: Input = serde_json::from_value(input)?;
        let state = ctx.app.state::<AppState>();
        let visible = state.workspace.read().await.visible_ids.clone();
        if visible.is_empty() || p.query.trim().is_empty() {
            return Ok(json!({ "hits": [] }).to_string());
        }
        let lim = p.limit.unwrap_or(10).clamp(1, 30);
        let exclude_id = if p.exclude_active {
            ctx.conversation_id
        } else {
            None
        };

        let ws_placeholders: String = (4..4 + visible.len())
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>()
            .join(", ");
        // ?1 = like pattern, ?2 = limit, ?3 = exclude_id (NULL or i64),
        // ?4..  = workspace ids.
        let sql = format!(
            "SELECT m.id, m.conversation_id, m.role, m.content, m.created_at,
                    c.title,
                    (SELECT um.content FROM conversation_message um
                       WHERE um.conversation_id = c.id AND um.role = 'user'
                       ORDER BY um.id ASC LIMIT 1) AS first_user
             FROM conversation_message m
             JOIN conversation c ON c.id = m.conversation_id
             WHERE c.workspace_id IN ({ws_placeholders})
               AND (?3 IS NULL OR m.conversation_id != ?3)
               AND LOWER(SUBSTR(m.content, 1, 8000)) LIKE ?1
             ORDER BY m.created_at DESC
             LIMIT ?2"
        );
        let like_pattern = format!("%{}%", p.query.to_lowercase());
        let mut q = sqlx::query_as::<
            _,
            (i64, i64, String, String, String, Option<String>, Option<String>),
        >(&sql)
        .bind(&like_pattern)
        .bind(lim)
        .bind(exclude_id);
        for id in &visible {
            q = q.bind(id);
        }
        let rows = q.fetch_all(&state.db.pool).await?;

        let hits: Vec<Value> = rows
            .into_iter()
            .map(|(id, conv_id, role, content, created_at, title, first_user)| {
                let snippet = excerpt_around(&content, &p.query, 200);
                let label = title
                    .as_deref()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .or_else(|| {
                        first_user
                            .as_deref()
                            .map(|s| {
                                s.lines()
                                    .next()
                                    .unwrap_or("")
                                    .chars()
                                    .take(80)
                                    .collect::<String>()
                            })
                            .filter(|s| !s.is_empty())
                    })
                    .unwrap_or_else(|| format!("Conversation #{conv_id}"));
                json!({
                    "conversationId": conv_id,
                    "conversationLabel": label,
                    "messageId": id,
                    "role": role,
                    "snippet": snippet,
                    "createdAt": created_at,
                })
            })
            .collect();

        Ok(json!({ "hits": hits, "queryEcho": p.query }).to_string())
    }
}

/// Build a `radius`-character excerpt of `text` centred on the first
/// case-insensitive occurrence of `query`. Adds ellipses to either
/// end when the excerpt is a slice of a longer body.
fn excerpt_around(text: &str, query: &str, radius: usize) -> String {
    let lower = text.to_lowercase();
    let q = query.to_lowercase();
    let pos = lower.find(&q).unwrap_or(0);
    // Work in char-safe boundaries so we don't slice mid-utf-8.
    let chars: Vec<char> = text.chars().collect();
    // Find the char index that corresponds to byte position `pos`.
    let mut byte_count = 0;
    let mut char_at = 0;
    for (i, c) in chars.iter().enumerate() {
        if byte_count >= pos {
            char_at = i;
            break;
        }
        byte_count += c.len_utf8();
    }
    let half = radius / 2;
    let start = char_at.saturating_sub(half);
    let end = (char_at + half).min(chars.len());
    let core: String = chars[start..end].iter().collect();
    let prefix = if start > 0 { "…" } else { "" };
    let suffix = if end < chars.len() { "…" } else { "" };
    format!("{prefix}{core}{suffix}")
}
