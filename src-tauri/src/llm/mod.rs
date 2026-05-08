use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub mod claude;
pub mod ollama;
pub mod openai;

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    #[default]
    User,
    Assistant,
    /// Used to send a tool result back to the model. Provider impls translate
    /// to the right wire format (Claude: user role with tool_result block;
    /// OpenAI: role=tool with tool_call_id; Ollama: role=tool).
    Tool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub role: Role,
    #[serde(default)]
    pub content: String,
    /// Populated for assistant messages that called tools. Empty otherwise.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    /// Set for Role::Tool messages — references the call this is responding to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl Message {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
            ..Default::default()
        }
    }

    #[allow(dead_code)]
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
            ..Default::default()
        }
    }

    #[allow(dead_code)]
    pub fn tool_result(call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: content.into(),
            tool_call_id: Some(call_id.into()),
            ..Default::default()
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatOptions {
    pub system: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    #[serde(default)]
    pub cache_system: bool,
    #[serde(default)]
    pub json_mode: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatResponse {
    pub content: String,
    pub model: String,
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
    pub cache_read_tokens: Option<u32>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PingResult {
    pub ok: bool,
    pub model: String,
    pub latency_ms: u64,
    pub message: Option<String>,
}

// ----- Tool calling / structured output types -----

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    /// JSON Schema for the tool's input. Use `serde_json::json!` to build.
    pub input_schema: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCall {
    /// Provider-assigned id (Claude/OpenAI). Ollama may not provide one — we synthesize.
    pub id: String,
    pub name: String,
    /// Raw JSON-deserialized args.
    pub input: serde_json::Value,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub enum ToolChoice {
    /// Let the model decide whether/which tool to call.
    Auto,
    /// Model MUST call at least one of the provided tools.
    Required,
    /// Model MUST call this specific tool.
    Specific(String),
}

#[derive(Clone, Debug, Default)]
pub struct ChatWithToolsOptions {
    pub system: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub cache_system: bool,
    pub tools: Vec<ToolDef>,
    pub tool_choice: Option<ToolChoice>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatTurn {
    /// Free-text content the LLM produced (may be empty when it only called tools).
    pub content: String,
    /// Tool calls the model is requesting we execute.
    pub tool_calls: Vec<ToolCall>,
    pub model: String,
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
    pub cache_read_tokens: Option<u32>,
    /// Provider-specific stop reason: "end_turn" | "tool_use" | "max_tokens" | etc.
    pub stop_reason: Option<String>,
}

#[async_trait]
pub trait LlmProvider: Send + Sync {
    #[allow(dead_code)]
    fn name(&self) -> &'static str;
    #[allow(dead_code)]
    fn model(&self) -> &str;
    async fn ping(&self) -> anyhow::Result<PingResult>;
    async fn chat(
        &self,
        messages: Vec<Message>,
        opts: ChatOptions,
    ) -> anyhow::Result<ChatResponse>;

    /// Tool-aware chat. Default impl maps to plain `chat()` — providers that
    /// implement native tool calling override this for richer behavior.
    async fn chat_with_tools(
        &self,
        messages: Vec<Message>,
        opts: ChatWithToolsOptions,
    ) -> anyhow::Result<ChatTurn> {
        let resp = self
            .chat(
                messages,
                ChatOptions {
                    system: opts.system,
                    max_tokens: opts.max_tokens,
                    temperature: opts.temperature,
                    cache_system: opts.cache_system,
                    json_mode: false,
                },
            )
            .await?;
        Ok(ChatTurn {
            content: resp.content,
            tool_calls: Vec::new(),
            model: resp.model,
            input_tokens: resp.input_tokens,
            output_tokens: resp.output_tokens,
            cache_read_tokens: resp.cache_read_tokens,
            stop_reason: None,
        })
    }
}

pub fn default_model(provider: &str) -> &'static str {
    match provider {
        "claude" => "claude-sonnet-4-6",
        "openai" => "gpt-4o",
        "ollama" => "llama3.1:8b",
        _ => "",
    }
}

/// Cheap-tier model for the provider, used by the journal ingest
/// path when the intent classifier flags a capture-style turn (no
/// retrieval, no question answering — just structure extraction).
/// None means "no cheap tier available; stick with default".
pub fn cheap_model(provider: &str) -> Option<&'static str> {
    match provider {
        "claude" => Some("claude-haiku-4-5-20251001"),
        // OpenAI has gpt-4o-mini but pricing/quality tradeoff is
        // less obvious — skip until we have telemetry to compare.
        // Ollama is single-model per install; nothing to tier.
        _ => None,
    }
}

pub fn build(
    provider: &str,
    api_key: Option<&str>,
    ollama_url: Option<&str>,
    model: Option<&str>,
    http: reqwest::Client,
) -> anyhow::Result<Box<dyn LlmProvider>> {
    let model = model.unwrap_or_else(|| default_model(provider));
    match provider {
        "claude" => {
            let key = api_key.ok_or_else(|| {
                anyhow::anyhow!("Claude API key not found in your OS keychain. Re-enter it below.")
            })?;
            Ok(Box::new(claude::ClaudeProvider::new(http, key.to_string(), model.to_string())))
        }
        "openai" => {
            let key = api_key.ok_or_else(|| {
                anyhow::anyhow!("OpenAI API key not found in your OS keychain. Re-enter it below.")
            })?;
            Ok(Box::new(openai::OpenAiProvider::new(http, key.to_string(), model.to_string())))
        }
        "ollama" => {
            let url = ollama_url.unwrap_or("http://localhost:11434");
            Ok(Box::new(ollama::OllamaProvider::new(http, url.to_string(), model.to_string())))
        }
        other => Err(anyhow::anyhow!("unknown provider: {other}")),
    }
}
