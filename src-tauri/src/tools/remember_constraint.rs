//! `remember_constraint` — v0.19.0 LLM tool for writing pack memory.
//!
//! Lets Travis store user-stated rules / preferences / constraints
//! so they survive into future turns and even future conversations.
//! Mirrors Claude.ai's Project Knowledge / Memory feature, but
//! pack-scoped: a rule about LTE invoicing doesn't pollute prompts
//! when the user is doing tutoring work, and vice versa.
//!
//! Use cases the LLM should reach for this tool:
//! - User says "always include the PO number in the subject line for
//!   DoF invoices" → remember as rule, target_kind="contract" if a
//!   contract is in scope, else pack-wide.
//! - User says "Taylor prefers Net 30 terms" → preference, pack-wide.
//! - User says "no, 03/17 was pre-PO — don't include it for IS 217
//!   invoices" → constraint, target_kind="school", target_id=<IS217>.
//! - User corrects Travis ("the school name is IS 217, not Performing
//!   Arts") → correction, target_kind="school", target_id=<IS217>.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::Manager;

use crate::llm::ToolDef;
use crate::packs::memory::{remember, MemoryKind};
use crate::tools::{Tool, ToolContext};
use crate::AppState;

pub struct RememberConstraintTool;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    /// Pack slug this memory belongs to. "lead-to-empower", "tutoring",
    /// etc. Required so memories don't bleed across packs.
    pack_slug: String,
    /// One of: rule, preference, constraint, fact, correction.
    /// Defaults to "rule".
    #[serde(default)]
    kind: Option<String>,
    /// Optional entity scope. When set, the memory ONLY surfaces when
    /// that specific entity is in conversation context. Pass the
    /// spine entity kind ("school", "contract", "engagement",
    /// "coach", ...) and id, NOT the pack table id.
    #[serde(default)]
    target_kind: Option<String>,
    #[serde(default)]
    target_id: Option<i64>,
    /// The memory text itself. Dense — the system prompt has limited
    /// room. Example: "Never include service dates that fall outside
    /// the PO activity window for DoF-route invoices."
    content: String,
}

#[async_trait]
impl Tool for RememberConstraintTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "remember_constraint".into(),
            description: "Persist a user-stated rule, preference, constraint, fact, or \
                correction so it surfaces in future system prompts. Use this whenever the \
                user establishes a constraint or preference that should hold beyond the \
                current turn. Pack-scoped: pass `packSlug` for the relevant pack \
                (\"lead-to-empower\", \"tutoring\", ...) so memories don't bleed across \
                domains.\n\n\
                Kinds:\n\
                - rule: hard constraint (\"never include March 17 service dates\")\n\
                - preference: soft preference (\"Taylor prefers Net 30 terms\")\n\
                - constraint: operational (\"DoF invoices require PO# in subject\")\n\
                - fact: context (\"IS 217 has 2 contracts — disambiguate first\")\n\
                - correction: something Travis got wrong (\"the school's IS 217, not \
                  Performing Arts\")\n\n\
                Scope:\n\
                - Pack-wide (no target): applies any time this pack is active.\n\
                - Entity-scoped (targetKind + targetId): only surfaces when that \
                  specific entity is in current conversation context. Use spine entity \
                  refs, NOT pack table ids.\n\n\
                Dedup: identical (pack, target, content) memories collapse into a single \
                row with bumped relevance — safe to call this whenever a rule comes up; \
                won't create duplicates."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "packSlug": {
                        "type": "string",
                        "description": "Pack slug — e.g. 'lead-to-empower'."
                    },
                    "kind": {
                        "type": "string",
                        "enum": ["rule", "preference", "constraint", "fact", "correction"],
                        "description": "Memory category. Defaults to 'rule'."
                    },
                    "targetKind": {
                        "type": "string",
                        "description": "Optional spine entity kind ('school', 'contract', 'engagement', 'coach', ...) to scope this memory to a specific entity. Required when targetId is set."
                    },
                    "targetId": {
                        "type": "integer",
                        "description": "Optional spine entity id. Pair with targetKind."
                    },
                    "content": {
                        "type": "string",
                        "description": "The memory text. Be dense and specific."
                    }
                },
                "required": ["packSlug", "content"]
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: Input = serde_json::from_value(input)?;
        let state = ctx.app.state::<AppState>();
        let workspace_id = state.workspace.read().await.active_id;
        let kind = MemoryKind::from_str(p.kind.as_deref().unwrap_or("rule"));
        let id = remember(
            &state.db.pool,
            workspace_id,
            &p.pack_slug,
            kind,
            p.target_kind.as_deref(),
            p.target_id,
            &p.content,
            "chat",
            ctx.conversation_id,
        )
        .await?;
        Ok(json!({
            "ok": true,
            "memoryId": id,
            "packSlug": p.pack_slug,
            "kind": kind.as_str(),
        })
        .to_string())
    }
}
