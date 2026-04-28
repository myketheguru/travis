use std::time::Instant;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};

use super::{
    ChatOptions, ChatResponse, ChatTurn, ChatWithToolsOptions, LlmProvider, Message, PingResult,
    Role, ToolCall, ToolChoice,
};

const ENDPOINT: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";

pub struct ClaudeProvider {
    http: reqwest::Client,
    api_key: String,
    model: String,
}

impl ClaudeProvider {
    pub fn new(http: reqwest::Client, api_key: String, model: String) -> Self {
        Self { http, api_key, model }
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
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct AnthropicUsage {
    input_tokens: Option<u32>,
    output_tokens: Option<u32>,
    cache_read_input_tokens: Option<u32>,
}

#[derive(Deserialize)]
struct AnthropicError {
    error: AnthropicErrorBody,
}
#[derive(Deserialize)]
struct AnthropicErrorBody {
    message: String,
}

/// Convert our internal Message list into Anthropic's `messages` array. Tool
/// results ride as a user-role message with a `tool_result` content block;
/// assistant messages with tool_calls become content arrays of text + tool_use.
fn build_anthropic_messages(messages: &[Message]) -> Vec<Value> {
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
                    out.push(json!({
                        "role": "assistant",
                        "content": m.content,
                    }));
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
                out.push(json!({"role": "user", "content": m.content}));
            }
        }
    }
    out
}

#[async_trait]
impl LlmProvider for ClaudeProvider {
    fn name(&self) -> &'static str { "claude" }
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
            "messages": build_anthropic_messages(&messages),
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

        let resp = self.http
            .post(ENDPOINT)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = resp.status();
        let bytes = resp.bytes().await?;
        if !status.is_success() {
            let msg = serde_json::from_slice::<AnthropicError>(&bytes)
                .map(|e| e.error.message)
                .unwrap_or_else(|_| String::from_utf8_lossy(&bytes).to_string());
            return Err(anyhow::anyhow!("anthropic {}: {msg}", status.as_u16()));
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
            "messages": build_anthropic_messages(&messages),
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
            body["tools"] = json!(opts
                .tools
                .iter()
                .map(|t| json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.input_schema,
                }))
                .collect::<Vec<_>>());
        }

        if let Some(choice) = opts.tool_choice {
            body["tool_choice"] = match choice {
                ToolChoice::Auto => json!({"type": "auto"}),
                ToolChoice::Required => json!({"type": "any"}),
                ToolChoice::Specific(name) => json!({"type": "tool", "name": name}),
            };
        }

        let resp = self
            .http
            .post(ENDPOINT)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = resp.status();
        let bytes = resp.bytes().await?;
        if !status.is_success() {
            let msg = serde_json::from_slice::<AnthropicError>(&bytes)
                .map(|e| e.error.message)
                .unwrap_or_else(|_| String::from_utf8_lossy(&bytes).to_string());
            return Err(anyhow::anyhow!("anthropic {}: {msg}", status.as_u16()));
        }

        let parsed: AnthropicMessage = serde_json::from_slice(&bytes)?;

        let mut text = String::new();
        let mut calls = Vec::new();
        for block in parsed.content {
            match block {
                ContentBlock::Text { text: t } => text.push_str(&t),
                ContentBlock::ToolUse { id, name, input } => {
                    calls.push(ToolCall { id, name, input });
                }
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
            stop_reason: parsed.stop_reason,
        })
    }
}
