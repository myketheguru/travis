//! `delegate` LLM tool — sub-agent-as-tool (OpenHands pattern).
//!
//! The user's manager loop (v0.15.1) is the outer orchestrator;
//! occasionally a complex turn benefits from spawning a *focused*
//! sub-agent on a tightly-scoped subtask without burning the parent
//! conversation's iteration budget. Per the OpenHands research
//! (v0.15.x synthesis): "sub-agent delegation as a tool. The parent
//! calls a `Delegate` tool that spawns an independent conversation
//! inheriting the workspace + model config. Result returns as an
//! observation."
//!
//! v0.16.3 ships the minimum-viable shape: the tool takes a
//! self-contained task description, runs one LLM call against it
//! with the same provider + a focused system prompt, and returns
//! the response string. Tools and full agent-loop access are NOT
//! inherited yet — that's a follow-up. Use this when:
//!   - The subtask is bounded and self-contained ("summarise this
//!     PDF in 3 sentences", "decide which date format the user
//!     wants based on these messages")
//!   - You don't want the subtask's reasoning to consume your
//!     remaining agent-loop iterations
//!   - You want a "fresh eyes" answer not coloured by the parent
//!     conversation's accumulated context
//!
//! Cost: 1 extra LLM call per delegation, on the cheap tier
//! (Haiku for Claude). Worth it for the focus.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::llm::{self, ChatOptions, Message};
use crate::llm::ToolDef;
use crate::secrets;
use crate::tools::{Tool, ToolContext};
use crate::AppState;
use tauri::Manager;

pub struct DelegateTool;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    /// One-line description of what the sub-agent should do.
    /// Surfaced to the user as the step name. e.g. "Decide the
    /// invoice date format the user expects from these examples".
    purpose: String,
    /// The actual task. Self-contained — the sub-agent doesn't see
    /// the parent conversation, so include any context it needs in
    /// here. Markdown allowed.
    task: String,
    /// Optional max tokens for the response (default 800).
    #[serde(default)]
    max_tokens: Option<u32>,
}

#[async_trait]
impl Tool for DelegateTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "delegate".into(),
            description:
                "Spawn a focused sub-agent on a self-contained subtask and return its answer. \
                 Use this when (a) the subtask is bounded — summarise this PDF, decide between \
                 these two options, draft a 3-sentence note — AND (b) you want fresh-eyes \
                 reasoning not coloured by the parent conversation's accumulated context, OR \
                 (c) you want to save your remaining agent-loop iterations for the bigger work.\n\n\
                 The sub-agent runs ONE LLM call on the cheap tier (Haiku for Claude). It does \
                 NOT inherit tools — it only reasons over the `task` text you give it. Include \
                 all context it needs in `task`.\n\n\
                 Returns the sub-agent's text response. Use it however you like in the parent \
                 turn — quote it, summarise it, base a decision on it."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "purpose": {
                        "type": "string",
                        "description": "One-line description of the subtask. Surfaced as the step name."
                    },
                    "task": {
                        "type": "string",
                        "description": "The self-contained task description for the sub-agent. Include any context it needs."
                    },
                    "maxTokens": {
                        "type": "integer",
                        "description": "Maximum response length in tokens. Default 800."
                    }
                },
                "required": ["purpose", "task"]
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: Input = serde_json::from_value(input)?;
        let state = ctx.app.state::<AppState>();

        let profile = state
            .db
            .user_profile()
            .await?
            .ok_or_else(|| anyhow::anyhow!("no user profile"))?;
        let api_key = match profile.llm_provider.as_str() {
            "claude" | "openai" => secrets::get_api_key(&profile.llm_provider),
            _ => None,
        };
        // Sub-agent uses the cheap tier. The full Sonnet is overkill
        // for a self-contained subtask; Haiku handles it fine and
        // keeps the per-delegation cost down.
        let cheap_model = llm::cheap_model(&profile.llm_provider).map(|m| m.to_string());

        let provider = llm::build(
            &profile.llm_provider,
            api_key.as_deref(),
            profile.ollama_url.as_deref(),
            cheap_model.as_deref().or(profile.model.as_deref()),
            state.http.clone(),
        )?;

        let system = "You are a sub-agent spawned by Travis (a personal AI assistant) to handle \
                      ONE focused subtask. The user did not write you directly — you're \
                      being asked by Travis itself. Read the task carefully and produce a \
                      tight, useful response. The response is consumed by Travis, not by the \
                      end user, so be terse and structured. No greetings, no caveats — just \
                      the answer.";

        let resp = provider
            .chat(
                vec![Message::user(p.task)],
                ChatOptions {
                    system: Some(system.to_string()),
                    max_tokens: Some(p.max_tokens.unwrap_or(800)),
                    temperature: Some(0.3),
                    cache_system: true,
                    cache_conversation: false,
                    json_mode: false,
                },
            )
            .await?;

        let payload = json!({
            "ok": true,
            "purpose": p.purpose,
            "response": resp.content,
            "model": resp.model,
        });
        Ok(serde_json::to_string(&payload)?)
    }
}
