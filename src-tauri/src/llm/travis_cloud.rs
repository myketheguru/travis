//! Travis Cloud LLM provider — v2 Phase 3.
//!
//! Sends Anthropic-shaped chat requests to `api.usetravis.com/llm/chat`,
//! authenticated with the user's session JWT from the keychain. The
//! cloud proxies to Anthropic on our managed account so the user never
//! handles an API key.
//!
//! Wire-compatible with [`crate::llm::claude::ClaudeProvider`] —
//! identical request body shape, identical response shape — because
//! the cloud's `/llm/chat` is a transparent passthrough for the body
//! (it just adds metering + tier policy + cache headers around it).
//!
//! Users who hit 429 on this provider have exhausted their tier's
//! daily cap; the desktop surfaces an upgrade-prompt rather than
//! falling back to a different provider silently.

use std::time::Instant;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::cloud::{self, CLOUD_BASE};

use super::{
    ChatOptions, ChatResponse, ChatTurn, ChatWithToolsOptions, LlmProvider, Message, PingResult,
    Role, ToolCall, ToolChoice,
};

const ENDPOINT_SUFFIX: &str = "/llm/chat";

pub struct TravisCloudProvider {
    http: reqwest::Client,
    model: String,
}

impl TravisCloudProvider {
    pub fn new(http: reqwest::Client, model: String) -> Self {
        Self { http, model }
    }

    fn endpoint() -> String {
        format!("{CLOUD_BASE}{ENDPOINT_SUFFIX}")
    }

    fn auth_header() -> anyhow::Result<String> {
        let jwt = cloud::read_jwt()
            .ok_or_else(|| anyhow::anyhow!("Travis Cloud requires sign-in. Open Settings to sign in with Google."))?;
        Ok(format!("Bearer {jwt}"))
    }
}

