//! `get_ambient_transcripts` tool — v0.28.4.
//!
//! Lets the LLM pull the last N minutes of ambient-captured
//! utterances so it can answer "what was decided in the meeting?" or
//! "did they mention the deadline?". Only makes sense when the user
//! has ambient listening on (chip in the top-right of the canvas);
//! when off, this returns an empty result and the LLM should tell
//! the user to enable ambient before/during meetings for later
//! recall.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::llm::ToolDef;

use super::{Tool, ToolContext};

pub struct GetAmbientTranscriptsTool;

#[derive(Deserialize)]
struct Input {
    #[serde(default = "default_minutes")]
    minutes: i64,
    #[serde(default = "default_limit")]
    limit: i64,
}

fn default_minutes() -> i64 {
    60
}
fn default_limit() -> i64 {
    100
}

#[async_trait]
impl Tool for GetAmbientTranscriptsTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "get_ambient_transcripts".into(),
            description: "Retrieve ambient-captured speech transcripts from the last N minutes. Use when the user asks about something that happened in a recent meeting, call, or during their own thinking-out-loud — e.g. 'what did they say about Q4?', 'summarize the meeting', 'what was I working on?'. Empty result means ambient listening was not on for that window; suggest the user enable it (top-right chip on the canvas) before/during their next meeting.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "minutes": {
                        "type": "integer",
                        "description": "How far back to look, in minutes. Default 60. Max 10080 (7 days).",
                        "minimum": 1,
                        "maximum": 10080
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Max number of transcript rows to return. Default 100.",
                        "minimum": 1,
                        "maximum": 500
                    }
                }
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let params: Input = serde_json::from_value(input).unwrap_or(Input {
            minutes: default_minutes(),
            limit: default_limit(),
        });
        let rows = crate::ambient::recent(&ctx.db.pool, params.minutes, params.limit).await?;
        if rows.is_empty() {
            return Ok(format!(
                "No ambient transcripts in the last {} minutes. Ambient listening was not enabled; ask the user to turn it on (top-right 'ambient' chip on the canvas) before their next meeting so you can help recall it later.",
                params.minutes
            ));
        }
        let mut out = String::new();
        out.push_str(&format!(
            "Ambient transcripts from the last {} minutes ({} entries, newest first):\n\n",
            params.minutes,
            rows.len()
        ));
        for r in &rows {
            out.push_str(&format!("[{}] {}\n", r.occurred_at, r.text));
        }
        Ok(out)
    }
}
