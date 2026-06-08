use std::time::Instant;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};

use super::{
    ChatOptions, ChatResponse, ChatTurn, ChatWithToolsOptions, LlmProvider, Message, PingResult,
    Role, ToolCall, ToolChoice,
};

pub struct OllamaProvider {
    http: reqwest::Client,
    base_url: String,
    model: String,
}

impl OllamaProvider {
    pub fn new(http: reqwest::Client, base_url: String, model: String) -> Self {
        let base_url = base_url.trim_end_matches('/').to_string();
        Self { http, base_url, model }
    }
}

#[derive(Deserialize)]
struct OllamaChat {
    message: OllamaMessage,
    model: String,
    #[serde(default)]
    done_reason: Option<String>,
    prompt_eval_count: Option<u32>,
    eval_count: Option<u32>,
}
#[derive(Deserialize)]
struct OllamaMessage {
    #[serde(default)]
    content: String,
    #[serde(default)]
    tool_calls: Vec<OllamaToolCall>,
}
#[derive(Deserialize)]
struct OllamaToolCall {
    function: OllamaToolFunction,
}
#[derive(Deserialize)]
struct OllamaToolFunction {
    name: String,
    /// Ollama returns arguments as a JSON object directly (not a string).
    arguments: Value,
}

fn build_ollama_messages(messages: &[Message], system: Option<&str>) -> Vec<Value> {
    let mut msgs = Vec::new();
    if let Some(sys) = system {
        msgs.push(json!({"role": "system", "content": sys}));
    }
    for m in messages {
        match m.role {
            Role::System => msgs.push(json!({"role": "system", "content": m.content})),
            Role::User => msgs.push(json!({"role": "user", "content": m.content})),
            Role::Assistant => {
                let mut entry = json!({"role": "assistant", "content": m.content});
                if !m.tool_calls.is_empty() {
                    entry["tool_calls"] = json!(m
                        .tool_calls
                        .iter()
                        .map(|tc| json!({
                            "function": {
                                "name": tc.name,
                                "arguments": tc.input,
                            },
                        }))
                        .collect::<Vec<_>>());
                }
                msgs.push(entry);
            }
            Role::Tool => {
                msgs.push(json!({"role": "tool", "content": m.content}));
            }
        }
    }
    msgs
}

#[async_trait]
impl LlmProvider for OllamaProvider {
    fn name(&self) -> &'static str { "ollama" }
    fn model(&self) -> &str { &self.model }

    async fn ping(&self) -> anyhow::Result<PingResult> {
        let started = Instant::now();
        let url = format!("{}/api/tags", self.base_url);
        let res = self.http.get(&url).send().await;
        let latency_ms = started.elapsed().as_millis() as u64;
        match res {
            Ok(r) if r.status().is_success() => {
                Ok(PingResult { ok: true, model: self.model.clone(), latency_ms, message: None })
            }
            Ok(r) => Ok(PingResult {
                ok: false,
                model: self.model.clone(),
                latency_ms,
                message: Some(format!("ollama {} at {}", r.status().as_u16(), self.base_url)),
            }),
            Err(e) => Ok(PingResult {
                ok: false,
                model: self.model.clone(),
                latency_ms,
                message: Some(format!("could not reach ollama at {}: {e}", self.base_url)),
            }),
        }
    }

    async fn chat(&self, messages: Vec<Message>, opts: ChatOptions) -> anyhow::Result<ChatResponse> {
        let msgs = build_ollama_messages(&messages, opts.system.as_deref());

        let mut body = json!({
            "model": self.model,
            "messages": msgs,
            "stream": false,
        });
        if opts.json_mode {
            body["format"] = json!("json");
        }
        let mut options = serde_json::Map::new();
        if let Some(t) = opts.temperature {
            options.insert("temperature".into(), json!(t));
        }
        if let Some(mt) = opts.max_tokens {
            options.insert("num_predict".into(), json!(mt));
        }
        if !options.is_empty() {
            body["options"] = json!(options);
        }

        let url = format!("{}/api/chat", self.base_url);
        let resp = self.http.post(&url).json(&body).send().await?;
        let status = resp.status();
        let bytes = resp.bytes().await?;
        if !status.is_success() {
            return Err(anyhow::anyhow!(
                "ollama {}: {}",
                status.as_u16(),
                String::from_utf8_lossy(&bytes)
            ));
        }
        let parsed: OllamaChat = serde_json::from_slice(&bytes)?;
        Ok(ChatResponse {
            content: parsed.message.content,
            model: parsed.model,
            input_tokens: parsed.prompt_eval_count,
            output_tokens: parsed.eval_count,
            cache_read_tokens: None,
        })
    }

    async fn chat_with_tools(
        &self,
        messages: Vec<Message>,
        opts: ChatWithToolsOptions,
    ) -> anyhow::Result<ChatTurn> {
        let msgs = build_ollama_messages(&messages, opts.system.as_deref());

        let mut body = json!({
            "model": self.model,
            "messages": msgs,
            "stream": false,
        });

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

        // Ollama doesn't natively support tool_choice — but if the caller demands
        // a specific tool we add a strong instruction to the system prompt and
        // also enable JSON mode as a hedge for older models.
        let force_specific = matches!(&opts.tool_choice, Some(ToolChoice::Specific(_)));
        if force_specific {
            body["format"] = json!("json");
        }

        let mut options = serde_json::Map::new();
        if let Some(t) = opts.temperature {
            options.insert("temperature".into(), json!(t));
        }
        if let Some(mt) = opts.max_tokens {
            options.insert("num_predict".into(), json!(mt));
        }
        if !options.is_empty() {
            body["options"] = json!(options);
        }

        let url = format!("{}/api/chat", self.base_url);
        let resp = self.http.post(&url).json(&body).send().await?;
        let status = resp.status();
        let bytes = resp.bytes().await?;
        if !status.is_success() {
            return Err(anyhow::anyhow!(
                "ollama {}: {}",
                status.as_u16(),
                String::from_utf8_lossy(&bytes)
            ));
        }

        let parsed: OllamaChat = serde_json::from_slice(&bytes)?;

        // If the model returned native tool_calls, use them.
        let mut tool_calls: Vec<ToolCall> = parsed
            .message
            .tool_calls
            .into_iter()
            .enumerate()
            .map(|(i, tc)| ToolCall {
                // Ollama doesn't provide ids — synthesize one for our session.
                id: format!("ollama_call_{i}"),
                name: tc.function.name,
                input: tc.function.arguments,
            })
            .collect();

        // Fallback for older models that don't emit tool_calls but do emit JSON
        // matching our forced tool's schema. If the caller forced a specific
        // tool and we got plain text content that parses as JSON, synthesize
        // a tool call so callers get structured output regardless.
        if tool_calls.is_empty() && force_specific {
            if let Some(ToolChoice::Specific(name)) = &opts.tool_choice {
                let trimmed = parsed.message.content.trim();
                if let Ok(parsed_json) = serde_json::from_str::<Value>(trimmed) {
                    tool_calls.push(ToolCall {
                        id: "ollama_synth_0".into(),
                        name: name.clone(),
                        input: parsed_json,
                    });
                }
            }
        }

        let content = if tool_calls.is_empty() {
            parsed.message.content
        } else {
            // When the model used tools, surface any preamble text verbatim.
            parsed.message.content
        };

        Ok(ChatTurn {
            content,
            tool_calls,
            model: parsed.model,
            input_tokens: parsed.prompt_eval_count,
            output_tokens: parsed.eval_count,
            cache_read_tokens: None,
            stop_reason: parsed.done_reason,
            thinking_blocks: Vec::new(),
        })
    }
}