#[derive(Deserialize)]
struct AnthropicMessage {
    content: Vec<ContentBlock>,
    model: String,
    #[serde(default)]
    stop_reason: Option<String>,
    usage: AnthropicUsage,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse { id: String, name: String, input: Value },
    #[serde(rename = "thinking")]
    Thinking { thinking: String },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct AnthropicUsage {
    input_tokens: Option<u32>,
    output_tokens: Option<u32>,
    cache_read_input_tokens: Option<u32>,
    cache_creation_input_tokens: Option<u32>,
}

#[derive(Deserialize)]
struct AnthropicErrorBody {
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    tier: Option<String>,
    #[serde(default, rename = "allowedModels")]
    allowed_models: Option<Vec<String>>,
    /// Phase 2 quality-teaser: which lane is blocked + remaining
    /// fallback lanes the user could switch to. The desktop renders
    /// a modal with these options when the chat layer surfaces the
    /// 429.
    #[serde(default)]
    lane: Option<String>,
    #[serde(default)]
    fallbacks: Option<Vec<FallbackLane>>,
    #[serde(default, rename = "canEnableOverage")]
    can_enable_overage: Option<bool>,
    /// v0.26.2 — when the cloud LLM route returns
    /// `{error:"upstream LLM error", status, body}`, `status` is the
    /// upstream Anthropic HTTP status and `body` is the raw text of
    /// Anthropic's error response. Preserved here so explain_error
    /// can surface it instead of the generic wrapper.
    #[serde(default)]
    status: Option<u16>,
    #[serde(default)]
    body: Option<String>,
    /// v0.27 — cloud's new sanitized error envelope for infra failures.
    /// When `error == "temporary_issue"`, `incident_id` carries the
    /// short slug ("inc_a3b7") the user can reference in support.
    /// The raw upstream detail lives server-side; the desktop shows a
    /// calm, non-technical message.
    #[serde(default, rename = "incident_id")]
    incident_id: Option<String>,
}

#[derive(serde::Deserialize, Debug)]
struct FallbackLane {
    lane: String,
    remaining_calls: i64,
    remaining_cents: i64,
}

fn build_messages(messages: &[Message], cache_conversation: bool) -> Vec<Value> {
    // Mirror of claude::build_anthropic_messages, kept local to keep
    // the providers independent.
    let mut out = Vec::new();
    for m in messages {
        match m.role {
            Role::System => continue,
            Role::Tool => {
                let call_id = m.tool_call_id.clone().unwrap_or_default();
                out.push(json!({
                    "role": "user",
                    "content": [{
                        "type": "tool_result",
                        "tool_use_id": call_id,
                        "content": m.content,
                    }],
                }));
            }
            Role::Assistant => {
                if m.tool_calls.is_empty() {
                    out.push(json!({"role": "assistant", "content": m.content}));
                } else {
                    let mut blocks: Vec<Value> = Vec::new();
                    if !m.content.is_empty() {
                        blocks.push(json!({"type": "text", "text": m.content}));
                    }
                    for tc in &m.tool_calls {
                        blocks.push(json!({
                            "type": "tool_use",
                            "id": tc.id,
                            "name": tc.name,
                            "input": tc.input,
                        }));
                    }
                    out.push(json!({"role": "assistant", "content": blocks}));
                }
            }
            Role::User => {
                if m.images.is_empty() {
                    out.push(json!({"role": "user", "content": m.content}));
                } else {
                    let mut blocks: Vec<Value> = Vec::new();
                    for img in &m.images {
                        blocks.push(json!({
                            "type": "image",
                            "source": {
                                "type": "base64",
                                "media_type": img.mime_type,
                                "data": img.base64_data,
                            },
                        }));
                    }
                    if !m.content.is_empty() {
                        blocks.push(json!({"type": "text", "text": m.content}));
                    }
                    out.push(json!({"role": "user", "content": blocks}));
                }
            }
        }
    }
    if cache_conversation {
        if let Some(idx) = out.iter().rposition(|m| m["role"] == "assistant") {
            mark_block_cache_breakpoint(&mut out[idx]);
        }
    }
    out
}

fn mark_block_cache_breakpoint(msg: &mut Value) {
    let content = match msg.get_mut("content") {
        Some(c) => c,
        None => return,
    };
    if let Some(s) = content.as_str() {
        let text = s.to_string();
        *content = json!([{
            "type": "text",
            "text": text,
            "cache_control": {"type": "ephemeral"},
        }]);
        return;
    }
    if let Some(arr) = content.as_array_mut() {
        if let Some(last) = arr.last_mut() {
            if let Some(obj) = last.as_object_mut() {
                obj.insert("cache_control".to_string(), json!({"type": "ephemeral"}));
            }
        }
    }
}

fn explain_error(status: u16, bytes: &[u8]) -> String {
    let parsed = serde_json::from_slice::<AnthropicErrorBody>(bytes).ok();
    // v0.27 — check the sanitized-envelope case FIRST. When the cloud
    // decides an incident is user-invisible (infra failure, upstream
    // rejection), we NEVER surface raw upstream text. The user sees a
    // calm sentence + the incident slug for support reference.
    if let Some(b) = parsed.as_ref() {
        if b.error.as_deref() == Some("temporary_issue") {
            return match b.incident_id.as_deref() {
                Some(id) => format!(
                    "Travis is having a moment. We're on it. ({id})"
                ),
                None => "Travis is having a moment. We're on it.".to_string(),
            };
        }
    }
    match parsed {
        Some(b) => match b.code.as_deref() {
            Some("model_not_allowed_for_tier") => {
                let allowed = b
                    .allowed_models
                    .as_ref()
                    .map(|v| v.join(", "))
                    .unwrap_or_else(|| "none".to_string());
                format!(
                    "your {} plan doesn't include this model. Available: {}. Upgrade in Settings → Account.",
                    b.tier.as_deref().unwrap_or("current"),
                    allowed,
                )
            }
            Some("rate_limit_calls") | Some("rate_limit_cost") => {
                "you've hit today's usage cap. Resets at midnight UTC, or upgrade for more.".to_string()
            }
            Some("quality_cap_hit") => {
                // Quality lane is out of budget. Build an actionable
                // message that names the next-best lane (cheap reasoning)
                // and, for paid users, mentions pay-as-you-go.
                let has_cheap = b
                    .fallbacks
                    .as_ref()
                    .map(|f| f.iter().any(|l| l.lane == "default"))
                    .unwrap_or(false);
                let overage = b.can_enable_overage.unwrap_or(false);
                let tier = b.tier.as_deref().unwrap_or("your");
                if overage {
                    if has_cheap {
                        format!(
                            "Daily quality budget is out. Enable pay-as-you-go in Settings to keep using top-tier reasoning, or switch to cheap reasoning to keep working today free of extra charge."
                        )
                    } else {
                        "Daily quality budget is out. Enable pay-as-you-go in Settings to keep going.".to_string()
                    }
                } else if has_cheap {
                    format!(
                        "Your {tier} plan's daily quality budget is out for today. You can switch to cheap reasoning to keep working — it's not as sharp, but it'll get you through. Or upgrade to Pro for ~20× more quality budget."
                    )
                } else {
                    format!(
                        "Your {tier} plan's daily budget is out. Travis resets at midnight UTC. Upgrade to Pro for more."
                    )
                }
            }
            Some("lane_cap_hit") => {
                let lane = b.lane.as_deref().unwrap_or("this");
                format!("Daily {lane} lane budget is out. Switching lane or coming back tomorrow.")
            }
            _ => {
                // v0.26.4 — 'upstream LLM error' means the cloud got a
                // non-2xx from an upstream. Could be Anthropic directly
                // OR Cloudflare AI Gateway (which sits in front). Parse
                // the body and label the source distinctly.
                if b.error.as_deref() == Some("upstream LLM error") {
                    let upstream_status = b.status.unwrap_or(status);
                    let raw = b.body.as_deref().unwrap_or("");
                    let parsed = serde_json::from_str::<serde_json::Value>(raw).ok();

                    // AI Gateway error shape: { name: "AiGatewayError",
                    //   error: [{code, message}], internalCode, ... }
                    let is_ai_gateway = parsed
                        .as_ref()
                        .and_then(|v| v.get("name").and_then(|n| n.as_str()))
                        == Some("AiGatewayError");
                    if is_ai_gateway {
                        let internal = parsed
                            .as_ref()
                            .and_then(|v| v.get("internalCode"))
                            .and_then(|c| c.as_i64());
                        let msg = parsed
                            .as_ref()
                            .and_then(|v| v.get("message"))
                            .and_then(|m| m.as_str())
                            .unwrap_or("");
                        return match internal {
                            Some(code) => format!(
                                "AI Gateway {upstream_status} (code {code}): {msg}"
                            ),
                            None => format!("AI Gateway {upstream_status}: {msg}"),
                        };
                    }

                    // Anthropic error shape: { type:"error",
                    //   error:{ type:"...", message:"..." } }
                    let detail = parsed
                        .as_ref()
                        .and_then(|v| {
                            let msg = v
                                .get("error")
                                .and_then(|e| e.get("message"))
                                .and_then(|m| m.as_str())
                                .map(String::from);
                            let ty = v
                                .get("error")
                                .and_then(|e| e.get("type"))
                                .and_then(|t| t.as_str())
                                .map(String::from);
                            match (ty, msg) {
                                (Some(t), Some(m)) => Some(format!("{t}: {m}")),
                                (Some(t), None) => Some(t),
                                (None, Some(m)) => Some(m),
                                _ => None,
                            }
                        })
                        .unwrap_or_else(|| {
                            let trimmed: String = raw.chars().take(240).collect();
                            trimmed
                        });
                    return format!("Anthropic {upstream_status}: {detail}");
                }
                b.error
                    .unwrap_or_else(|| format!("travis cloud {status}"))
            }
        },
        None => format!(
            "travis cloud {status}: {}",
            String::from_utf8_lossy(bytes)
        ),
    }
}

#[async_trait]
impl LlmProvider for TravisCloudProvider {
    fn name(&self) -> &'static str { "travis_cloud" }
    fn model(&self) -> &str { &self.model }

