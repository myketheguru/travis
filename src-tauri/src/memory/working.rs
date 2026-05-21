//! Working memory (BRAIN.md Phase 4.5 #6).
//!
//! Per-conversation short-lived hypothesis store. Multi-turn reasoning
//! compounds rather than restarting from scratch every turn: when
//! Travis tentatively concludes something ("looks like Maria works at
//! PS 142, low confidence one-data-point"), the conclusion sits in
//! working memory keyed by conversation_id. The next turn's prompt
//! gets it as WORKING MEMORY block, and Travis can either firm it up
//! (upgrade to a claim) or revise it without re-deriving.
//!
//! In-process map, not a table. Lifetime = process lifetime, capped at
//! ~30 minutes per entry. Survives across turns in the same
//! conversation; lost on restart (intentional — these are
//! hypothesis-grade, not facts).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use tokio::sync::RwLock;

/// One hypothesis held in working memory. Format mirrors a claim so
/// "upgrade to claim" is a cheap structural copy.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hypothesis {
    pub topic: String,
    /// Free-form note Travis wrote to itself.
    pub note: String,
    pub confidence: String,
    /// Entity ids this hypothesis touches, for cross-referencing
    /// during retrieval.
    pub entity_ids: Vec<i64>,
    /// Instant the entry was recorded — used for TTL + age-in-prompt
    /// display. Not serialized (Instant has no Serialize impl) — the
    /// LLM sees a human age in the WORKING MEMORY block instead.
    #[serde(skip)]
    pub created_at: Instant,
}

/// TTL — entries older than this fall out on next access. 30 minutes
/// keeps a coherent multi-turn window without retaining stale
/// guesses indefinitely.
const TTL: Duration = Duration::from_secs(30 * 60);
/// Max entries per conversation. Bounded to keep prompt-size predictable.
const MAX_PER_CONV: usize = 10;

#[derive(Default)]
struct Inner {
    by_conv: HashMap<i64, Vec<Hypothesis>>,
}

#[derive(Clone, Default)]
pub struct WorkingMemory {
    inner: Arc<RwLock<Inner>>,
}

impl WorkingMemory {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add or refresh a hypothesis for this conversation. If a
    /// hypothesis with the same topic exists, overwrite — Travis
    /// can revise its own thinking turn-over-turn.
    pub async fn record(
        &self,
        conversation_id: i64,
        topic: String,
        note: String,
        confidence: String,
        entity_ids: Vec<i64>,
    ) {
        let mut g = self.inner.write().await;
        let list = g.by_conv.entry(conversation_id).or_default();
        // Prune expired first.
        list.retain(|h| h.created_at.elapsed() < TTL);
        // Replace existing topic if present.
        if let Some(pos) = list.iter().position(|h| h.topic == topic) {
            list[pos] = Hypothesis {
                topic,
                note,
                confidence,
                entity_ids,
                created_at: Instant::now(),
            };
            return;
        }
        // Otherwise push; bound by MAX_PER_CONV (drop oldest).
        if list.len() >= MAX_PER_CONV {
            list.remove(0);
        }
        list.push(Hypothesis {
            topic,
            note,
            confidence,
            entity_ids,
            created_at: Instant::now(),
        });
    }

    /// All live hypotheses for this conversation, freshest first.
    /// Drops expired entries lazily on read.
    pub async fn for_conversation(&self, conversation_id: i64) -> Vec<Hypothesis> {
        let mut g = self.inner.write().await;
        let Some(list) = g.by_conv.get_mut(&conversation_id) else {
            return Vec::new();
        };
        list.retain(|h| h.created_at.elapsed() < TTL);
        let mut out = list.clone();
        out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        out
    }

    /// Drop everything for a conversation — called when the
    /// conversation auto-closes or the user resolves it.
    pub async fn clear_conversation(&self, conversation_id: i64) {
        let mut g = self.inner.write().await;
        g.by_conv.remove(&conversation_id);
    }
}

/// Render a list of hypotheses as the WORKING MEMORY block injected
/// into the LLM's user message. Empty string when no hypotheses.
pub fn format_for_prompt(hs: &[Hypothesis]) -> String {
    if hs.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "WORKING MEMORY (hypotheses you wrote to yourself earlier in this conversation — revise or firm up as evidence accumulates):\n",
    );
    for h in hs {
        let age = h.created_at.elapsed().as_secs() / 60;
        out.push_str(&format!(
            "- [{conf}, {age}m ago] {topic}: {note}\n",
            conf = h.confidence,
            age = age,
            topic = h.topic,
            note = h.note,
        ));
    }
    out
}
