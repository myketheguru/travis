//! Travis-to-Travis LLM tools.
//!
//! - `t2t_list_contacts` — reads the user's active T2T relationships
//!   so the LLM can resolve "Taylor" -> a user id before sending a
//!   query. Read-only.
//! - `t2t_ask` — sends a question from this Travis to another. Writes
//!   a T2t query; the recipient's Travis picks it up + drafts a
//!   response asynchronously. This tool is read-only in the LLM
//!   registry sense (the LLM invokes it directly) because the query
//!   is a scoped, revocable communication + doesn't take a side
//!   effect on the user's own data. The recipient still gates their
//!   own reply via approval on the T2tConvoCard.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::cloud::t2t;
use crate::llm::ToolDef;

use super::{Tool, ToolContext};

// ─── t2t_list_contacts ────────────────────────────────────────────

pub struct T2tListContactsTool;

#[async_trait]
impl Tool for T2tListContactsTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "t2t_list_contacts".into(),
            description: "List the user's active Travis-to-Travis contacts \
                — other Travises this user has an accepted relationship \
                with. Use this to resolve a name like 'Taylor' to a user \
                id before calling t2t_ask. Returns email + display name + \
                user id per contact.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {},
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, _input: Value) -> anyhow::Result<String> {
        let all = t2t::list_relationships(&ctx.http).await?;
        let active: Vec<_> = all
            .into_iter()
            .filter(|r| matches!(r.status, t2t::RelationshipStatus::Active))
            .map(|r| {
                let other_id = if r.other_email.is_some() || r.other_name.is_some() {
                    // The join field populated by cloud tells us which side
                    // is the "other" party.
                    r.to_user_id.clone()
                } else {
                    r.from_user_id.clone()
                };
                json!({
                    "user_id": other_id,
                    "email": r.other_email,
                    "name": r.other_name,
                })
            })
            .collect();
        if active.is_empty() {
            return Ok("No active Travis-to-Travis contacts. Ask the user to invite someone from Settings → T2T.".into());
        }
        Ok(serde_json::to_string(&active)?)
    }
}

// ─── t2t_ask ──────────────────────────────────────────────────────

pub struct T2tAskTool;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AskInput {
    /// The other user's cloud id — resolve via t2t_list_contacts first.
    to_user_id: String,
    /// The question to ask. Full sentence; the other side's Travis
    /// will read it in context.
    question: String,
    /// Optional. Ignored on send today; carried for future work.
    #[serde(default)]
    expires_after_days: Option<u32>,
}

#[async_trait]
impl Tool for T2tAskTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "t2t_ask".into(),
            description: "Send a question from this Travis to another via \
                Travis-to-Travis. The recipient's Travis will draft a \
                reply and the recipient will approve, edit, or decline it. \
                Use when the user says things like 'ask Taylor about X' or \
                'check with Michael on Y'. First call t2t_list_contacts to \
                resolve the recipient's name to a user id. Returns the new \
                query id, which you can reference back in your response as \
                a t2t_convo message part with state='sending'.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "toUserId": {
                        "type": "string",
                        "description": "The recipient's user id, from t2t_list_contacts."
                    },
                    "question": {
                        "type": "string",
                        "description": "The question in one or two sentences."
                    },
                    "expiresAfterDays": {
                        "type": "number",
                        "description": "Optional TTL. Defaults to no expiry.",
                        "nullable": true
                    }
                },
                "required": ["toUserId", "question"]
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: AskInput = serde_json::from_value(input)?;
        let id = t2t::send_query(
            &ctx.http,
            &p.to_user_id,
            &p.question,
            None,
            p.expires_after_days,
        )
        .await?;
        Ok(json!({
            "query_id": id,
            "state": "sending",
            "note": "Query dispatched. Include a t2t_convo message part in your response referencing this query_id so the user can see it in the workspace.",
        })
        .to_string())
    }
}
