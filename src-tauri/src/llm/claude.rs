use std::time::Instant;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};

use super::{
    ChatOptions, ChatResponse, ChatTurn, ChatWithToolsOptions, LlmProvider, Message, PingResult,
    Role, StreamCallback, StreamEvent, ToolCall, ToolChoice,
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
    /// v0.15.2 — Anthropic's extended-thinking content block. Returned
    /// before the final text/tool_use blocks when `thinking` is
    /// enabled on the request. The redacted variant is captured via
    /// `Other` (we don't display redactions).
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
struct AnthropicError {
    error: AnthropicErrorBody,
}
#[derive(Deserialize)]
struct AnthropicErrorBody {
    message: String,
}

/// Mark the last content block in `msg` with `cache_control: ephemeral`.
/// Anthropic caches everything up to and including the marked block — so
/// marking the last assistant turn's last block caches the entire prior
/// conversation. If the content is still a string we promote it to a
/// single-element array so we have a block to attach the marker to.
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

/// Convert our internal Message list into Anthropic's `messages` array. Tool
/// results ride as a user-role message with a `tool_result` content block;
/// assistant messages with tool_calls become content arrays of text + tool_use.
///
/// When `cache_conversation` is true and there's at least one assistant
/// message, the last assistant message's final content block is tagged
/// with `cache_control: ephemeral`. That extends the cached prefix
/// through the whole prior conversation; only the new user message at
/// the tail is fresh input the next time we call with cache_read.
fn build_anthropic_messages(messages: &[Message], cache_conversation: bool) -> Vec<Value> {
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
                // v0.20.18 — when the user message has image attachments
                // (sample doc renders, output renders), emit Claude's
                // multimodal content blocks. Image blocks come first so
                // the model sees what the text is talking about.
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

#[async_trait]
impl LlmProvider for ClaudeProvider {
    fn name(&self) -> &'static str { "claude" }
    fn model(&self) -> &str { &self.model }

    /// Vision-based PDF extraction using Claude's native `document`
    /// content block. The model OCRs each page internally and returns
    /// a text response — when the system prompt asks for JSON, that's
    /// what comes back. No PDFium / Tesseract pipeline needed.
    async fn extract_pdf(
        &self,
        bytes: &[u8],
        system_prompt: &str,
        max_tokens: Option<u32>,
    ) -> anyhow::Result<String> {
        use base64::{engine::general_purpose::STANDARD, Engine as _};
        let encoded = STANDARD.encode(bytes);

        let body = json!({
            "model": self.model,
            "max_tokens": max_tokens.unwrap_or(2_000),
            "system": [{
                "type": "text",
                "text": system_prompt,
                "cache_control": {"type": "ephemeral"},
            }],
            "messages": [{
                "role": "user",
                "content": [
                    {
                        "type": "document",
                        "source": {
                            "type": "base64",
                            "media_type": "application/pdf",
                            "data": encoded,
                        }
                    },
                    {
                        "type": "text",
                        "text": "Extract structured fields from the attached PDF per the system instructions. Return ONLY valid JSON."
                    }
                ]
            }],
            "temperature": 0.0,
        });

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
            return Err(anyhow::anyhow!(
                "anthropic vision extract {}: {msg}",
                status.as_u16()
            ));
        }

        let parsed: AnthropicMessage = serde_json::from_slice(&bytes)?;
        let text = parsed
            .content
            .into_iter()
            .filter_map(|b| match b {
                ContentBlock::Text { text } => Some(text),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("");
        Ok(text)
    }

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
            "messages": build_anthropic_messages(&messages, opts.cache_conversation),
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
            cache_write_tokens: parsed.usage.cache_creation_input_tokens,
        })
    }

    async fn chat_with_tools_streaming(
        &self,
        messages: Vec<Message>,
        opts: ChatWithToolsOptions,
        on_event: Option<StreamCallback>,
    ) -> anyhow::Result<ChatTurn> {
        // Non-streaming path when caller doesn't want live events —
        // keeps overhead identical to the old code for sub-agent /
        // condense / verify calls.
        if on_event.is_none() {
            return self.chat_with_tools(messages, opts).await;
        }
        self.chat_with_tools_stream_inner(messages, opts, on_event.unwrap())
            .await
    }

    async fn chat_with_tools(
        &self,
        messages: Vec<Message>,
        opts: ChatWithToolsOptions,
    ) -> anyhow::Result<ChatTurn> {
        let mut body = json!({
            "model": self.model,
            "max_tokens": opts.max_tokens.unwrap_or(1024),
            "messages": build_anthropic_messages(&messages, opts.cache_conversation),
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
            let mut tools_arr: Vec<Value> = opts
                .tools
                .iter()
                .map(|t| json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.input_schema,
                }))
                .collect();
            // Cache the tools block. Anthropic caches everything up to
            // and including the marked tool, so tagging the LAST tool
            // extends the cached prefix to cover the whole tools array.
            // Cheap when the tools list is stable across calls — which
            // it is for the planner / workflow loop.
            if opts.cache_tools {
                if let Some(last) = tools_arr.last_mut() {
                    if let Some(obj) = last.as_object_mut() {
                        obj.insert(
                            "cache_control".to_string(),
                            json!({"type": "ephemeral"}),
                        );
                    }
                }
            }
            body["tools"] = json!(tools_arr);
        }

        // v0.15.2 — Extended thinking. When set, Anthropic returns
        // separate `thinking` content blocks before any text/tool_use,
        // giving the model a dedicated cognitive budget. Required for
        // multi-doc reconciliation, constraint solving, forensic
        // analysis — anything the Claude.ai surface does with the
        // visible "Thinking" boxes.
        //
        // v0.15.4 — Constraints when thinking is enabled (per Anthropic):
        // 1. `temperature` must be unset or 1 — strip it here.
        // 2. `tool_choice` cannot be `Specific(...)` or `Required` —
        //    Anthropic returns 400. Forced-tool requires the model
        //    NOT to think first. Coerce to `auto` so the request is
        //    accepted; the system prompt + retry directive still steer
        //    the model toward the right tool.
        let thinking_enabled = opts.thinking_budget.is_some();
        if let Some(budget) = opts.thinking_budget {
            body["thinking"] = json!({
                "type": "enabled",
                "budget_tokens": budget,
            });
            body.as_object_mut().and_then(|o| o.remove("temperature"));
        }

        if let Some(choice) = opts.tool_choice {
            let resolved = if thinking_enabled {
                // Coerce forced tool choices to `auto` when thinking
                // is enabled (Anthropic constraint).
                match &choice {
                    ToolChoice::Specific(name) => {
                        tracing::debug!(
                            "claude: tool_choice Specific({name}) coerced to Auto because thinking is enabled"
                        );
                        json!({"type": "auto"})
                    }
                    ToolChoice::Required => {
                        tracing::debug!(
                            "claude: tool_choice Required coerced to Auto because thinking is enabled"
                        );
                        json!({"type": "auto"})
                    }
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
        let mut thinking_blocks: Vec<String> = Vec::new();
        for block in parsed.content {
            match block {
                ContentBlock::Text { text: t } => text.push_str(&t),
                ContentBlock::ToolUse { id, name, input } => {
                    calls.push(ToolCall { id, name, input });
                }
                ContentBlock::Thinking { thinking } => {
                    thinking_blocks.push(thinking);
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
            cache_write_tokens: parsed.usage.cache_creation_input_tokens,
            stop_reason: parsed.stop_reason,
            thinking_blocks,
        })
    }
}

// ============================================================
// v0.28.66 — Anthropic Messages API SSE streaming.
//
// Parses the byte stream chunk-by-chunk, fires normalized
// StreamEvents as text_delta / input_json_delta events arrive,
// and returns the final ChatTurn (identical shape to non-streaming)
// so callers that also want the aggregate get it. The frontend
// renderer keys off the events for progressive rendering; the
// journal keys off the returned ChatTurn for DB persistence.
// ============================================================

impl ClaudeProvider {
    /// Build the request body shared by streaming + non-streaming.
    /// `stream` toggles the `stream: true` field Anthropic uses to
    /// switch response format to SSE.
    fn build_stream_body(
        &self,
        messages: &[Message],
        opts: &ChatWithToolsOptions,
        stream: bool,
    ) -> Value {
        let mut body = json!({
            "model": self.model,
            "max_tokens": opts.max_tokens.unwrap_or(1024),
            "messages": build_anthropic_messages(messages, opts.cache_conversation),
        });
        if stream {
            body["stream"] = json!(true);
        }
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
            let mut tools_arr: Vec<Value> = opts
                .tools
                .iter()
                .map(|t| json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.input_schema,
                }))
                .collect();
            if opts.cache_tools {
                if let Some(last) = tools_arr.last_mut() {
                    if let Some(obj) = last.as_object_mut() {
                        obj.insert(
                            "cache_control".to_string(),
                            json!({"type": "ephemeral"}),
                        );
                    }
                }
            }
            body["tools"] = json!(tools_arr);
        }
        let thinking_enabled = opts.thinking_budget.is_some();
        if let Some(budget) = opts.thinking_budget {
            body["thinking"] = json!({
                "type": "enabled",
                "budget_tokens": budget,
            });
            body.as_object_mut().and_then(|o| o.remove("temperature"));
        }
        if let Some(choice) = &opts.tool_choice {
            let resolved = if thinking_enabled {
                match choice {
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
        body
    }

    async fn chat_with_tools_stream_inner(
        &self,
        messages: Vec<Message>,
        opts: ChatWithToolsOptions,
        on_event: StreamCallback,
    ) -> anyhow::Result<ChatTurn> {
        let body = self.build_stream_body(&messages, &opts, true);
        let resp = self
            .http
            .post(ENDPOINT)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .header("accept", "text/event-stream")
            .json(&body)
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let bytes = resp.bytes().await?;
            let msg = serde_json::from_slice::<AnthropicError>(&bytes)
                .map(|e| e.error.message)
                .unwrap_or_else(|_| String::from_utf8_lossy(&bytes).to_string());
            return Err(anyhow::anyhow!("anthropic {}: {msg}", status.as_u16()));
        }

        parse_anthropic_sse(resp, on_event).await
    }
}

/// Parse an Anthropic Messages SSE stream. Fires `on_event` per
/// text_delta / input_json_delta / tool_use start / etc. Returns
/// the aggregated ChatTurn on `message_stop`.
///
/// The parser is a simple byte-buffer state machine. SSE frames are
/// separated by `\n\n`; within each frame we look for `event: <name>`
/// and `data: <json>` lines. Multi-line data fields (rare in
/// Anthropic's output but permitted by the spec) are joined with
/// newlines per RFC.
pub(crate) async fn parse_anthropic_sse(
    mut resp: reqwest::Response,
    on_event: StreamCallback,
) -> anyhow::Result<ChatTurn> {
    let mut buf: Vec<u8> = Vec::with_capacity(8192);
    // Accumulators — mirror what non-streaming builds up from
    // parsed.content, so the returned ChatTurn is byte-identical
    // shape either way.
    let mut text = String::new();
    let mut thinking_blocks: Vec<String> = Vec::new();
    let mut model = String::new();
    let mut input_tokens: Option<u32> = None;
    let mut output_tokens: Option<u32> = None;
    let mut cache_read_tokens: Option<u32> = None;
    let mut cache_write_tokens: Option<u32> = None;
    let mut stop_reason: Option<String> = None;
    // Per-index block state — tracks in-flight tool_use blocks so
    // we can accumulate their input_json_delta into a final ToolCall.
    #[derive(Default)]
    struct BlockState {
        kind: Option<String>, // "text" | "tool_use" | "thinking"
        tool_id: Option<String>,
        tool_name: Option<String>,
        tool_json_buf: String,
        thinking_buf: String,
    }
    let mut blocks: std::collections::HashMap<u64, BlockState> = std::collections::HashMap::new();
    let mut tool_calls: Vec<ToolCall> = Vec::new();

    while let Some(chunk) = resp.chunk().await? {
        buf.extend_from_slice(&chunk);
        // Process every complete frame in the buffer. Frames end on
        // `\n\n`. Everything after the last `\n\n` stays in buf for
        // the next chunk.
        loop {
            let Some(pos) = find_frame_boundary(&buf) else { break; };
            let frame = buf[..pos].to_vec();
            buf.drain(..pos + 2); // +2 for the trailing \n\n

            let (event_name, data) = parse_sse_frame(&frame);
            if event_name.is_empty() && data.is_empty() {
                continue;
            }
            // Anthropic sends `event: ping` between events as keep-alive.
            if event_name == "ping" {
                continue;
            }
            if event_name == "error" {
                let err_msg = serde_json::from_str::<AnthropicError>(&data)
                    .map(|e| e.error.message)
                    .unwrap_or_else(|_| data.clone());
                return Err(anyhow::anyhow!("anthropic stream error: {err_msg}"));
            }
            let payload: Value = match serde_json::from_str(&data) {
                Ok(v) => v,
                Err(_) => continue,
            };
            match event_name.as_str() {
                "message_start" => {
                    if let Some(m) = payload.get("message") {
                        if let Some(s) = m.get("model").and_then(|v| v.as_str()) {
                            model = s.to_string();
                        }
                        if let Some(u) = m.get("usage") {
                            input_tokens = u.get("input_tokens").and_then(|v| v.as_u64()).map(|n| n as u32);
                            cache_read_tokens =
                                u.get("cache_read_input_tokens").and_then(|v| v.as_u64()).map(|n| n as u32);
                            cache_write_tokens =
                                u.get("cache_creation_input_tokens").and_then(|v| v.as_u64()).map(|n| n as u32);
                        }
                    }
                }
                "content_block_start" => {
                    let idx = payload.get("index").and_then(|v| v.as_u64()).unwrap_or(0);
                    let cb = payload.get("content_block").cloned().unwrap_or(Value::Null);
                    let kind = cb.get("type").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let entry = blocks.entry(idx).or_default();
                    entry.kind = Some(kind.clone());
                    if kind == "tool_use" {
                        let id = cb.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let name = cb.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        entry.tool_id = Some(id.clone());
                        entry.tool_name = Some(name.clone());
                        on_event(StreamEvent::ToolCallStart { id, name });
                    }
                }
                "content_block_delta" => {
                    let idx = payload.get("index").and_then(|v| v.as_u64()).unwrap_or(0);
                    let delta = payload.get("delta").cloned().unwrap_or(Value::Null);
                    let dtype = delta.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    let entry = blocks.entry(idx).or_default();
                    match dtype {
                        "text_delta" => {
                            if let Some(t) = delta.get("text").and_then(|v| v.as_str()) {
                                text.push_str(t);
                                on_event(StreamEvent::TextDelta(t.to_string()));
                            }
                        }
                        "thinking_delta" => {
                            if let Some(t) = delta.get("thinking").and_then(|v| v.as_str()) {
                                entry.thinking_buf.push_str(t);
                                on_event(StreamEvent::ReasoningDelta(t.to_string()));
                            }
                        }
                        "input_json_delta" => {
                            if let Some(pj) = delta.get("partial_json").and_then(|v| v.as_str()) {
                                entry.tool_json_buf.push_str(pj);
                                if let Some(id) = entry.tool_id.clone() {
                                    on_event(StreamEvent::ToolCallInputDelta {
                                        id,
                                        json_delta: pj.to_string(),
                                    });
                                }
                            }
                        }
                        _ => {}
                    }
                }
                "content_block_stop" => {
                    let idx = payload.get("index").and_then(|v| v.as_u64()).unwrap_or(0);
                    if let Some(entry) = blocks.remove(&idx) {
                        match entry.kind.as_deref() {
                            Some("tool_use") => {
                                let id = entry.tool_id.unwrap_or_default();
                                let name = entry.tool_name.unwrap_or_default();
                                // Empty args → default to {} so JSON parse never fails.
                                let raw = if entry.tool_json_buf.trim().is_empty() {
                                    "{}".to_string()
                                } else {
                                    entry.tool_json_buf
                                };
                                let input: Value = serde_json::from_str(&raw)
                                    .unwrap_or(Value::Object(serde_json::Map::new()));
                                let call = ToolCall { id, name, input };
                                on_event(StreamEvent::ToolCallComplete(call.clone()));
                                tool_calls.push(call);
                            }
                            Some("thinking") => {
                                if !entry.thinking_buf.is_empty() {
                                    thinking_blocks.push(entry.thinking_buf);
                                }
                            }
                            _ => {}
                        }
                    }
                }
                "message_delta" => {
                    if let Some(d) = payload.get("delta") {
                        if let Some(s) = d.get("stop_reason").and_then(|v| v.as_str()) {
                            stop_reason = Some(s.to_string());
                        }
                    }
                    if let Some(u) = payload.get("usage") {
                        if let Some(n) = u.get("output_tokens").and_then(|v| v.as_u64()) {
                            output_tokens = Some(n as u32);
                        }
                    }
                }
                "message_stop" => {
                    on_event(StreamEvent::Done {
                        model: model.clone(),
                        input_tokens,
                        output_tokens,
                        cache_read_tokens,
                        cache_write_tokens,
                        stop_reason: stop_reason.clone(),
                    });
                    return Ok(ChatTurn {
                        content: text,
                        tool_calls,
                        model,
                        input_tokens,
                        output_tokens,
                        cache_read_tokens,
                        cache_write_tokens,
                        stop_reason,
                        thinking_blocks,
                    });
                }
                _ => {}
            }
        }
    }

    // Stream ended without message_stop — synthesize a Done event and
    // return whatever we have (defensive; Anthropic always sends stop).
    on_event(StreamEvent::Done {
        model: model.clone(),
        input_tokens,
        output_tokens,
        cache_read_tokens,
        cache_write_tokens,
        stop_reason: stop_reason.clone(),
    });
    Ok(ChatTurn {
        content: text,
        tool_calls,
        model,
        input_tokens,
        output_tokens,
        cache_read_tokens,
        cache_write_tokens,
        stop_reason,
        thinking_blocks,
    })
}

/// Find the first `\n\n` (frame boundary) or `\r\n\r\n` in the buffer.
fn find_frame_boundary(buf: &[u8]) -> Option<usize> {
    // Look for LF LF or CR LF CR LF; return index of the first byte
    // of the boundary so the caller can slice everything before it.
    for i in 0..buf.len().saturating_sub(1) {
        if buf[i] == b'\n' && buf[i + 1] == b'\n' {
            return Some(i);
        }
        if i + 3 < buf.len()
            && buf[i] == b'\r'
            && buf[i + 1] == b'\n'
            && buf[i + 2] == b'\r'
            && buf[i + 3] == b'\n'
        {
            return Some(i);
        }
    }
    None
}

/// Parse a single SSE frame into (event_name, data_json). Returns
/// empty strings if the frame is malformed. Multi-line `data:` fields
/// are joined with newlines per SSE spec.
fn parse_sse_frame(frame: &[u8]) -> (String, String) {
    let text = String::from_utf8_lossy(frame);
    let mut event_name = String::new();
    let mut data_lines: Vec<String> = Vec::new();
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if let Some(rest) = line.strip_prefix("event:") {
            event_name = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("data:") {
            data_lines.push(rest.trim_start().to_string());
        }
    }
    (event_name, data_lines.join("\n"))
}