    async fn ping(&self) -> anyhow::Result<PingResult> {
        let started = Instant::now();
        let res = self.chat(
            vec![Message::user("ping")],
            ChatOptions { max_tokens: Some(8), ..Default::default() },
        ).await;
        let latency_ms = started.elapsed().as_millis() as u64;
        match res {
            Ok(_) => Ok(PingResult { ok: true, model: self.model.clone(), latency_ms, message: None }),
            Err(e) => Ok(PingResult { ok: false, model: self.model.clone(), latency_ms, message: Some(e.to_string()) }),
        }
    }

    async fn chat(&self, messages: Vec<Message>, opts: ChatOptions) -> anyhow::Result<ChatResponse> {
        let mut body = json!({
            "model": self.model,
            "max_tokens": opts.max_tokens.unwrap_or(1024),
            "messages": build_messages(&messages, opts.cache_conversation),
        });
        if let Some(t) = opts.temperature {
            body["temperature"] = json!(t);
        }
        if let Some(sys) = opts.system.as_deref() {
            if opts.cache_system {
                body["system"] = json!([{
                    "type": "text",
                    "text": sys,
                    "cache_control": {"type": "ephemeral"},
                }]);
            } else {
                body["system"] = json!(sys);
            }
        }

        let resp = self
            .http
            .post(Self::endpoint())
            .header("authorization", Self::auth_header()?)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;
        let status = resp.status();
        let bytes = resp.bytes().await?;
        if !status.is_success() {
            return Err(anyhow::anyhow!(explain_error(status.as_u16(), &bytes)));
        }

        let parsed: AnthropicMessage = serde_json::from_slice(&bytes)?;
        let content = parsed.content.into_iter().filter_map(|b| match b {
            ContentBlock::Text { text } => Some(text),
            _ => None,
        }).collect::<Vec<_>>().join("");

        Ok(ChatResponse {
            content,
            model: parsed.model,
            input_tokens: parsed.usage.input_tokens,
            output_tokens: parsed.usage.output_tokens,
            cache_read_tokens: parsed.usage.cache_read_input_tokens,
            cache_write_tokens: parsed.usage.cache_creation_input_tokens,
        })
    }

