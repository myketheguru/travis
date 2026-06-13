//! Condenser — collapse old events into a summary the LLM can chew
//! through without paying the full token cost. Substrate-only in
//! v0.17.0; not wired into the live agent loop yet.
//!
//! Two pieces:
//!
//! 1. [`should_condense`] — heuristic that decides whether a
//!    conversation's events have grown beyond what the model can
//!    comfortably re-read every turn. Token-budget aware.
//!
//! 2. [`condense`] — given the older events to collapse and the
//!    newer events to keep, calls a cheap-tier LLM (Haiku) with a
//!    summary prompt, then appends a [`EventKind::Condensation`] row
//!    pointing at the range that was summarised. The raw events are
//!    NEVER deleted; the LLM-visible view simply replaces the older
//!    span with the condensation.
//!
//! Future wiring: the agent loop in `journal.rs` checks
//! `should_condense` before building the message stack. When it
//! returns true, the LLM-visible projection swaps out the older
//! events for the most recent `CondensationEvent` covering them.

use sqlx::SqlitePool;

use super::{append, Event, EventKind};
use crate::llm::{ChatOptions, LlmProvider, Message, Role};

/// Rough budget where condensation becomes worth the latency tax.
/// Below this, the model can re-read every event with no problem.
/// Above this, every iteration pays for re-reading the same context
/// and the condenser starts to pay back.
const SOFT_TOKEN_BUDGET: usize = 12_000;

/// Approximate token count for an event (cheap proxy: bytes/4).
/// Real tokenizer would cost more than it saves; this is good enough
/// for the condense-or-not decision.
fn estimate_tokens(events: &[Event]) -> usize {
    events
        .iter()
        .map(|e| {
            // Empty / null events still cost a turn marker.
            let payload_size = e.payload_json.as_deref().map(str::len).unwrap_or(0);
            (payload_size / 4) + 16
        })
        .sum()
}

/// True when the event list's estimated token cost exceeds the soft
/// budget. Callers can then call [`condense`] on the older prefix.
pub fn should_condense(events: &[Event]) -> bool {
    estimate_tokens(events) > SOFT_TOKEN_BUDGET
}

/// Choose a split point — events `0..split` become condensation
/// fodder, events `split..` stay verbatim. Default: keep the last 8
/// events verbatim, condense everything before, when condensation is
/// warranted. Caller may override.
pub fn default_split(events: &[Event], keep_recent: usize) -> usize {
    events.len().saturating_sub(keep_recent.max(2))
}

/// Build the prompt that asks the cheap-tier LLM to summarise the
/// old span. The summary becomes the payload of a
/// `EventKind::Condensation` event.
fn build_summary_prompt(events: &[Event]) -> String {
    let mut out = String::new();
    out.push_str(
        "Summarise this conversation segment as compactly as possible while preserving every \
         decision, identifier (school names, contract numbers, dollar amounts, dates), and \
         unresolved question. The summary will replace the original events in the LLM's view \
         so omitted detail is lost forever. Format: dense bullet list under 400 words, no \
         filler.\n\n--- BEGIN SEGMENT ---\n",
    );
    for e in events {
        out.push_str(&format!(
            "[{}] {}\n{}\n\n",
            e.id,
            e.kind,
            e.payload_json.as_deref().unwrap_or("(no payload)"),
        ));
    }
    out.push_str("--- END SEGMENT ---");
    out
}

/// Condense the older span using a cheap-tier LLM call and append a
/// `EventKind::Condensation` event whose payload is the summary text
/// plus pointers at the first/last event ids it covers. Returns the
/// new condensation event id.
///
/// Errors are surfaced — callers (the future agent-loop integration)
/// should fall back to "skip condensation this turn" on failure
/// rather than block the user.
pub async fn condense(
    pool: &SqlitePool,
    provider: &dyn LlmProvider,
    conversation_id: i64,
    older_events: &[Event],
) -> anyhow::Result<i64> {
    if older_events.is_empty() {
        anyhow::bail!("nothing to condense");
    }
    let first = older_events.first().unwrap();
    let last = older_events.last().unwrap();

    let prompt = build_summary_prompt(older_events);
    let messages = vec![Message {
        role: Role::User,
        content: prompt,
        ..Default::default()
    }];
    let opts = ChatOptions {
        system: Some(
            "You are a condenser. Output ONLY the summary — no preamble, no closing remarks, \
             no meta-commentary. Dense bullet list under 400 words."
                .to_string(),
        ),
        max_tokens: Some(1200),
        temperature: Some(0.2),
        cache_system: false,
        json_mode: false,
    };
    let response = provider.chat(messages, opts).await?;

    let payload = serde_json::json!({
        "summary": response.content,
        "covers_first_event_id": first.id,
        "covers_last_event_id": last.id,
        "covered_event_count": older_events.len(),
    });

    append(
        pool,
        conversation_id,
        EventKind::Condensation,
        Some(&payload),
        None,
        None,
    )
    .await
}