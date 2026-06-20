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
    /// If the used_name resolved to a workspace entity, this is its id.
    /// When present, the correction can be persisted as a claim so it
    /// survives across conversations.
    pub matched_entity_id: Option<i64>,
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
/// Async variant that resolves the extracted name against the
/// workspace entity table so the correction can later be persisted as
/// a claim. Falls back to the sync [`detect_correction_basic`] when
/// matching fails.
///
/// Strategy:
///   1. The user message must lead with a correction marker
///      ("no", "actually", "the other", "I meant", etc.).
///   2. Extract a proper-noun-shaped reference from the assistant's
///      message — single capitalized word, multi-word run, OR
///      ALL-CAPS token. See [`extract_proper_noun_runs`].
///   3. Try to match it against an active workspace entity; attach
///      `matched_entity_id` when found so persistence is trivial.
pub async fn detect_correction(
    pool: &SqlitePool,
    workspace_id: i64,
    prev_assistant: &str,
    current_user: &str,
) -> Option<CorrectionSignal> {
    let mut signal = detect_correction_basic(prev_assistant, current_user)?;
    // Best-effort entity match. If it fails (no workspace entity by
    // that name yet), the signal still flows through — caller will
    // surface it as a RECENT MISS but won't be able to persist as a
    // claim. Both are fine; the claim is the durable-across-sessions
    // bonus, not the primary mechanism.
    if let Ok(row) = sqlx::query_as::<_, (i64,)>(
        "SELECT id FROM entity
         WHERE workspace_id = ?1
           AND archived_at IS NULL
           AND (lower(display_name) = lower(?2) OR lower(normalized_name) = lower(?2))
         ORDER BY mentions_count DESC, last_seen DESC LIMIT 1",
    )
    .bind(workspace_id)
    .bind(&signal.used_name)
    .fetch_optional(pool)
    .await
    {
        signal.matched_entity_id = row.map(|r| r.0);
    }
    Some(signal)
}

/// Sync version. Returns None when no correction is detected; when one
/// is detected, `matched_entity_id` is always None — callers wanting
/// entity attribution should use [`detect_correction`] instead.
pub fn detect_correction_basic(
    prev_assistant: &str,
    current_user: &str,
) -> Option<CorrectionSignal> {
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
    let runs = extract_proper_noun_runs(prev_assistant);
    let used = runs.first().cloned()?;
    Some(CorrectionSignal {
        used_name: used,
        user_snippet: current_user.chars().take(200).collect(),
        detected_at: chrono::Utc::now().to_rfc3339(),
        matched_entity_id: None,
    })
}