    async fn chat_with_tools(
        &self,
        messages: Vec<Message>,
        opts: ChatWithToolsOptions,
    ) -> anyhow::Result<ChatTurn> {
        let mut body = json!({
            "model": self.model,
            "max_tokens": opts.max_tokens.unwrap_or(1024),
            "messages": build_messages(&messages, opts.cache_conversation),
        });
        if let Some(t) = opts.temperature {
            body["temperature"] = json!(t);
        }
        if let Some(sys) = opts.system.as_deref() {
            if opts.cache_system {
                body["system"] = json!([{
                    "type": "text",
                    "text": sys,
                    "cache_control": {"type": "ephemeral"},
                }]);
            } else {
                body["system"] = json!(sys);
            }
        }
        if !opts.tools.is_empty() {
            let mut tools_arr: Vec<Value> = opts.tools.iter().map(|t| json!({
                "name": t.name,
                "description": t.description,
                "input_schema": t.input_schema,
            })).collect();
            if opts.cache_tools {
                if let Some(last) = tools_arr.last_mut() {
                    if let Some(obj) = last.as_object_mut() {
                        obj.insert("cache_control".to_string(), json!({"type": "ephemeral"}));
                    }
                }
            }
            body["tools"] = json!(tools_arr);
        }

        let thinking_enabled = opts.thinking_budget.is_some();
        if let Some(budget) = opts.thinking_budget {
            body["thinking"] = json!({"type": "enabled", "budget_tokens": budget});
            body.as_object_mut().and_then(|o| o.remove("temperature"));
        }
        if let Some(choice) = opts.tool_choice {
            let resolved = if thinking_enabled {
                match &choice {
                    ToolChoice::Specific(_) | ToolChoice::Required => json!({"type": "auto"}),
                    ToolChoice::Auto => json!({"type": "auto"}),
                }
            } else {
                match choice {
                    ToolChoice::Auto => json!({"type": "auto"}),
                    ToolChoice::Required => json!({"type": "any"}),
                    ToolChoice::Specific(name) => json!({"type": "tool", "name": name}),
                }
            };
            body["tool_choice"] = resolved;
        }

        let resp = self
            .http
            .post(Self::endpoint())
            .header("authorization", Self::auth_header()?)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;
        let status = resp.status();
        let bytes = resp.bytes().await?;
        if !status.is_success() {
            return Err(anyhow::anyhow!(explain_error(status.as_u16(), &bytes)));
        }

        let parsed: AnthropicMessage = serde_json::from_slice(&bytes)?;
        let mut text = String::new();
        let mut calls = Vec::new();
        let mut thinking_blocks: Vec<String> = Vec::new();
        for block in parsed.content {
            match block {
                ContentBlock::Text { text: t } => text.push_str(&t),
                ContentBlock::ToolUse { id, name, input } => {
                    calls.push(ToolCall { id, name, input });
                }
                ContentBlock::Thinking { thinking } => thinking_blocks.push(thinking),
                ContentBlock::Other => {}
            }
        }
        Ok(ChatTurn {
            content: text,
            tool_calls: calls,
            model: parsed.model,
            input_tokens: parsed.usage.input_tokens,
            output_tokens: parsed.usage.output_tokens,
            cache_read_tokens: parsed.usage.cache_read_input_tokens,
            cache_write_tokens: parsed.usage.cache_creation_input_tokens,
            stop_reason: parsed.stop_reason,
            thinking_blocks,
        })
    }
}
