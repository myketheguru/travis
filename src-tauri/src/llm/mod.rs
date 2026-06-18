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
    /// v0.20.18 — image attachments rendered as Claude vision content
    /// blocks. Used to inject sample doc renders so the LLM has visual
    /// context, not just JSON descriptions from analyze_document_styling.
    /// Only honored on Role::User messages today.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<MessageImage>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageImage {
    /// e.g. "image/png" or "image/jpeg".
    pub mime_type: String,
    /// Base64-encoded raw image bytes (no data: URI prefix).
    pub base64_data: String,
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
    /// Cache the system prompt. Set true whenever the system prompt is
    /// stable across calls (which is almost always). The Anthropic
    /// provider attaches `cache_control: {type: "ephemeral"}` to the
    /// system block; first call writes the cache (1.25x input cost),
    /// subsequent calls within the 5min TTL read it (0.1x input cost).
    #[serde(default)]
    pub cache_system: bool,
    /// Cache the conversation history up through the last assistant
    /// turn. The new user message is the only uncached fresh input.
    /// Set true for multi-turn chat loops, false for one-shot calls
    /// (condense, verify, sub-agent) where conversation reuse is moot.
    #[serde(default)]
    pub cache_conversation: bool,
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
    /// Tokens billed at cache-read rate (10x cheaper than fresh input).
    pub cache_read_tokens: Option<u32>,
    /// Tokens billed at cache-write rate (1.25x fresh input). One-time
    /// charge per prompt-prefix that gets cached; surfaces so we can
    /// distinguish "first call of a session" from "subsequent calls."
    pub cache_write_tokens: Option<u32>,
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
    /// See ChatOptions::cache_system.
    pub cache_system: bool,
    /// Cache the tools array. Travis ships large tools blocks
    /// (planner + workflow + file tools) — caching the last tool
    /// extends the cached prefix through the whole tools array.
    pub cache_tools: bool,
    /// See ChatOptions::cache_conversation.
    pub cache_conversation: bool,
    pub tools: Vec<ToolDef>,
    pub tool_choice: Option<ToolChoice>,
    /// v0.15.2 — extended thinking. When set, the Claude provider
    /// requests `thinking: { type: "enabled", budget_tokens: N }`
    /// on the messages call. Thinking content blocks come back in
    /// the response and surface via `ChatTurn.thinking_blocks`.
    /// Other providers ignore this for now.
    pub thinking_budget: Option<u32>,
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
    pub cache_write_tokens: Option<u32>,
    /// Provider-specific stop reason: "end_turn" | "tool_use" | "max_tokens" | etc.
    pub stop_reason: Option<String>,
    /// v0.15.2 — Anthropic extended-thinking content blocks (text only,
    /// not the redacted variant). Empty for providers / requests that
    /// don't use extended thinking. The chat surface can render these
    /// as inline reasoning under the active manager step.
    #[serde(default)]
    pub thinking_blocks: Vec<String>,
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

    /// Vision-capable PDF extraction. Send a PDF's raw bytes + an
    /// extraction prompt; the provider OCRs / parses internally and
    /// returns the model's text response (typically JSON when the
    /// prompt asks for it). Providers that don't support document
    /// inputs return an error; the caller decides whether to fall
    /// back or fail.
    ///
    /// Default impl: error. Override per-provider when supported.
    /// Used by `documents::extract` when text-layer extraction returns
    /// nothing (scanned-image PDFs).
    async fn extract_pdf(
        &self,
        _bytes: &[u8],
        _system_prompt: &str,
        _max_tokens: Option<u32>,
    ) -> anyhow::Result<String> {
        anyhow::bail!(
            "vision-based PDF extraction not supported by this provider; \
             try switching to Claude in Settings for scanned-image PDFs"
        )
    }

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
                    cache_conversation: opts.cache_conversation,
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
            cache_write_tokens: resp.cache_write_tokens,
            stop_reason: None,
            thinking_blocks: Vec::new(),
        })
    }
}

