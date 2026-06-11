//! Manager loop — the outer layer that refuses to let the worker
//! (the agent loop in `journal_ingest`) bail early.
//!
//! Context: prompt-only enforcement of "drive the process, finish or
//! ask, never hand off" has been hitting diminishing returns across
//! v0.14.3–v0.15.0. Even with explicit banned-phrase prohibitions,
//! Claude keeps producing future-tense placeholders ("reading them
//! now", "I'll generate", "give me a moment") and ending its turn
//! without progress. The fix is architectural: a structural layer
//! that evaluates whether the worker actually delivered, and re-runs
//! it with a forcing message if not.
//!
//! This mirrors what Claude.ai does in its chat interface — the
//! multiple `Thinking` boxes the user sees are manager-driven
//! sub-passes. The worker LLM is unchanged; the manager is the
//! difference.
//!
//! The manager is NOT another LLM. It's a deterministic Rust
//! function that inspects the worker's output and decides one of
//! three outcomes:
//!   - Delivered: artifact, action, or substantive multi-paragraph
//!     answer is present. Stop, return to the user.
//!   - AskedBlocker: response contains a real question (?-marked,
//!     with a concrete noun naming the field/option/doc needed).
//!     Stop, return to the user.
//!   - Handoff: response is a placeholder ("reading them now",
//!     etc.) OR empty OR a generic acknowledgement with no progress.
//!     Loop again with a continuation directive.
//!
//! Manager iteration cap: 3. Each iteration of the inner agent loop
//! still gets its 8 tool-call budget. Worst case is 24 LLM calls per
//! turn, but the typical case lands in 1-2 manager iterations.

use crate::journal::Extraction;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressKind {
    /// Worker delivered something the user can act on: an artifact
    /// file, a queued proposed action, or a substantive answer.
    Delivered,
    /// Worker asked a specific blocker question requiring user input.
    AskedBlocker,
    /// Worker handed the turn back without progressing the work.
    /// Manager will inject a continuation and re-run.
    Handoff,
}

/// Banned future-tense / placeholder phrases. If the response is
/// dominated by these, the manager treats it as a handoff regardless
/// of length.
const HANDOFF_PHRASES: &[&str] = &[
    "i'll generate", "i'll create", "i'll build", "i'll extract",
    "i'll pull", "i'll come back", "i'll be back", "i'll get back",
    "i'll write", "i'll draft", "i'll send", "i'll let you know",
    "reading them now", "reading it now", "reading the docs",
    "reading the po", "reading the wo", "reading the spreadsheet",
    "let me read", "let me check", "let me extract", "let me crunch",
    "let me pull", "let me see", "let me work", "let me dig",
    "working on it", "give me a moment", "give me a sec",
    "on it", "coming up", "coming back", "coming up shortly",
    "to crunch the data", "to build the invoice", "to pull the",
    "drop the sign-in", "drop the excel", "drop it when ready",
    "send the sign-in", "send me the",
];

/// v0.16.4 — phrases where the LLM is hallucinating a tool-not-ready
/// state to avoid calling it. Distinct from generic handoff phrases
/// because the fix is different: we don't want to just retry, we want
/// to tell the LLM "the tool IS ready, call it now."
const PYODIDE_EXCUSE_PHRASES: &[&str] = &[
    "pyodide is still cold",
    "pyodide isn't ready",
    "pyodide is not ready",
    "interpreter is still cold",
    "interpreter is still loading",
    "interpreter isn't ready",
    "interpreter is not ready",
    "wasm environment",
    "cold-loading",
    "cold loading",
    "still cold-loading",
    "python interpreter is still",
    "the moment it's ready",
    "the moment it's available",
    "as soon as interpreter loads",
    "as soon as the interpreter",
    "i can't emit the pdf this turn",
    "can't generate the pdf this turn",
];

/// Whether the worker is making a Pyodide-cold excuse. Used by
/// the manager to force a Handoff + a more targeted continuation
/// directive ("the tool IS ready, call it").
pub fn is_pyodide_excuse(response: &str) -> bool {
    let lower = response.to_lowercase();
    PYODIDE_EXCUSE_PHRASES.iter().any(|p| lower.contains(p))
}