/// Tokenize `text` and return contiguous runs of proper-noun-looking
/// tokens. Handles three patterns:
///   - **Multi-word capitalized:** "Henderson Trust" → one run
///   - **Single capitalized:** "Sarah" → one run
///   - **ALL-CAPS tokens:** "NEW BOARD" → one run
///
/// Stop-words (sentence starters, common short capitalized words like
/// "The", "I", "Got") never start or extend a run on their own. They
/// CAN appear inside a run when a real name follows directly
/// ("The Acme Group") — actually we strip them at boundaries only.
///
/// Returns runs in the order they appear. Caller decides which to use
/// — `detect_correction_basic` takes the first; future callers might
/// want all of them.
pub fn extract_proper_noun_runs(text: &str) -> Vec<String> {
    const STOP: &[&str] = &[
        "The", "A", "An", "I", "It", "We", "You", "They", "He", "She",
        "There", "This", "That", "Here", "Yes", "No", "OK", "Okay",
        "Got", "Done", "Yeah", "Sure", "And", "But", "Or", "If", "When",
        "While", "For", "From", "Of", "On", "At", "In", "To", "With",
        "By", "Drafted", "Sent", "Pulled", "Called", "Sure.", "Done.",
    ];
    let stop_set: std::collections::HashSet<&str> = STOP.iter().copied().collect();

    // Light tokenization. Keep apostrophes (O'Brien) and hyphens
    // (Anne-Marie) inside tokens; treat other punctuation as
    // separators. Sentence-ending punctuation (. ? !) closes a run
    // even if the next token is capitalized.
    let mut tokens: Vec<(String, bool)> = Vec::new(); // (token, ends_sentence)
    let mut buf = String::new();
    let mut ends_sentence = false;
    for c in text.chars() {
        if c.is_alphanumeric() || c == '\'' || c == '-' || c == '&' {
            buf.push(c);
        } else {
            if !buf.is_empty() {
                tokens.push((std::mem::take(&mut buf), ends_sentence));
                ends_sentence = false;
            }
            if c == '.' || c == '?' || c == '!' {
                ends_sentence = true;
                if let Some(last) = tokens.last_mut() {
                    last.1 = true;
                }
            }
        }
    }
    if !buf.is_empty() {
        tokens.push((buf, ends_sentence));
    }

    let is_proper = |word: &str| -> bool {
        if word.len() < 2 || stop_set.contains(word) {
            return false;
        }
        let mut chars = word.chars();
        let first = match chars.next() {
            Some(c) => c,
            None => return false,
        };
        // ALL-CAPS: all alphabetic chars uppercase, at least 2 letters
        let all_caps = word.chars().all(|c| !c.is_alphabetic() || c.is_uppercase())
            && word.chars().filter(|c| c.is_alphabetic()).count() >= 2;
        // Title-case: first letter upper, rest can be mixed (but not
        // all-lower-then-upper which is usually a typo we skip)
        let title_case = first.is_uppercase() && !word.chars().all(|c| c.is_uppercase());
        all_caps || title_case
    };

    let mut runs: Vec<String> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    for (i, (token, ends)) in tokens.iter().enumerate() {
        // Always-stop-word at start of sentence (= first token, or
        // previous token ended a sentence) is NOT a name. Inside a
        // run we let "The" pass through if it's part of a name run
        // like "Acme & The Co" — rare, fall through.
        let prev_ended = i == 0 || tokens.get(i.wrapping_sub(1)).map_or(true, |t| t.1);
        let token_is_stop = stop_set.contains(token.as_str());
        if token_is_stop && prev_ended {
            if !current.is_empty() {
                runs.push(current.join(" "));
                current.clear();
            }
            continue;
        }
        if is_proper(token) {
            current.push(token.clone());
        } else {
            if !current.is_empty() {
                runs.push(current.join(" "));
                current.clear();
            }
        }
        // Sentence ends close the run unconditionally.
        if *ends {
            if !current.is_empty() {
                runs.push(current.join(" "));
                current.clear();
            }
        }
    }
    if !current.is_empty() {
        runs.push(current.join(" "));
    }
    runs
}

/// Persist a correction signal as a claim so it survives across
/// conversations. When `matched_entity_id` is set, writes a
/// `(entity, "user-corrected-misreference", user_snippet)` claim
/// with high confidence. When it's None, this is a no-op — the
/// correction still flows through the system prompt in-session.
pub async fn persist_correction(
    pool: &SqlitePool,
    workspace_id: i64,
    signal: &CorrectionSignal,
) -> anyhow::Result<()> {
    let Some(entity_id) = signal.matched_entity_id else {
        return Ok(());
    };
    let value = format!(
        "Travis referenced \"{}\" and the user corrected with: \"{}\". Don't repeat this misreference.",
        signal.used_name, signal.user_snippet
    );
    crate::memory::claims::upsert(
        pool,
        crate::memory::claims::ClaimInput {
            workspace_id,
            entity_id,
            other_entity_id: None,
            predicate: "user-corrected-misreference".to_string(),
            value,
            confidence: Some("high".to_string()),
            source: Some("clarity_check".to_string()),
            source_journal_entry_id: None,
        },
    )
    .await?;
    Ok(())
}

/// Load recent correction claims so they can be injected into a
/// new-conversation system prompt. Returns the formatted prompt block
/// or an empty string if there are none.
///
/// Looks back 7 days; caps at 5 most-recent corrections to keep the
/// prompt tight. Empty when there are no corrections — caller can
/// splat unconditionally.
pub async fn load_persisted_corrections(
    pool: &SqlitePool,
    workspace_id: i64,
) -> String {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT c.value, e.display_name
         FROM claim c
         JOIN entity e ON e.id = c.entity_id
         WHERE c.workspace_id = ?1
           AND c.predicate = 'user-corrected-misreference'
           AND c.superseded_at IS NULL
           AND datetime(c.updated_at) >= datetime('now', '-7 day')
         ORDER BY datetime(c.updated_at) DESC
         LIMIT 5",
    )
    .bind(workspace_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    if rows.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    out.push_str("PERSISTENT MISSES (entity-reference mistakes the user has corrected in the last week — don't repeat them across conversations):\n");
    for (value, entity_name) in &rows {
        out.push_str(&format!("- {entity_name}: {value}\n"));
    }
    out.push_str(
        "When in doubt about which entity the user means, ASK — name the candidates from the inferred context.\n",
    );
    out
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