pub fn default_model(provider: &str) -> &'static str {
    match provider {
        "travis_cloud" | "claude" => "claude-sonnet-4-6",
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
        "travis_cloud" | "claude" => Some("claude-haiku-4-5-20251001"),
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
        // v0.20.2 — Travis Cloud: managed Anthropic backend. Uses the
        // build-time embedded key (TRAVIS_CLOUD_ANTHROPIC_KEY) so new
        // users have a working LLM out of the box without configuring
        // anything. Falls through to a clear error when the build
        // wasn't compiled with the key.
        "travis_cloud" => {
            let key = travis_cloud_key()
                .ok_or_else(|| anyhow::anyhow!(
                    "This build of Travis wasn't compiled with a Travis Cloud key. \
                     Open Settings → LLM Provider and switch to 'Use my own LLM', \
                     OR install an official release build.",
                ))?;
            let cloud_model = travis_cloud_model().unwrap_or_else(|| default_model("claude").to_string());
            Ok(Box::new(claude::ClaudeProvider::new(
                http,
                key.to_string(),
                cloud_model,
            )))
        }
        "claude" => {
            let key = api_key.ok_or_else(|| missing_key_error("Claude", "claude"))?;
            Ok(Box::new(claude::ClaudeProvider::new(http, key.to_string(), model.to_string())))
        }
        "openai" => {
            let key = api_key.ok_or_else(|| missing_key_error("OpenAI", "openai"))?;
            Ok(Box::new(openai::OpenAiProvider::new(http, key.to_string(), model.to_string())))
        }
        "ollama" => {
            let url = ollama_url.unwrap_or("http://localhost:11434");
            Ok(Box::new(ollama::OllamaProvider::new(http, url.to_string(), model.to_string())))
        }
        other => Err(anyhow::anyhow!("unknown provider: {other}")),
    }
}

/// v0.20.2 — build-time Travis Cloud key. Returns None when the build
/// didn't ship with one (local dev, third-party fork without the
/// secret). The Cloud provider falls through to a clear error in that
/// case so the user knows to switch to "own LLM" mode.
pub fn travis_cloud_key() -> Option<&'static str> {
    let raw = option_env!("TRAVIS_CLOUD_ANTHROPIC_KEY")?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// Optional override for the Travis Cloud default model. If unset,
/// falls back to default_model("claude") so we ship the latest
/// recommended Anthropic model.
pub fn travis_cloud_model() -> Option<String> {
    let raw = option_env!("TRAVIS_CLOUD_MODEL")?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// True iff this build shipped with a Travis Cloud key — i.e. the
/// Cloud provider option will actually work without user setup.
pub fn travis_cloud_available() -> bool {
    travis_cloud_key().is_some()
}

/// Build a descriptive error explaining WHY the key is missing — was
/// it never stored, is the OS keychain returning an empty entry, or
/// is the OS itself misbehaving. The user sees this verbatim in chat,
/// so naming the actual failure mode makes self-diagnosis possible
/// instead of falling into a "I re-entered the key 5 times and it
/// still doesn't work" loop.
fn missing_key_error(label: &str, provider: &str) -> anyhow::Error {
    use crate::secrets::{lookup_api_key, KeyLookup};
    match lookup_api_key(provider) {
        KeyLookup::FromCache(_) | KeyLookup::FromKeychain(_) => {
            anyhow::anyhow!(
                "{label} API key is set, but the LLM call was made without it — internal wiring bug. Restart Travis and try again."
            )
        }
        KeyLookup::NoEntry => anyhow::anyhow!(
            "{label} API key not found in your OS keychain. Open Settings → LLM Provider and enter your key."
        ),
        KeyLookup::EmptyEntry => anyhow::anyhow!(
            "{label} API key in your OS keychain is empty. Open Settings → LLM Provider and re-enter the key."
        ),
        KeyLookup::KeychainError(msg) => anyhow::anyhow!(
            "{label} API key — OS keychain returned an error: {msg}. The key may have been stored under a different OS account, or the keychain access is locked. Open Settings → LLM Provider and re-enter the key; if it still fails, this is a Windows/macOS credential-storage issue."
        ),
    }
}
