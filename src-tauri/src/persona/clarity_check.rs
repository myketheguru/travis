//! Clarity-first enforcement layer. v2 Phase 1.5+.
//!
//! Two complementary post-checks that catch when Travis violates the
//! [[feedback-clarity-first]] doctrine:
//!
//! 1. **Ambiguity post-check** — after each LLM turn, scan the response
//!    for entity references that are ambiguous in the conversation's
//!    workspace (multiple entities sharing the same display name). If
//!    one is referenced AND the response doesn't end with a clarifying
//!    question, log + record an event. Soft signal; doesn't block.
//!
//! 2. **Correction capture** — when the user's next message contains a
//!    correction signal ("no, the other Sarah", "actually I meant Acme,
//!    not Henderson"), record a claim and surface it as a "RECENT MISS"
//!    block on the next system prompt so Travis doesn't repeat.
//!
//! These are heuristic; they will miss subtle cases. The system-prompt
//! doctrine in persona::mod and journal.rs is the primary defense; this
//! is the belt-and-suspenders that catches the ones the prompt missed.

use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

const RESPONSE_WINDOW: usize = 800;
const CORRECTION_LEADERS: &[&str] = &[
    "no", "nope", "wrong", "not", "actually", "i meant", "i mean",
    "the other", "different ", "the wrong", "you got the wrong",
];
const CLARIFY_HINTS: &[&str] = &["?", "which one", "did you mean"];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AmbiguityWarning {
    /// The shared display name that has multiple entities in scope.
    pub name: String,
    /// The candidate entities Travis could have meant. Up to 5.
    pub candidates: Vec<(String, i64)>,
    /// Whether the response already ends with a clarifying question. If
    /// true, this is informational — Travis is doing the right thing.
    pub already_clarifying: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CorrectionSignal {
    /// The entity name Travis used that the user appeared to correct.
    pub used_name: String,
    /// Snippet of the user's correction message (first 200 chars).
    pub user_snippet: String,
    /// ISO-8601 detected_at.
    pub detected_at: String,
}

/// Run the ambiguity post-check against a Travis response.
///
/// Returns `Some(warning)` if at least one entity reference in the
/// response is ambiguous in the workspace. The caller decides what to
/// do with it — log, record an event, or feed it back to the LLM as a
/// soft correction.
pub async fn scan_response(
    pool: &SqlitePool,
    workspace_id: i64,
    response: &str,
) -> anyhow::Result<Option<AmbiguityWarning>> {
    if response.trim().is_empty() {
        return Ok(None);
    }
    let lower = response.to_lowercase();
    let already_clarifying = response_ends_with_clarification(response);

    // Pull all unarchived entities in this workspace whose name appears
    // in the response. Group by normalized name; flag any normalized
    // name with ≥ 2 distinct ids.
    let rows = sqlx::query(
        "SELECT id, kind, display_name, normalized_name
         FROM entity
         WHERE workspace_id = ?1
           AND archived_at IS NULL
           AND length(display_name) >= 3
         LIMIT 1000",
    )
    .bind(workspace_id)
    .fetch_all(pool)
    .await?;

    let mut by_normalized: std::collections::HashMap<String, Vec<(String, i64, String)>> =
        std::collections::HashMap::new();
    for row in &rows {
        let id: i64 = row.get(0);
        let kind: String = row.get(1);
        let display_name: String = row.get(2);
        let normalized: String = row.get(3);
        if normalized.is_empty() {
            continue;
        }
        // Quick substring filter — must appear in the response window.
        if !lower.contains(&display_name.to_lowercase()) {
            continue;
        }
        by_normalized
            .entry(normalized)
            .or_default()
            .push((display_name, id, kind));
    }

    // Find the first ambiguous bucket (≥ 2 distinct entities).
    let mut ambiguous: Option<(String, Vec<(String, i64, String)>)> = None;
    for (normalized, group) in by_normalized.iter() {
        if group.len() >= 2 {
            ambiguous = Some((normalized.clone(), group.clone()));
            break;
        }
    }
    let Some((normalized, group)) = ambiguous else {
        return Ok(None);
    };

    let candidates: Vec<(String, i64)> = group
        .into_iter()
        .take(5)
        .map(|(name, id, kind)| (format!("{name} ({kind})"), id))
        .collect();

    Ok(Some(AmbiguityWarning {
        name: normalized,
        candidates,
        already_clarifying,
    }))
}

/// Heuristic: does the response end with a clarifying question? If yes,
/// the warning is informational — Travis is asking, exactly as the
/// doctrine prescribes. If no, the LLM made a confident guess instead.
fn response_ends_with_clarification(response: &str) -> bool {
    let tail_start = response.len().saturating_sub(RESPONSE_WINDOW);
    let tail = response[tail_start..].to_lowercase();
    CLARIFY_HINTS.iter().any(|hint| tail.contains(hint))
}

/// Detect whether the user's current message corrects an entity choice
/// from the immediately-preceding assistant message. Best-effort
/// heuristic; misses subtle cases.
///
/// Strategy:
///   1. The user message must lead with a correction marker
///      ("no", "actually", "the other", "I meant", etc.).
///   2. The prior assistant message must contain at least one entity
///      reference (proper-noun-shaped token, capitalized in
///      `prev_assistant`).
///   3. The user message either names a different entity, OR uses a
///      direct pronoun reference ("the other one") that lets us infer
///      it's pointing away from the one we used.
pub fn detect_correction(prev_assistant: &str, current_user: &str) -> Option<CorrectionSignal> {
    let user_lower = current_user.trim().to_lowercase();
    if user_lower.is_empty() {
        return None;
    }
    let starts_with_correction = CORRECTION_LEADERS
        .iter()
        .any(|leader| user_lower.starts_with(leader));
    if !starts_with_correction {
        return None;
    }

    // Pull out the first capitalized-proper-noun-looking token from the
    // assistant's message — that's our best bet for "the name Travis
    // used." Strip punctuation; require at least 2 chars; reject common
    // words.
    let used = first_proper_noun(prev_assistant)?;
    Some(CorrectionSignal {
        used_name: used,
        user_snippet: current_user.chars().take(200).collect(),
        detected_at: chrono::Utc::now().to_rfc3339(),
    })
}

fn first_proper_noun(text: &str) -> Option<String> {
    // Skip common sentence-starting capitalized words. Not a real NER —
    // just enough to avoid stupid false positives like "Got it." showing
    // up as a "proper noun."
    const STOP: &[&str] = &[
        "The", "I", "It", "We", "You", "They", "He", "She", "There", "This",
        "That", "Here", "Yes", "No", "OK", "Okay", "Got", "Done", "Yeah",
        "Sure", "And", "But", "Or", "If", "When", "While", "For",
    ];
    let stop_set: std::collections::HashSet<&str> = STOP.iter().copied().collect();
    text.split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|w| w.len() >= 2)
        .filter(|w| {
            let first = w.chars().next().map(|c| c.is_uppercase()).unwrap_or(false);
            first && !stop_set.contains(*w)
        })
        .next()
        .map(|s| s.to_string())
}

/// Format a list of recent corrections as a system-prompt block. Empty
/// string when there are no corrections — caller can splat it in
/// unconditionally.
pub fn format_recent_miss(corrections: &[CorrectionSignal]) -> String {
    if corrections.is_empty() {
        return String::new();
    }
    let mut s = String::new();
    s.push_str("RECENT MISS (a user-corrected guess from earlier in this conversation — don't repeat the same mistake):\n");
    for c in corrections.iter().take(3) {
        s.push_str(&format!(
            "- You said \"{}\"; user corrected: \"{}\"\n",
            c.used_name, c.user_snippet
        ));
    }
    s.push_str("Apply the correction for the rest of this turn. If you're not sure which entity the user means now, ASK.\n");
    s
}