/// Decide whether the worker made progress this turn.
///
/// `generated_doc_ids` is the list of doc ids run_python produced
/// (via the conversation_message payload). `tool_calls_made` is
/// whether the agent loop called any tools other than
/// `report_extraction`.
pub fn evaluate_progress(
    extraction: &Extraction,
    generated_doc_ids: &[i64],
    tool_calls_made: bool,
) -> ProgressKind {
    // Strong delivery signals — these always count regardless of
    // response prose.
    if !generated_doc_ids.is_empty() {
        return ProgressKind::Delivered;
    }
    if !extraction.proposed_actions.is_empty() {
        return ProgressKind::Delivered;
    }
    // v0.20.11 — `doc#N` marker in the response means the worker
    // generated (or referenced) a file. The chat UI uses the same
    // marker to render the file card. Strong delivery signal.
    if extraction
        .response
        .as_deref()
        .map(|r| r.contains("doc#"))
        .unwrap_or(false)
    {
        return ProgressKind::Delivered;
    }

    let response = extraction
        .response
        .as_deref()
        .unwrap_or("")
        .trim();

    // v0.16.4 — Pyodide-excuse detection. If the worker manufactured
    // "interpreter is cold-loading" without actually generating
    // output, treat as a Handoff so the manager forces a retry
    // with a targeted directive. Strong signal: the LLM is
    // hallucinating tool unavailability.
    if !response.is_empty() && is_pyodide_excuse(response) {
        return ProgressKind::Handoff;
    }

    if response.is_empty() {
        return ProgressKind::Handoff;
    }

    let lower = response.to_lowercase();

    // Handoff phrase detection. Even with a long response, if it
    // OPENS with a handoff phrase or contains multiple of them, the
    // worker is stalling.
    let handoff_hits: usize = HANDOFF_PHRASES
        .iter()
        .filter(|p| lower.contains(*p))
        .count();
    let opens_with_handoff = HANDOFF_PHRASES
        .iter()
        .any(|p| lower.trim_start().starts_with(*p));

    // Does the response ask a substantive question? Question mark
    // alone isn't enough — we want at least one concrete noun in
    // proximity that names what's needed (field, doc, value, option).
    let has_question = response.contains('?');
    let has_specific_noun = lower.contains("which ")
        || lower.contains("what ")
        || lower.contains("when ")
        || lower.contains("how many")
        || lower.contains("how much")
        || lower.contains("name of")
        || lower.contains("number of");
    let asks_specific = has_question && has_specific_noun;

    // Two handoff phrases or opens with one → almost certainly a
    // placeholder. Bail out as Handoff unless it also asks something
    // specific (rare but possible: "Reading the docs — which date
    // range should I use?").
    if (handoff_hits >= 2 || opens_with_handoff) && !asks_specific {
        return ProgressKind::Handoff;
    }

    // Asked a specific blocker question — good outcome.
    if asks_specific {
        return ProgressKind::AskedBlocker;
    }

    // Substantive response with tool calls = work was done, content
    // is reported. Counts as delivered.
    if tool_calls_made && response.len() >= 80 {
        return ProgressKind::Delivered;
    }

    // Long-form prose (analysis, summary, draft) without tools or
    // question — still counts as delivered if it's meaningful.
    if response.len() >= 200 {
        return ProgressKind::Delivered;
    }

    // Short, no tools, no question, doesn't look like a placeholder
    // — call it Handoff so we try harder next iteration. The cost is
    // one extra LLM call when the worker actually had a valid short
    // answer; the upside is catching the cases where it's bailing.
    if handoff_hits >= 1 {
        return ProgressKind::Handoff;
    }

    // Default: treat short non-placeholder replies as delivered.
    // This covers conversational/greeting turns where a short reply
    // is correct.
    ProgressKind::Delivered
}

/// The continuation message injected when the manager detects a
/// handoff. Appended as a user-role message in the rerun's message
/// stack, between the worker's previous attempt and the new turn.
pub fn continuation_directive() -> &'static str {
    "Your previous reply handed the turn back to me without progressing the work. The user is still waiting on a deliverable.\n\n\
     DO THIS NOW in your next turn:\n\
     - If you need to read documents: call read_document on them. Pre-loaded summaries are in the user message but the FULL bodies require the tool.\n\
     - If you need to process a spreadsheet: call run_python with pandas to load /inputs/<filename> and extract what you need.\n\
     - If you need to analyze sample styling: call analyze_document_styling.\n\
     - If you have enough information: call run_python to GENERATE the artifact and emit it to /outputs/.\n\n\
     Your response must EITHER report concrete results in past tense (\"Generated invoice ...\", \"Extracted 14 dates from ...\") OR ask a SPECIFIC question naming the exact field or value you need (\"Which 10 of these 22 dates belong on this invoice?\"). \n\n\
     Do NOT write another placeholder. Phrases like \"reading them now\", \"on it\", \"give me a moment\", \"I'll generate\", \"I'll come back\", \"drop the X\" are forbidden. The user already gave you the input — DO the work."
}
