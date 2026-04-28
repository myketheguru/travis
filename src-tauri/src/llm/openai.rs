use std::time::Instant;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use super::{
    ChatOptions, ChatResponse, ChatTurn, ChatWithToolsOptions, LlmProvider, Message, PingResult,
    Role, ToolCall, ToolChoice,
};

const ENDPOINT: &str = "https://api.openai.com/v1/chat/completions";

pub struct OpenAiProvider {
    http: reqwest::Client,
    api_key: String,
    model: String,
}

impl OpenAiProvider {
    pub fn new(http: reqwest::Client, api_key: String, model: String) -> Self {
        Self { http, api_key, model }
    }
}

#[derive(Deserialize)]
struct OpenAiChat {
    choices: Vec<Choice>,
    model: String,
    usage: Option<OpenAiUsage>,
}
#[derive(Deserialize)]
struct Choice {
    message: ChoiceMessage,
    #[serde(default)]
    finish_reason: Option<String>,
}
#[derive(Deserialize)]
struct ChoiceMessage {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<OpenAiToolCall>,
}
#[derive(Deserialize)]
struct OpenAiToolCall {
    id: String,
    function: OpenAiToolFunction,
}
#[derive(Deserialize)]
struct OpenAiToolFunction {
    name: String,
    /// JSON-string of arguments (per OpenAI spec).
    arguments: String,
}
#[derive(Deserialize)]
struct OpenAiUsage {
    prompt_tokens: Option<u32>,
    completion_tokens: Option<u32>,
    prompt_tokens_details: Option<OpenAiPromptDetails>,
}
#[derive(Deserialize)]
struct OpenAiPromptDetails {
    cached_tokens: Option<u32>,
}

#[derive(Deserialize)]
struct OpenAiError {
    error: OpenAiErrorBody,
}
#[derive(Deserialize)]
struct OpenAiErrorBody {
    message: String,
}

fn build_openai_messages(messages: &[Message], system: Option<&str>) -> Vec<serde_json::Value> {
    let mut msgs: Vec<serde_json::Value> = Vec::new();
    if let Some(sys) = system {
        msgs.push(json!({"role": "system", "content": sys}));
    }
    for m in messages {
        match m.role {
            Role::System => {
                msgs.push(json!({"role": "system", "content": m.content}));
            }
            Role::User => {
                msgs.push(json!({"role": "user", "content": m.content}));
            }
            Role::Assistant => {
                let mut entry = json!({"role": "assistant", "content": m.content});
                if !m.tool_calls.is_empty() {
                    entry["tool_calls"] = json!(m
                        .tool_calls
                        .iter()
                        .map(|tc| json!({
                            "id": tc.id,
                            "type": "function",
                            "function": {
                                "name": tc.name,
                                "arguments": tc.input.to_string(),
                            },
                        }))
                        .collect::<Vec<_>>());
                }
                msgs.push(entry);
            }
            Role::Tool => {
                let call_id = m.tool_call_id.clone().unwrap_or_default();
                msgs.push(json!({
                    "role": "tool",
                    "tool_call_id": call_id,
                    "content": m.content,
                }));
            }
        }
    }
    msgs
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    fn name(&self) -> &'static str { "openai" }
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
        let msgs = build_openai_messages(&messages, opts.system.as_deref());
        let mut body = json!({
            "model": self.model,
            "messages": msgs,
        });
        if let Some(t) = opts.temperature {
            body["temperature"] = json!(t);
        }
        if let Some(mt) = opts.max_tokens {
            body["max_tokens"] = json!(mt);
        }
        if opts.json_mode {
            body["response_format"] = json!({"type": "json_object"});
        }

        let resp = self.http
            .post(ENDPOINT)
            .bearer_auth(&self.api_key)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = resp.status();
        let bytes = resp.bytes().await?;
        if !status.is_success() {
            let msg = serde_json::from_slice::<OpenAiError>(&bytes)
                .map(|e| e.error.message)
                .unwrap_or_else(|_| String::from_utf8_lossy(&bytes).to_string());
            return Err(anyhow::anyhow!("openai {}: {msg}", status.as_u16()));
        }

        let parsed: OpenAiChat = serde_json::from_slice(&bytes)?;
        let content = parsed.choices.into_iter()
            .next()
            .and_then(|c| c.message.content)
            .unwrap_or_default();

        let cache_read = parsed.usage.as_ref()
            .and_then(|u| u.prompt_tokens_details.as_ref())
            .and_then(|d| d.cached_tokens);

        Ok(ChatResponse {
            content,
            model: parsed.model,
            input_tokens: parsed.usage.as_ref().and_then(|u| u.prompt_tokens),
            output_tokens: parsed.usage.as_ref().and_then(|u| u.completion_tokens),
            cache_read_tokens: cache_read,
        })
    }

    async fn chat_with_tools(
        &self,
        messages: Vec<Message>,
        opts: ChatWithToolsOptions,
    ) -> anyhow::Result<ChatTurn> {
        let msgs = build_openai_messages(&messages, opts.system.as_deref());
        let mut body = json!({
            "model": self.model,
            "messages": msgs,
        });
        if let Some(t) = opts.temperature {
            body["temperature"] = json!(t);
        }
        if let Some(mt) = opts.max_tokens {
            body["max_tokens"] = json!(mt);
        }

        if !opts.tools.is_empty() {
            body["tools"] = json!(opts
                .tools
                .iter()
                .map(|t| json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.input_schema,
                    },
                }))
                .collect::<Vec<_>>());
        }

        if let Some(choice) = opts.tool_choice {
            body["tool_choice"] = match choice {
                ToolChoice::Auto => json!("auto"),
                ToolChoice::Required => json!("required"),
                ToolChoice::Specific(name) => json!({
                    "type": "function",
                    "function": {"name": name},
                }),
            };
        }

        let resp = self
            .http
            .post(ENDPOINT)
            .bearer_auth(&self.api_key)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = resp.status();
        let bytes = resp.bytes().await?;
        if !status.is_success() {
            let msg = serde_json::from_slice::<OpenAiError>(&bytes)
                .map(|e| e.error.message)
                .unwrap_or_else(|_| String::from_utf8_lossy(&bytes).to_string());
            return Err(anyhow::anyhow!("openai {}: {msg}", status.as_u16()));
        }

        let parsed: OpenAiChat = serde_json::from_slice(&bytes)?;
        let mut content = String::new();
        let mut calls: Vec<ToolCall> = Vec::new();
        let mut stop_reason = None;
        if let Some(choice) = parsed.choices.into_iter().next() {
            stop_reason = choice.finish_reason;
            if let Some(c) = choice.message.content {
                content = c;
            }
            for tc in choice.message.tool_calls {
                let input: serde_json::Value =
                    serde_json::from_str(&tc.function.arguments).unwrap_or(serde_json::Value::Null);
                calls.push(ToolCall {
                    id: tc.id,
                    name: tc.function.name,
                    input,
                });
            }
        }

        let cache_read = parsed
            .usage
            .as_ref()
            .and_then(|u| u.prompt_tokens_details.as_ref())
            .and_then(|d| d.cached_tokens);

        Ok(ChatTurn {
            content,
            tool_calls: calls,
            model: parsed.model,
            input_tokens: parsed.usage.as_ref().and_then(|u| u.prompt_tokens),
            output_tokens: parsed.usage.as_ref().and_then(|u| u.completion_tokens),
            cache_read_tokens: cache_read,
            stop_reason,
        })
    }
}
