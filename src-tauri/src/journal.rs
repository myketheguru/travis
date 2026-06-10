use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, State};

use crate::actions::{self, ProposedAction};
use crate::behavioral;
use crate::conversation::{self, Thread};
use crate::db::UserProfile;
use crate::domain::task::{self, Task, TaskFilter, TaskInput};
use crate::feedback::{self, AppFeedbackInput};
use crate::identity;
use crate::llm::{self, ChatWithToolsOptions, Message, Role, ToolChoice, ToolDef};
use crate::tools::{self, ToolContext};
use crate::memory;
use crate::reminders::{self, ReminderInput};
use crate::secrets;
use crate::telemetry;
use crate::AppState;

/// Build the journal extraction system prompt.
///
/// **Cache invariant:** Anthropic's prompt cache hits on prefix
/// equality. Anything that changes per-turn must go in the user
/// message, not here. Allowed inputs to this prompt:
///   - profile (changes ~never)
///   - pack_fragment (changes only on pack toggle / app rebuild)
///   - workspace_block (changes only on workspace switch)
///
/// Specifically NOT allowed: today's date, open task list, recent
/// memory snippets, conversation history. All of those go into the
/// user message at the call site so the cache prefix stays stable
/// within the 5-minute Anthropic cache window.
fn build_system_prompt(
    profile: &UserProfile,
    pack_fragment: &str,
    workspace_block: &str,
    pack_memory_block: &str,
) -> String {
    let name = profile.name.trim();
    let first = name.split_whitespace().next().unwrap_or(name);
    let _role = profile.role.trim();
    let _org = profile.org.trim();
    let user_context = profile.context_block();

    // Persona block — Travis's identity / values / voice / constraints.
    // Single source of truth across journal, ask, proactive, splash
    // (BRAIN.md capability #2). Replaces the inline VOICE block that
    // used to live here.
    let persona_block = crate::persona::build_prompt_fragment(profile);

    let mut prompt = format!(r#"{persona_block}You are Travis — a personal AI assistant for {first}. You can help with anything Claude.ai can: writing, analysis, code, research, creative work, document handling, scheduling, and ops capture. The chat surface is your primary interface; tools let you act, persist information, and process files locally on {first}'s computer.

== HOW YOUR TURN ENDS — READ THIS FIRST ==

You drive the process. You own it. The user is not your reviewer or your gatekeeper — they're handing you a task and trusting you to complete it. Act, don't narrate.

Your turn ends ONLY when one of these is true:

1. **You delivered an artifact** — a file generated, an action completed, a substantive answer pulled from memory, a draft they can edit, a structured field-by-field readout of what you found in a document. Something the user can act on or check.

2. **You asked a SPECIFIC question** that the user must answer for you to proceed — name the field, the option, the doc you need. Vague asks ("what would you like next?") don't count.

3. **You hit a real blocker** — a tool you called returned a hard error, or the user asked for something genuinely outside your capabilities.

== FUTURE TENSE IS BANNED ==

If your `response` contains any of these patterns, you have FAILED this turn — go back and call the relevant tool(s) BEFORE writing the response:

- "I'll generate…", "I'll create…", "I'll build…", "I'll extract…", "I'll write…"
- "Reading it now", "Let me read…", "Let me check…", "Let me extract…"
- "Working on it", "Give me a moment", "Coming up", "On it"
- "I'll come back with…", "I'll be back with…", "Coming back with…"
- "Captured", "Noted", "Got it" (when used as a complete reply)

These phrases describe work you HAVEN'T done. The user already gave you the input — your job is to do it in THIS turn. If you need to call a tool, call it. Then in your `response`, report the RESULT in past tense.

Examples of GOOD end-of-turn replies:
- "Drafted the email — copied to your clipboard. Two things I assumed: I kept the subject neutral and didn't mention the deadline since you didn't specify one. Want me to adjust?"
- "Pulled the fields you asked for from the source doc: field A = X, field B = Y, field C = blank. The blank field isn't in this document — do you have a value, or should I use a default?"
- "Generated the report — link below. Crunched the totals in Python; used last quarter's averages where the August row was missing. Worth double-checking the August line."
- "Looked at the project plan you uploaded. Three risks jump out: [1] the timeline assumes 5 engineers but you mentioned hiring is paused, [2] the deployment window overlaps the holiday freeze, [3] the dependency on Acme isn't covered by a signed contract yet. Want me to draft a revision?"
- "You mentioned that meeting on Tuesday — your notes from the 18th say it was rescheduled to next Wednesday. Looks like that's still on the calendar."
- "Morning. What's on for today?"

If you genuinely cannot complete the work in the remaining tool-call iterations, ASK A SPECIFIC QUESTION instead of writing a placeholder. "I need the spreadsheet that has the line items to finish the invoice — do you have it?" is acceptable. "I'll get back to you" is not.

== DOCUMENT HANDLING ==

Document editing and generation is a universal capability — every professional context needs it (invoices, reports, contracts, briefs, sign-in sheets, decks, spreadsheets, letters). The pattern below applies regardless of domain; any enabled pack will layer its own vocabulary and rules on top.

**When documents are attached:** their content is pre-loaded into the user message under `== ATTACHED DOCUMENTS ==`. Spreadsheets show only a structural preview — read them in `run_python` with pandas (`pd.read_excel("/inputs/<filename>")` or `pd.read_csv`). Other docs are summarized; call `read_document(documentId)` if you need the full body. Image inputs (PNG/JPG) become Travis-visible automatically via vision.

**When the user gives you a sample + asks to adapt it** ("make one like this", "match this format", "do this for X instead", "edit this for the new Y"):
1. Call `analyze_document_styling(documentId)` on the sample — returns the colour/font/layout JSON you need to drive generation code.
2. Optionally `read_document(documentId)` for the full body if the pre-loaded summary doesn't show the fields you need.
3. Enumerate the fields you found WITH THEIR CURRENT VALUES in your `response` — give the user a numbered list of "here's what's on the current doc → tell me the new value for each". Use generic universal headers: bold field name, code-fenced current value, arrow + question.
4. Once the user answers (or supplies supporting docs that fill the fields), call `run_python` to generate the new version using the styling JSON. Emit to `/outputs/<descriptive_filename>.pdf` (or `.xlsx`, `.docx` — match the sample's format).
5. In your `response` after generation: report the result in PAST TENSE + list the assumptions you made + flag any field you defaulted.

**When the user uploads multiple supporting docs** (a sample + PO + WO + reference data, or several invoices + a pricing sheet, etc.):
1. ASSUME the docs are your input set. Multiple documents arriving together are not a trial balloon — they're the data you need to do the work.
2. Run `analyze_document_styling` once on the sample (cached).
3. Use `run_python` with pandas on spreadsheets — never try to enumerate spreadsheet content in your response.
4. Cross-reference fields across the docs (see CROSS-DOCUMENT RECONCILIATION below).
5. Generate the artifact with `run_python`. Don't ask for permission first; if you have enough information for a reasonable default, USE IT and flag the assumption at the end.

**Iterative refinement.** After you've generated a document and the user asks for a tweak ("remove the school name from the To: block", "add 7 hours to row 1", "the line needs to come down a tiny bit"): treat it as an EDIT on the existing artifact, not a regeneration from scratch. (v0.15.3 adds a dedicated `edit_python_artifact` tool; until then, re-run `run_python` with the prior script reasoning + the small change applied.)

**Mid-workflow doc uploads.** When the user uploads additional documents mid-workflow (supplementary samples, sign-in sheets, pricing sheets), they're CONTINUATION inputs, not fresh captures. Read them, integrate them into the work, and advance — do NOT respond with "got it, give me a moment" placeholder phrasing.

**When to call `run_python` vs a structured-action tool.** Use `run_python` when:
- The user supplied a SAMPLE to match (styling/layout differs from any canonical template).
- The task needs constraint solving (find quantities/values summing to $X exactly, find combinations meeting multiple criteria).
- Cross-document reconciliation deeper than a single tool.
- An uncommon format (.docx, .pptx, custom layouts).
- The user explicitly asks for "code" or imperative reasoning.

Use a structured-action tool when one exists and matches the request shape exactly (canonical template + standard fields + no sample to match). Packs may register such tools; trust their tool descriptions.

== CROSS-DOCUMENT RECONCILIATION ==

When the user attaches multiple documents — a sample + PO + WO, several invoices + a pricing sheet, contract + appendix + service log — automatically compare overlapping fields across them BEFORE you produce output. Specifically check: names + addresses, dollar amounts + unit prices, dates + service periods, reference numbers (PO/WO/invoice/contract), line-item descriptions, total amounts.

If you find a discrepancy, FLAG IT EXPLICITLY in your response. Don't quietly pick one side. Name the authoritative source. Defaults:

- **A PO authorizing payment overrides a sample document** made for a different engagement. The sample shows old-engagement data; the PO is what's authorized to bill now.
- **A contract or contract appendix overrides a downstream pricing sheet.** If a per-school pricing sheet conflicts with the master service catalog, the catalog rate is authoritative; the pricing sheet likely has a labeling error.
- **A sign-in sheet (logged data) overrides recollection** ("the CEO said work started 03/20 but the sheet logs 03/17" → go with the sheet, surface the discrepancy).
- **A more recent document overrides an older one** for the same field, when both are authoritative sources.
- **An external-facing official document (PO, WO, contract) overrides an internal working doc** (sample, draft, narrative).

When numbers don't reconcile, trace WHERE the divergence enters. The user values knowing "this rate came from the contract; the sample used a different one because it was a previous engagement" — that's defensible. Quietly picking the larger number is not.

== WHEN ASKED FOR A RECOMMENDATION ==

When the user asks "what do you suggest", "what should we do", "which way", "your call", or similar — TAKE A POSITION. Lead with your recommendation, then justify it. Two-line example:

> "My recommendation: don't re-issue invoice #1 — it's correct against the contract rate. The error is in the pricing sheet, not the invoice. Here's why: ..."

Option-listing without a position is a cop-out when the user explicitly asked for a recommendation. If the user's stated instinct points toward the wrong path (e.g., "should I correct invoice #1 to make the math work?"), PUSH BACK with reasoning — "the instinct to correct #1 is the trap because..." Decisions on payment documents, contracts, and money-handling carry legal weight; clean arithmetic isn't worth a wrong number.

Caveat the position only when there's a genuinely open factual question you need answered — "I'd build it this way assuming X; if X is wrong, the answer changes to Y."

== CAPTURE HAPPENS IN THE BACKGROUND ==

A separate background pipeline extracts tasks, entities, reminders, capability gaps, entity facts, hypotheses, and affect signals from each turn. You DO NOT do that work here. Focus your `response` on the user's actual request; the background pipeline reads both your reply and the user's message to capture the rest.

Do not narrate captures in your `response`. NEVER say "I captured X" or "I noted Y" — those events are invisible to the user and irrelevant to your reply.

== WHAT YOU CAN DO ==

**Write and edit** — drafts, emails, summaries, plans, status updates, code. Put anything the user will paste elsewhere into the clipboard with `write_clipboard`.

**Run code** — `run_python` gives you a full CPython interpreter with reportlab, openpyxl, pypdf, pandas, pillow, python-docx pre-installed. Use it for data analysis, file generation (PDFs, Excel, Word), spreadsheet processing, image work, constraint solving, or anything imperative. Files in `/inputs/` are the user's attached docs; emit results to `/outputs/`. Set a `purpose` string the user sees as the step name ("Analyzing Q3 sales numbers", "Generating PDF report").

**Handle documents** — `read_document` for full text, `analyze_document_styling` for layout/colour/font JSON on a sample, `find_documents` to search past attachments, `run_python` for spreadsheets.

**Remember and recall** — `search_memory` for semantic lookup across past journal entries, `list_open_tasks` for the user's task list. The user message already includes RELEVANT MEMORY snippets and OPEN TASKS — start there before searching deeper.

**Schedule and act** — propose `set_reminder` for OS notifications, `defer_task` to move due dates, `send_email` (when Gmail/Outlook is connected), `open_url` to hand the user a link, `web_fetch` for a specific URL's content.

**Configure** — `update_profile_context` when the user tells you something about themselves or corrects your phrasing/voice.

== WHAT YOU CAN'T DO YET ==

Always voice these conversationally in `response` when relevant — never silently swallow:

- Send email without a connected Gmail/Outlook account — offer to draft to clipboard instead.
- Schedule calendar events / send invites — coming soon.
- Make phone calls or send SMS.
- Browse the web freely — only `web_fetch` a specific URL the user gives you.
- Anything destructive on the file system.

Example: "I'd email Maria, but Gmail isn't connected — should I draft it to your clipboard so you can send manually?"

== USER CONTEXT ==

Use this to make examples + language relevant; never invent details beyond what's stated; if the context is sparse, ask 1 clarifying question over time to enrich it rather than guessing.

{user_context}

The user message includes: TODAY's date · {first}'s OPEN TASKS (with ids — use these for completion + defer) · RELEVANT MEMORY (snippets pulled by semantic search) · The new user turn · Any active workflow state · Attached documents.

== JSON OUTPUT ==

Set `intent` to "operational" if you used tools or emitted workflowOps/proposedActions; "conversational" otherwise. Metadata only.

Your output is the `report_extraction` tool call. The fields you care about:

- `response` (REQUIRED, string, minLength 1) — your reply that shows in the chat. PAST TENSE. Substantive.
- `thinking` (optional, string, 2-4 sentences) — your inner narration shown in a collapsible section. What you understood, what you noticed in any attached doc, what you decided to do.
- `workflowOps` (array) — workflow state transitions: `start` a recipe, `fillSlot` to populate a field, `complete` when finalised, `abandon` to stop. Use these to model multi-turn work.
- `proposedActions` (array) — actions that need the user's go-ahead before they execute. The user sees a confirm card.

Available `proposedActions` kinds:

- `defer_task` — params {{ "taskId": int, "newDueAt": "YYYY-MM-DD" }}
- `set_reminder` — params {{ "text": str, "remindAt": "YYYY-MM-DD HH:MM" }}
- `write_clipboard` — params {{ "text": str }}. Copy drafted text to clipboard.
- `run_shell_command` — params {{ "command": str, "workingDir"?: str, "timeoutSeconds"?: int }}. READ-ONLY ONLY (`git status`, `ls`, `node --version`, etc.). Never destructive (deletes, force-pushes, sudo). User must enable in Settings.
- `send_email` — params {{ "to": str, "subject": str, "body": str, "provider"?: "gmail"|"outlook" }}. Only when user explicitly asked.
- `open_url` — hand a URL to the user's browser.
- `update_profile_context` — params {{ "contextBlurb"?: str, "communicationStyle"?: str }}. Use (a) when the user told you about their work/role; or (b) when they corrected your voice/phrasing — pass `communicationStyle` as a single-line rule in their voice.

Each proposedAction has a `rationale` (under 90 chars) shown verbatim to the user on the confirm card. Write it as the OUTCOME in plain English, not the technical command. Bad: "Run `git status`". Good: "Show me what's changed in this folder."

Don't propose actions the user didn't ask for. If unsure of a parameter, ask a clarifying question instead.

Capture-only fields (`tasks`, `entities`, `reminders`, `completedTaskIds`, `clarifyingQuestions`, `capabilityGaps`, `workspaceRouting`, `entityFacts`, `hypotheses`, `affectSignals`, `genericEntities`) — leave EMPTY. The background pipeline handles them. They'll be ignored if you populate them.
"#);

    // Append vertical-pack guidance — each enabled pack contributes a
    // prompt fragment describing its domain (PACKS_AUDIT.md step 10).
    if !pack_fragment.is_empty() {
        prompt.push_str("\n\n");
        prompt.push_str(pack_fragment);
    }
    if !workspace_block.is_empty() {
        prompt.push_str("\n\n");
        prompt.push_str(workspace_block);
    }
    // v0.19.0 — pack memory recall. User-stated rules / preferences /
    // constraints / facts / corrections, scoped to entities currently
    // in conversation context. Appended last so they sit near the
    // model's attention horizon and aren't outranked by the older
    // boilerplate above.
    if !pack_memory_block.is_empty() {
        prompt.push_str("\n\n");
        prompt.push_str(pack_memory_block);
    }
    prompt
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractedTask {
    pub title: String,
    #[serde(default)]
    pub due_at: Option<String>,
    #[serde(default)]
    pub priority: Option<i64>,
    #[serde(default)]
    pub notes: Option<String>,
}

/// Pack-aware bag of named-entity mentions extracted from a journal note.
///
/// Buckets are pluralised entity kinds declared by enabled packs — for the
/// L2E pack, that's `coaches` / `schools` / `depts`. The shape stays a
/// `HashMap` so a future pack with different entity kinds (e.g. tutoring
/// declaring `tutors` / `students` / `parents`) just adds buckets without
/// any core schema change.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EntityMentions(pub std::collections::HashMap<String, Vec<String>>);

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractedReminder {
    pub text: String,
    #[serde(default)]
    pub remind_at: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityGap {
    pub capability: String,
    #[serde(default)]
    pub context: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposedActionInput {
    pub kind: String,
    #[serde(default)]
    pub rationale: Option<String>,
    #[serde(default)]
    pub params: serde_json::Value,
}

fn default_intent() -> String {
    "operational".into()
}

/// One ambient-discovered entity outside the pack-declared kind
/// buckets — names Travis sees in journals before any pack has
/// claimed them as a typed record. The LLM picks a top-level
/// `kind` (person / place / org); the persistence layer maps to
/// `<kind>:unknown` and stores at GENERIC confidence (0.5).
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenericEntity {
    pub name: String,
    /// One of "person", "place", "org". Anything else is dropped.
    #[serde(default)]
    pub kind: String,
    /// Optional short snippet showing where in the note the name
    /// appeared. Stored on the `mentioned` event for UI context;
    /// the entity row itself doesn't carry it.
    #[serde(default)]
    pub context_snippet: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceRouting {
    /// Slug of the target workspace. None means "stay where the
    /// active workspace is".
    #[serde(default)]
    pub target_slug: Option<String>,
    /// LLM's confidence in the routing decision.
    /// "high" / "medium" → silent route. "low" → ask the user.
    #[serde(default)]
    pub confidence: Option<String>,
    /// One-line rationale shown in the UI chip.
    #[serde(default)]
    pub rationale: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Extraction {
    #[serde(default = "default_intent")]
    pub intent: String,
    #[serde(default)]
    pub response: Option<String>,
    #[serde(default)]
    pub tasks: Vec<ExtractedTask>,
    #[serde(default)]
    pub entities: EntityMentions,
    #[serde(default)]
    pub reminders: Vec<ExtractedReminder>,
    #[serde(default)]
    pub completed_task_ids: Vec<i64>,
    #[serde(default)]
    pub clarifying_questions: Vec<String>,
    #[serde(default)]
    pub capability_gaps: Vec<CapabilityGap>,
    #[serde(default)]
    pub proposed_actions: Vec<ProposedActionInput>,
    #[serde(default)]
    pub workspace_routing: Option<WorkspaceRouting>,
    /// Names mentioned in the note that don't fit any pack-declared
    /// entity kind — populated by the LLM and dedup'd against existing
    /// entity rows on persist.
    #[serde(default)]
    pub generic_entities: Vec<GenericEntity>,
    /// Typed facts about specific entities extracted from the note
    /// (BRAIN.md Phase 4.5 #2). Each is persisted to the `claim`
    /// table so it survives across sessions and surfaces in retrieval.
    #[serde(default)]
    pub entity_facts: Vec<ExtractedEntityFact>,
    /// Hypothesis-grade notes Travis writes to working memory for
    /// the next ~30 minutes of this conversation (Phase 4.5 #6).
    #[serde(default)]
    pub hypotheses: Vec<ExtractedHypothesis>,
    /// Light tone + themes pulled from the note (capability #7 wellbeing).
    /// Null when the note is pure ops with no emotional register.
    #[serde(default)]
    pub affect_signals: Option<ExtractedAffect>,
    /// Workflow transitions the LLM wants applied to the active
    /// workflow on this conversation. Empty when no workflow activity.
    /// See [`crate::workflows`] and [[feedback-workflow-led]].
    #[serde(default)]
    pub workflow_ops: Vec<ExtractedWorkflowOp>,
    /// v0.14.0 — concise inner reasoning surfaced in a collapsible
    /// "Thinking" section of the chat (Claude-class). 2-4 sentences:
    /// what you understood about the request, what you're planning,
    /// any constraint you noticed. Empty for purely conversational
    /// turns where reasoning isn't useful.
    #[serde(default)]
    pub thinking: Option<String>,
    /// v0.19.3 — document classifications the LLM picked up from
    /// attached docs. Applied as kind updates + entity links.
    #[serde(default)]
    pub document_classifications: Vec<ProposedDocumentClassification>,
    /// v0.19.3 — coach_hours rows extracted from signing sheets.
    /// Persisted to the pack's coach_hours table; coach + school
    /// rows are auto-created via ensure() if not present.
    #[serde(default)]
    pub coach_hours: Vec<ExtractedCoachHours>,
    /// v0.19.4 — engagement / contract enrichment from PO/WO docs.
    /// When a PO is classified, the LLM can attach its ref, period,
    /// and ceiling so the engagement row populates with the business
    /// terms instead of just the name.
    #[serde(default)]
    pub engagement_enrichments: Vec<ExtractedEngagementEnrichment>,
    /// v0.19.4 — invoice drafts the LLM produced this turn (e.g. via
    /// run_python emitting an invoice PDF). One row per draft; the
    /// pack persists to its `invoice` table as status='draft' so the
    /// Invoices tab reflects work-in-progress without committing
    /// "sent" or "paid" without confirmation.
    #[serde(default)]
    pub invoice_drafts: Vec<ExtractedInvoiceDraft>,
    /// v0.19.0 — pack memories the LLM picked out of the turn.
    /// User-stated rules, preferences, constraints, corrections, or
    /// facts that should outlive the current conversation. Persisted
    /// to `pack_memory` and recalled into future system prompts.
    /// Travis should populate this whenever the user states something
    /// the LLM should remember — proactively, not only when asked.
    #[serde(default)]
    pub pack_memories: Vec<ExtractedPackMemory>,
}

/// v0.19.3 — proposed document classification the LLM emits after
/// reading attached docs. The agent loop applies these immediately:
/// sets the document kind and links to a spine entity if one is
/// named (resolves by spine entity lookup on the (kind, name) pair).
/// "Generic file" gets reclassified to "po" / "wo" / "signed_sheet"
/// / "invoice" / "contract" / etc. so the Manage tab can group docs
/// by their real semantic kind, not the catch-all bucket.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposedDocumentClassification {
    pub document_id: i64,
    /// New kind: po | wo | signed_sheet | invoice | contract | …
    pub kind: String,
    /// Optional spine entity (kind, name) this doc belongs to —
    /// the agent loop resolves it to an entity_id by name and writes
    /// a document_link row.
    #[serde(default)]
    pub linked_entity_kind: Option<String>,
    #[serde(default)]
    pub linked_entity_name: Option<String>,
    /// Optional period the doc covers (helps with "show me docs from
    /// March-June"-style filters).
    #[serde(default)]
    pub period_start: Option<String>,
    #[serde(default)]
    pub period_end: Option<String>,
}

/// v0.19.3 — coach_hours row extracted from a signing sheet. Agent
/// loop ensures the coach + school exist, then inserts the row.
/// `linked_signing_sheet_doc_id` ties the row back to the sheet for
/// audit.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractedCoachHours {
    pub coach_name: String,
    pub school_name: String,
    /// ISO date string YYYY-MM-DD.
    pub session_date: String,
    /// Hours worked that day, decimal.
    pub hours: f64,
    /// Optional doc id of the signing sheet these hours came from.
    #[serde(default)]
    pub linked_signing_sheet_doc_id: Option<i64>,
}

/// v0.19.4 — fields the LLM picks out of a PO/WO/scope doc to
/// enrich the matching engagement row. All fields optional; the
/// pack only updates columns the LLM actually filled.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractedEngagementEnrichment {
    pub engagement_name: String,
    /// Contract reference string from the doc — e.g. "QR179CF".
    #[serde(default)]
    pub contract_ref: Option<String>,
    /// Activity start.
    #[serde(default)]
    pub period_start: Option<String>,
    /// Activity end.
    #[serde(default)]
    pub period_end: Option<String>,
    /// Total dollar value in cents (PO ceiling).
    #[serde(default)]
    pub ceiling_cents: Option<i64>,
    /// School year string, e.g. "2025-26".
    #[serde(default)]
    pub school_year: Option<String>,
}

/// v0.19.4 — a single invoice draft the LLM produced this turn.
/// Pack creates a row in the `invoice` table with status='draft'.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractedInvoiceDraft {
    /// The invoice number string. e.g. "LTE2026217002".
    pub number: String,
    /// Bill-to / recipient string (the school name or full address).
    pub recipient: String,
    /// Optional school + coach refs by name. Resolved by the pack.
    #[serde(default)]
    pub school_name: Option<String>,
    #[serde(default)]
    pub coach_name: Option<String>,
    /// ISO YYYY-MM-DD.
    pub period_start: String,
    pub period_end: String,
    /// Decimal hours totalled across line items.
    #[serde(default)]
    pub hours_total: f64,
    /// Per-hour or per-unit rate in cents.
    #[serde(default)]
    pub rate_cents: i64,
    /// Total amount in cents.
    pub amount_cents: i64,
    /// Optional generated-PDF document id so the invoice row links
    /// back to the file Travis produced.
    #[serde(default)]
    pub generated_doc_id: Option<i64>,
    /// Optional note (e.g. "Includes 03/17 per CEO permission —
    /// outside PO window otherwise").
    #[serde(default)]
    pub notes: Option<String>,
}

/// v0.19.0 — a single pack memory the LLM picked out of the turn.
/// Mirror of the [`crate::tools::remember_constraint`] tool input
/// but emitted automatically as part of the extraction.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractedPackMemory {
    pub pack_slug: String,
    /// rule | preference | constraint | fact | correction
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub target_kind: Option<String>,
    #[serde(default)]
    pub target_id: Option<i64>,
    pub content: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractedAffect {
    pub tone: Option<String>,
    #[serde(default)]
    pub themes: Vec<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractedHypothesis {
    pub topic: String,
    pub note: String,
    #[serde(default)]
    pub confidence: Option<String>,
}

/// One LLM-emitted workflow transition. The dialogue manager applies
/// these in order after extraction, updating the persisted workflow
/// state. See [`crate::workflows`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum ExtractedWorkflowOp {
    /// Begin a new workflow on this conversation. Abandons any active
    /// one first — Taylor's new intent supersedes whatever was running.
    Start {
        recipe: String,
        #[serde(default)]
        intent: Option<String>,
    },
    /// Fill a single slot on the active workflow. `value` is JSON of
    /// any shape the slot expects.
    FillSlot {
        slot_name: String,
        value: serde_json::Value,
        #[serde(default)]
        source: Option<String>,
    },
    /// All required slots are filled and the finalize action has been
    /// proposed. Marks the workflow completed so it stops surfacing.
    Complete,
    /// User changed subject; abandon the active workflow.
    Abandon {
        #[serde(default)]
        reason: Option<String>,
    },
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractedEntityFact {
    /// Entity name as the LLM extracted it. Resolved to entity_id on persist.
    pub entity: String,
    /// Optional second entity for relational facts.
    #[serde(default)]
    pub other_entity: Option<String>,
    /// Predicate slug — role, relationship, contact, context, etc.
    pub predicate: String,
    /// The fact's value (short phrase).
    pub value: String,
    /// LLM-assigned confidence: high/medium/low.
    #[serde(default)]
    pub confidence: Option<String>,
}

/// One chip surfaced in the capture overlay when the LLM extraction
/// matched a name to an entity Travis already knew about. Renders as
/// "→ Maria (coach)" — passive recognition, no interaction needed.
/// Only entities with `mentions_count > 1` (i.e. pre-existing) make
/// it onto the chip list — fresh-this-turn names don't generate
/// noise.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MentionChip {
    pub entity_id: i64,
    pub display_name: String,
    pub kind: String,
    pub mentions_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutingResult {
    /// Slug + name of the workspace where this capture actually
    /// landed. May differ from the active workspace when the LLM
    /// detected a clear other-world signal.
    pub workspace_slug: String,
    pub workspace_name: String,
    /// True when routing diverged from the active workspace — the UI
    /// uses this to render the "Captured to <name>" chip.
    pub routed: bool,
    /// LLM's confidence ("high" | "medium" | "low") if a decision was
    /// reported. Empty when the model didn't return a routing object.
    #[serde(default)]
    pub confidence: Option<String>,
    /// One-line rationale shown in the chip tooltip.
    #[serde(default)]
    pub rationale: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalIngestResult {
    pub journal_entry_id: i64,
    pub conversation_id: i64,
    pub thread: Thread,
    pub intent: String,
    pub response: Option<String>,
    pub tasks_created: Vec<Task>,
    pub tasks_completed: Vec<Task>,
    pub entities: EntityMentions,
    pub reminders: Vec<ExtractedReminder>,
    pub clarifying_questions: Vec<String>,
    pub capability_gaps: Vec<CapabilityGap>,
    pub proposed_actions: Vec<ProposedAction>,
    pub routing: Option<RoutingResult>,
    /// Pre-existing entities that this turn's extraction matched.
    /// The overlay renders these as faint chips beneath the chat
    /// reply — "Travis recognised these names from before."
    pub mention_chips: Vec<MentionChip>,
    pub extraction_ok: bool,
    pub error: Option<String>,
}

/// v0.16.0 — auto-open a case (or find the existing one) for the
/// current conversation. Best-effort; failures log and return None.
///
/// Triggers (need ≥2 of 3 to auto-open a fresh case):
/// - active workflow on this conversation
/// - multi-doc upload in this turn (≥2 doc# markers)
/// - conversation depth ≥3 (this isn't a first message)
///
/// Always touches the case's last_activity_at on entry if one exists,
/// so cases stay "warm" while the user is interacting.
async fn build_or_resume_case(
    pool: &sqlx::SqlitePool,
    conv_id: i64,
    workspace_id: i64,
    raw: &str,
    inbound_doc_ids: &[i64],
    prior_message_count: i64,
) -> Option<crate::cases::db::Case> {
    // Already linked to a case?
    if let Some(c) = crate::cases::db::find_by_conversation(pool, conv_id).await {
        let _ = crate::cases::db::touch(pool, c.id).await;
        return Some(c);
    }

    // Evaluate auto-open triggers
    let workflow_active = crate::workflows::state::get_active(pool, conv_id)
        .await
        .is_some();
    let multi_doc = inbound_doc_ids.len() >= 2;
    let deep_conv = prior_message_count >= 3;
    let trigger_count =
        (workflow_active as u8) + (multi_doc as u8) + (deep_conv as u8);
    if trigger_count < 2 {
        return None;
    }

    // Build a case name. Prefer the active workflow's recipe; fall
    // back to a truncated user note. Never panic on weird input.
    let name = if let Some(w) =
        crate::workflows::state::get_active(pool, conv_id).await
    {
        format!("Case: {}", w.recipe_name)
    } else {
        let trimmed = raw.trim();
        let snippet: String = trimmed.chars().take(60).collect();
        if snippet.is_empty() {
            format!("Case for conversation #{conv_id}")
        } else {
            snippet
        }
    };

    let case = match crate::cases::db::upsert_open(
        pool,
        workspace_id,
        crate::cases::db::CaseInput {
            name,
            summary: None,
            parent_case_id: None,
        },
    )
    .await
    {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("v0.16.0 auto-open case failed: {e}");
            return None;
        }
    };
    if let Err(e) = crate::cases::db::link_conversation(pool, case.id, conv_id).await {
        tracing::warn!("v0.16.0 link_conversation failed: {e}");
    }
    tracing::info!(
        "v0.16.0 auto-opened case '{}' (id {}) for conv {} — triggers: workflow={} multi_doc={} deep={}",
        case.name,
        case.id,
        conv_id,
        workflow_active,
        multi_doc,
        deep_conv
    );
    Some(case)
}

/// Format the active case for inclusion in the LLM user message.
/// Renders a tight ~5-line block giving the LLM continuity context
/// for the current case. Kept short — Travis's broader cases-list
/// block already names other cases.
fn format_active_case_block(case: &crate::cases::db::Case) -> String {
    let mut s = String::from("== ACTIVE CASE ==\n");
    s.push_str(&format!("You are working on case: {} (#{})\n", case.name, case.id));
    s.push_str(&format!("Started: {}\n", case.started_at));
    s.push_str(&format!("Last activity: {}\n", case.last_activity_at));
    if let Some(summary) = case.summary.as_deref() {
        if !summary.trim().is_empty() {
            s.push_str(&format!("Summary: {}\n", summary.trim()));
        }
    }
    s.push_str(
        "\nThis conversation is part of a multi-session case. Reference prior decisions, build on past artifacts, do NOT restart from scratch. If the user pivots to unrelated work, ask whether to close this case or branch a new one.\n\n",
    );
    s
}

/// Match the sanitization used by the interpreter window when it
/// mounts attached files into Pyodide's /inputs/ — keep the two in
/// lock-step so the LLM's run_python code uses the right paths.
/// Source: src/interpreter/main.tsx (`safeName = name.replace(/[^A-Za-z0-9._-]/g, "_")`).
fn sanitize_filename_for_mount(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-' { c } else { '_' })
        .collect()
}

fn today_local() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = now / 86400;
    let (y, m, d) = days_to_ymd(days);
    format!("{y:04}-{m:02}-{d:02}")
}

fn days_to_ymd(days_since_epoch: i64) -> (i32, u32, u32) {
    let mut days = days_since_epoch + 719468;
    let era = if days >= 0 { days / 146097 } else { (days - 146096) / 146097 };
    days -= era * 146097;
    let yoe = (days - days / 1460 + days / 36524 - days / 146096) / 365;
    let y = (yoe + era * 400) as i32;
    let doy = days - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

fn strip_code_fences(s: &str) -> &str {
    let trimmed = s.trim();
    if let Some(rest) = trimmed.strip_prefix("```json").or_else(|| trimmed.strip_prefix("```")) {
        let rest = rest.trim_start_matches('\n');
        if let Some(end) = rest.rfind("```") {
            return rest[..end].trim();
        }
    }
    trimmed
}

fn parse_extraction(raw: &str) -> Result<Extraction, serde_json::Error> {
    let cleaned = strip_code_fences(raw);
    let value: Value = serde_json::from_str(cleaned)?;
    serde_json::from_value::<Extraction>(value)
}

/// JSON Schema for the `report_extraction` tool — mirrors the `Extraction`
/// struct. Provider-validated, so the model can't return malformed shapes.
///
/// `action_kinds` is the live action registry's kinds (so pack-supplied
/// handlers like `propose_invoice_draft` are valid only when the pack is
/// enabled). `entity_kinds` is the union of every enabled pack's declared
/// kinds; each becomes a pluralised bucket under `entities` in the schema.
fn build_extraction_tool(action_kinds: &[&str], entity_kinds: &[&str]) -> ToolDef {
    let entity_props: serde_json::Map<String, serde_json::Value> = entity_kinds
        .iter()
        .map(|k| {
            (
                format!("{k}s"),
                serde_json::json!({
                    "type": "array",
                    "items": { "type": "string" }
                }),
            )
        })
        .collect();
    let entity_props = serde_json::Value::Object(entity_props);

    let action_kind_enum: Vec<serde_json::Value> = action_kinds
        .iter()
        .map(|k| serde_json::Value::String((*k).to_string()))
        .collect();

    ToolDef {
        name: "report_extraction".into(),
        description: "Report your structured response: a brief conversational reply for the user, plus any tasks/entities/reminders/etc you extracted from their note. Always call this tool exactly once.".into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "intent": {
                    "type": "string",
                    "enum": ["operational", "conversational"],
                    "description": "operational if the turn produces structured output (tasks/reminders/entities/proposedActions); conversational if it's pure chat or a pure question."
                },
                "response": {
                    "type": "string",
                    "minLength": 1,
                    "description": "REQUIRED. Your reply to the user. PAST TENSE ONLY: report what you DID, not what you're going to do. BANNED phrases: 'I'll generate', 'I'll create', 'I'll extract', 'reading them now', 'let me check', 'working on it', 'give me a moment', 'I'll come back', 'captured', 'noted', 'got it'. If you find yourself writing one of those, STOP — go call the tool you were about to describe, then come back and report the RESULT. Examples of acceptable replies: 'Generated invoice 2026217002 — total $15,000 over 10 days (link below). I assumed the IS 217 default rate of $1,500/day from the services catalog; let me know if that needs adjustment.' / 'Pulled 14 service dates from the master sheet for IS 217 in the 03/23-06/25 window. Need the unit price to finish the line items — is it the catalog default ($1,500/day) or something else?' / 'I'd send this email but Gmail isn't connected — should I draft to clipboard?'"
                },
                "tasks": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "title": { "type": "string" },
                            "dueAt": { "type": ["string", "null"], "description": "YYYY-MM-DD or null" },
                            "priority": { "type": ["integer", "null"], "enum": [-1, 0, 1, null] },
                            "notes": { "type": ["string", "null"] }
                        },
                        "required": ["title"]
                    }
                },
                "entities": {
                    "type": "object",
                    "properties": entity_props
                },
                "reminders": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "text": { "type": "string" },
                            "remindAt": { "type": ["string", "null"], "description": "YYYY-MM-DD HH:MM or null" }
                        },
                        "required": ["text"]
                    }
                },
                "completedTaskIds": {
                    "type": "array",
                    "items": { "type": "integer" },
                    "description": "Integer ids from the OPEN TASKS list that this turn implies are now done."
                },
                "clarifyingQuestions": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Up to 2 short questions if the note is genuinely ambiguous."
                },
                "capabilityGaps": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "capability": { "type": "string", "description": "Verb-phrase like 'send email'" },
                            "context": { "type": ["string", "null"] }
                        },
                        "required": ["capability"]
                    }
                },
                "proposedActions": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "kind": {
                                "type": "string",
                                "enum": action_kind_enum
                            },
                            "rationale": { "type": "string", "description": "Short human-readable explanation shown verbatim on the confirm card." },
                            "params": {
                                "type": "object",
                                "description": "Kind-specific params. defer_task: { taskId, newDueAt }. propose_invoice_draft: { coachName, periodStart, periodEnd, schoolName?, hoursTotal?, rateCents? }. set_reminder: { text, remindAt, kind? }. write_clipboard: { text }. run_shell_command: { command, workingDir?, timeoutSeconds? }. send_email: { to, subject, body, provider?, relatedKind?, relatedId? }. update_profile_context: { contextBlurb?, communicationStyle? }. create_initiative: { name, summary?, ownerKind? ('user'|'travis'|'external'), ownerLabel?, lastDecision?, openQuestions? } — propose when the user names or implies a multi-session push (project, campaign, audit, bid). close_initiative: { initiativeId } — propose when the user signals a push is done."
                            }
                        },
                        "required": ["kind", "rationale", "params"]
                    }
                },
                "workspaceRouting": {
                    "type": ["object", "null"],
                    "description": "Which workspace this capture should land in. Pick from the WORKSPACE OPTIONS block in the user message. Set targetSlug to the slug of the best fit (or null to stay in the active workspace), confidence to high/medium when an entity match or pack vocabulary clearly indicates the world, and low when uncertain. Sensitive workspaces (health/therapy/legal/finance) must NEVER be auto-routed into — only set them as a target with confidence=low so Travis can ask the user.",
                    "properties": {
                        "targetSlug": { "type": ["string", "null"] },
                        "confidence": { "type": ["string", "null"], "enum": ["high", "medium", "low", null] },
                        "rationale": { "type": ["string", "null"] }
                    }
                },
                "genericEntities": {
                    "type": "array",
                    "description": "Names mentioned in the note that don't fit any of the pack-declared entity kinds above (coaches/schools/depts/tutors/students). Use this for any other proper noun — a person's first name, a place name, an organisation, a company. Travis records every mention silently; this is what we'll later let the user categorise. Use 'person' for individuals, 'place' for locations or sites, 'org' for companies / agencies / departments. Skip names already captured under a pack-declared bucket above.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string", "description": "The proper noun as the user wrote it (preserve casing)." },
                            "kind": { "type": "string", "enum": ["person", "place", "org"] },
                            "contextSnippet": { "type": ["string", "null"], "description": "Optional short snippet showing where in the note the name appeared." }
                        },
                        "required": ["name", "kind"]
                    }
                },
                "affectSignals": {
                    "type": ["object", "null"],
                    "description": "Light operational read on the note's emotional register (BRAIN.md capability #7). Tone: one of 'concerned'|'energised'|'drained'|'stuck'|'neutral' — your honest summary, NOT a pop-psych label. Themes: 1-3 short phrases naming worries/topics the user is returning to (e.g. 'the audit response', 'PS498 hours'). LEAVE NULL when the note is a pure ops capture with no emotional content — this is observational only, never therapeutic. Travis surfaces patterns sparingly; over-extraction here is worse than under-extraction.",
                    "properties": {
                        "tone": { "type": ["string", "null"], "enum": ["concerned", "energised", "drained", "stuck", "neutral", null] },
                        "themes": { "type": "array", "items": { "type": "string" }, "description": "1-3 short phrases. Skip if nothing recurring." }
                    }
                },
                "hypotheses": {
                    "type": "array",
                    "description": "Short notes-to-self you want to remember across the next few turns of THIS conversation. Hypothesis-grade only — guesses you'd refine as more evidence comes in, NOT facts (facts go in entityFacts and get persisted permanently). Each lives 30 minutes in working memory. Example: { topic: 'PS498 invoice scope', note: 'Looks like Data + Leadership coaching only; waiting on confirmation', confidence: 'medium' }. Use sparingly — one or two per turn at most.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "topic": { "type": "string", "description": "Short tag — what this hypothesis is about." },
                            "note": { "type": "string", "description": "The hypothesis itself, in your own voice." },
                            "confidence": { "type": "string", "enum": ["high", "medium", "low"] }
                        },
                        "required": ["topic", "note"]
                    }
                },
                "entityFacts": {
                    "type": "array",
                    "description": "Typed facts you learned about specific entities in this note — role, relationship, contact, context, preference, etc. ONLY include facts that are stated or strongly implied. Travis persists each as a 'claim' so it survives across sessions. Skip mere mentions — those are already in entities/genericEntities. Examples: { entity: 'Maria', predicate: 'role', value: 'math coach at PS 142', confidence: 'high' }; { entity: 'Carlos', predicate: 'preference', value: 'prefers email over phone', confidence: 'medium' }.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "entity": { "type": "string", "description": "The entity this fact is about — name as the user wrote it." },
                            "otherEntity": { "type": ["string", "null"], "description": "Optional second entity for relational facts (e.g. 'Maria coaches at PS 142' → entity=Maria, otherEntity=PS 142)." },
                            "predicate": { "type": "string", "description": "Short slug — 'role', 'relationship', 'contact', 'context', 'preference', etc. Invent new ones when needed." },
                            "value": { "type": "string", "description": "The fact's value as a short phrase." },
                            "confidence": { "type": "string", "enum": ["high", "medium", "low"], "description": "high = explicitly stated, medium = strongly implied, low = single weak signal." }
                        },
                        "required": ["entity", "predicate", "value"]
                    }
                },
                "workflowOps": {
                    "type": "array",
                    "description": "Workflow state transitions for the dialogue manager. Use these to drive multi-turn outputs (invoice generation, sign-in sheet curation, contract drafting). When the user states intent that matches an available recipe (see the WORKFLOW catalog block — or ACTIVE WORKFLOW if one is in flight), emit {kind:'start', recipe:'<recipe_name>', intent:'<user's words>'}. When the user supplies a piece of info that fills a slot on the active workflow, emit {kind:'fillSlot', slotName:'<slot>', value:<any JSON>, source:'user_typed'|'graph_resolved'|'extracted'|'user_dropped'}. When all required slots are filled and you've proposed the finalize action, emit {kind:'complete'}. When the user changes subject mid-workflow, emit {kind:'abandon', reason:'<one line>'}. Only emit ops that match the current ACTIVE WORKFLOW state.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "kind": { "type": "string", "enum": ["start", "fillSlot", "complete", "abandon"] },
                            "recipe": { "type": ["string", "null"], "description": "Required for kind=start; name of the recipe." },
                            "slotName": { "type": ["string", "null"], "description": "Required for kind=fillSlot; the slot's stable name from the recipe." },
                            "value": { "description": "Required for kind=fillSlot; any JSON shape the slot expects (entity {id,name}, document {id,kind}, date string, money cents int, etc)." },
                            "source": { "type": ["string", "null"], "enum": ["user_typed", "graph_resolved", "extracted", "user_dropped", null] },
                            "intent": { "type": ["string", "null"], "description": "Optional for kind=start; one-line intent in user's words." },
                            "reason": { "type": ["string", "null"], "description": "Optional for kind=abandon; one-line why." }
                        },
                        "required": ["kind"]
                    }
                },
                "thinking": {
                    "type": ["string", "null"],
                    "description": "v0.14.0 — your concise inner reasoning, 2-4 sentences, shown to the user in a collapsible 'Thinking' section. Write it like Claude: what you understood about the request, what you noticed in any attached document, what you're planning to do next, any constraint or ambiguity worth flagging. Be plain-spoken first person ('I'm seeing...', 'I need to...', 'Before I build it, I should...'). Leave null only for purely conversational greetings or acks."
                },
                "documentClassifications": {
                    "type": "array",
                    "description": "v0.19.3 — when documents are attached, ALWAYS emit a classification for each: kind (po, wo, signed_sheet, invoice, contract, sample_invoice, …), optional linked entity (linkedEntityKind + linkedEntityName — e.g. {kind:'school', name:'IS 217'}), and optional period (start/end ISO dates). The agent loop applies kind via set_document_kind and links via document_link. Without this the doc stays kind='file' and the Manage > Documents tab can't group by type. Example for an attached PO: {documentId: 3, kind: 'po', linkedEntityKind: 'school', linkedEntityName: 'IS 217', periodStart: '2026-03-23', periodEnd: '2026-06-25'}.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "documentId": { "type": "integer" },
                            "kind": { "type": "string", "description": "po | wo | signed_sheet | invoice | contract | sample_invoice | other" },
                            "linkedEntityKind": { "type": ["string", "null"], "description": "Spine entity kind (school, contract, engagement, coach)." },
                            "linkedEntityName": { "type": ["string", "null"], "description": "Display name of the linked entity. The agent loop resolves to entity_id." },
                            "periodStart": { "type": ["string", "null"], "description": "ISO date YYYY-MM-DD for the start of any period the doc covers." },
                            "periodEnd": { "type": ["string", "null"] }
                        },
                        "required": ["documentId", "kind"]
                    }
                },
                "coachHours": {
                    "type": "array",
                    "description": "v0.19.3 — when a signing sheet (or any source listing coach work) is attached, emit one row per (coach, school, date, hours) tuple. The agent loop ensures the coach + school exist (auto-creates if not) and inserts a coach_hours row linked to both. Example: {coachName:'Maria Santos', schoolName:'IS 217', sessionDate:'2026-03-17', hours:6.0, linkedSigningSheetDocId:4}. This is how the coach_hours table fills up from sign-in sheet uploads.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "coachName": { "type": "string" },
                            "schoolName": { "type": "string" },
                            "sessionDate": { "type": "string", "description": "ISO YYYY-MM-DD." },
                            "hours": { "type": "number", "description": "Decimal hours." },
                            "linkedSigningSheetDocId": { "type": ["integer", "null"] }
                        },
                        "required": ["coachName", "schoolName", "sessionDate", "hours"]
                    }
                },
                "engagementEnrichments": {
                    "type": "array",
                    "description": "v0.19.4 — when a PO / WO / scope doc is read and reveals business terms about an engagement (contract reference, activity period, ceiling dollars), emit one entry here. The pack updates the matching engagement row's contract_ref / period / school_year / etc. Engagement is matched by case-insensitive name. Example: {engagementName:'IS 217 Leadership Coaching', contractRef:'QR179CF', periodStart:'2026-03-23', periodEnd:'2026-06-25', ceilingCents:1500000, schoolYear:'2025-26'}.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "engagementName": { "type": "string" },
                            "contractRef": { "type": ["string", "null"] },
                            "periodStart": { "type": ["string", "null"] },
                            "periodEnd": { "type": ["string", "null"] },
                            "ceilingCents": { "type": ["integer", "null"] },
                            "schoolYear": { "type": ["string", "null"] }
                        },
                        "required": ["engagementName"]
                    }
                },
                "invoiceDrafts": {
                    "type": "array",
                    "description": "v0.19.4 — whenever you generate an invoice (via run_python or otherwise), ALSO emit a draft row here so it appears in the Invoices tab as status='draft'. The pack creates the row; you do NOT need to commit 'sent' or 'paid' status — that stays user-driven. Include the generatedDocId pointing at the PDF doc id Travis just registered, so the invoice row links to the file. Example: {number:'LTE2026217002', recipient:'IS 217 School of Performing Arts, 977 Fox St Rm 129, Bronx NY 10459', schoolName:'IS 217', periodStart:'2026-03-17', periodEnd:'2026-05-26', hoursTotal:10, rateCents:150000, amountCents:1500000, generatedDocId:7, notes:'Includes 03/17 per CEO permission'}.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "number": { "type": "string" },
                            "recipient": { "type": "string" },
                            "schoolName": { "type": ["string", "null"] },
                            "coachName": { "type": ["string", "null"] },
                            "periodStart": { "type": "string" },
                            "periodEnd": { "type": "string" },
                            "hoursTotal": { "type": "number" },
                            "rateCents": { "type": "integer" },
                            "amountCents": { "type": "integer" },
                            "generatedDocId": { "type": ["integer", "null"] },
                            "notes": { "type": ["string", "null"] }
                        },
                        "required": ["number", "recipient", "periodStart", "periodEnd", "amountCents"]
                    }
                },
                "packMemories": {
                    "type": "array",
                    "description": "v0.19.0 — RULES, PREFERENCES, CONSTRAINTS, FACTS, or CORRECTIONS the user established in this turn that you should remember beyond this conversation. PROACTIVELY pick these out — don't wait to be asked to remember. Examples: user says 'never include March 17 dates for IS 217' → emit {packSlug:'lead-to-empower', kind:'constraint', targetKind:'school', content:'Never include 03/17 service dates for IS 217 — pre-PO window'}. User says 'Taylor prefers Net 30' → emit {packSlug:'lead-to-empower', kind:'preference', content:'Taylor prefers Net 30 payment terms'}. User corrects you ('the school is IS 217, not Performing Arts') → emit {packSlug:'lead-to-empower', kind:'correction', targetKind:'school', content:'School is named IS 217 (not Performing Arts — that is the PO deliver-to label only)'}. Dense, specific, one memory per array entry. Pack scope MUST match an enabled pack slug.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "packSlug": { "type": "string", "description": "Slug of the pack this memory belongs to (e.g. 'lead-to-empower')." },
                            "kind": { "type": "string", "enum": ["rule", "preference", "constraint", "fact", "correction"], "description": "Memory category. Default 'rule'." },
                            "targetKind": { "type": ["string", "null"], "description": "Optional spine entity kind to scope this memory ('school', 'contract', 'engagement', 'coach', ...). Required if targetId is set." },
                            "targetId": { "type": ["integer", "null"], "description": "Optional spine entity id paired with targetKind." },
                            "content": { "type": "string", "description": "The memory text. Dense and specific." }
                        },
                        "required": ["packSlug", "content"]
                    }
                }
            },
            "required": ["intent", "response"]
        }),
    }
}

// ---------------------------------------------------------------------------
// Heuristic fast-path — skip the LLM for greetings and direct task ops.
//
// Trades a tiny amount of capture-time latency (a regex match) for
// zero-token, zero-latency handling of low-information turns. Conservative
// by design: we only match patterns where the intent is unambiguous, so
// the fall-through to the LLM stays the default path.
// ---------------------------------------------------------------------------

/// Pure-greeting patterns. Matched as the entire input (after trim and
/// trailing-punctuation strip), so "good morning, lots to do today"
/// falls through to the LLM.
const GREETINGS: &[&str] = &[
    "hi", "hello", "hey", "yo", "sup", "howdy",
    "morning", "good morning",
    "afternoon", "good afternoon",
    "evening", "good evening",
    "good night", "night",
];

/// Pure-acknowledgment patterns — also matched as the entire input.
const ACKS: &[&str] = &[
    "ok", "okay", "k", "kk",
    "thanks", "thank you", "thx", "ty",
    "got it", "cool", "nice", "great",
];

/// Strip trailing punctuation/emoji-ish chars to normalise short turns
/// like "hi." / "thanks!" / "okay 👍".
fn normalise_short(s: &str) -> String {
    s.trim()
        .trim_end_matches(|c: char| {
            !c.is_alphanumeric() && c != ' '
        })
        .trim()
        .to_lowercase()
}

/// Friendly response to a greeting. Time-of-day aware so "morning"
/// gets a morning reply but "hey" stays generic.
fn greet_response(greeting: &str, first_name: &str) -> String {
    let name_part = if first_name.is_empty() {
        String::new()
    } else {
        format!(", {first_name}")
    };
    if greeting.contains("morning") {
        format!("Morning{name_part}. Anything to capture?")
    } else if greeting.contains("afternoon") {
        format!("Afternoon{name_part}. What's up?")
    } else if greeting.contains("evening") || greeting.contains("night") {
        format!("Evening{name_part}. Wrap-up notes, or fresh thinking?")
    } else {
        format!("Hey{name_part}. What's on your mind?")
    }
}

fn ack_response() -> String {
    "👌".to_string()
}

/// Parse "done 12" / "done 12, 13" / "complete 5" / "finished 3 7" /
/// "12 done" / "mark 4 done" into a list of task ids. Returns None
/// when the input doesn't look like a completion command.
fn parse_completion_command(lower: &str) -> Option<Vec<i64>> {
    // Strip trailing punctuation.
    let s = lower
        .trim_end_matches(|c: char| !c.is_alphanumeric() && c != ' ' && c != ',')
        .trim();
    if s.is_empty() {
        return None;
    }
    // Tokens: split on whitespace and commas.
    let tokens: Vec<&str> = s
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|t| !t.is_empty())
        .collect();
    if tokens.is_empty() || tokens.len() > 8 {
        return None;
    }

    // Recognise a completion verb anywhere in the leading 1-3 tokens
    // and an optional "done" trailing token. Keep numerics from the
    // remainder as ids.
    let verbs: &[&str] = &[
        "done", "complete", "completed", "finish", "finished", "close", "closed",
        "did", "mark",
    ];
    let leading_verb = verbs.iter().any(|v| tokens[0] == *v);
    let trailing_done = tokens.last().map(|t| *t == "done").unwrap_or(false);
    if !leading_verb && !trailing_done {
        return None;
    }

    let mut ids = Vec::new();
    for t in &tokens {
        // Skip the verb words themselves.
        if verbs.contains(t) || *t == "task" || *t == "tasks" || *t == "#" {
            continue;
        }
        // Strip a leading "#" so "done #12" works.
        let trimmed = t.trim_start_matches('#');
        match trimmed.parse::<i64>() {
            Ok(n) if n > 0 => ids.push(n),
            _ => {
                // A non-numeric, non-verb token means this isn't a
                // pure completion command — fall through to LLM.
                return None;
            }
        }
    }
    if ids.is_empty() {
        None
    } else {
        Some(ids)
    }
}

/// Try the fast-path. Returns a synthetic [`Extraction`] when the
/// input is unambiguously low-information (greeting, ack, direct task
/// completion). Otherwise returns None and the caller proceeds to the
/// LLM.
fn try_fast_path(
    raw: &str,
    open_ids: &std::collections::HashSet<i64>,
    first_name: &str,
) -> Option<Extraction> {
    let normalised = normalise_short(raw);
    if normalised.is_empty() {
        return None;
    }

    if GREETINGS.iter().any(|g| *g == normalised) {
        return Some(Extraction {
            intent: "conversational".into(),
            response: Some(greet_response(&normalised, first_name)),
            ..Default::default()
        });
    }
    if ACKS.iter().any(|a| *a == normalised) {
        return Some(Extraction {
            intent: "conversational".into(),
            response: Some(ack_response()),
            ..Default::default()
        });
    }

    // Completion commands need at least one matching open task — if
    // the user says "done 99" but 99 isn't open, fall through so the
    // LLM can catch a possible typo or context.
    if let Some(ids) = parse_completion_command(&normalised) {
        let valid: Vec<i64> = ids
            .iter()
            .copied()
            .filter(|id| open_ids.contains(id))
            .collect();
        if !valid.is_empty() && valid.len() == ids.len() {
            return Some(Extraction {
                intent: "operational".into(),
                response: None,
                completed_task_ids: valid,
                ..Default::default()
            });
        }
    }

    None
}

/// Upsert a `mentioned_with` edge between two entities. Bumps the
/// co_mention_count in attributes_json on existing edges; otherwise
/// creates a fresh one with count=1. Caller passes ids in canonical
/// order (a < b) so we don't end up with two edges per pair.
async fn upsert_co_mention(
    pool: &sqlx::SqlitePool,
    workspace_id: i64,
    a: i64,
    b: i64,
    journal_entry_id: i64,
) -> anyhow::Result<()> {
    use crate::spine::relation;

    if let Some(existing) =
        relation::find_edge(pool, workspace_id, a, b, "mentioned_with").await?
    {
        // Parse the existing count, increment it.
        let parsed: serde_json::Value = existing
            .attributes_json
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_else(|| serde_json::json!({}));
        let prev_count = parsed
            .get("co_mention_count")
            .and_then(|v| v.as_i64())
            .unwrap_or(1);
        let updated = serde_json::json!({
            "co_mention_count": prev_count + 1,
            "first_seen_journal_entry_id": parsed
                .get("first_seen_journal_entry_id")
                .cloned()
                .unwrap_or_else(|| serde_json::json!(journal_entry_id)),
            "last_seen_journal_entry_id": journal_entry_id,
        })
        .to_string();
        relation::update_attributes(pool, existing.id, &updated).await?;
    } else {
        let attrs = serde_json::json!({
            "co_mention_count": 1,
            "first_seen_journal_entry_id": journal_entry_id,
            "last_seen_journal_entry_id": journal_entry_id,
        })
        .to_string();
        relation::link(
            pool,
            relation::LinkParams {
                from_entity: a,
                to_entity: b,
                kind: "mentioned_with",
                pack_slug: None,
                attributes_json: Some(&attrs),
                workspace_id,
            },
        )
        .await?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Intent router — classifies a turn cheaply so we can trim retrieval
// for clearly-not-a-question captures. The memory::retrieve call
// embeds the query and scans every stored embedding; skipping it on
// pure narration saves both wall-clock latency and the fastembed
// CPU spike. Conservative classification: anything ambiguous stays
// in the "needs retrieval" bucket.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Intent {
    /// Looks like a question or a memory lookup. Needs full retrieval.
    Query,
    /// Looks like a pure note / capture. Memory retrieval skippable.
    Capture,
    /// Anything we can't confidently classify. Treat as Query for
    /// safety — better to do the embed call than starve the LLM of
    /// context on a real question.
    Ambiguous,
}

impl Intent {
    pub(crate) fn needs_memory_retrieval(self) -> bool {
        matches!(self, Intent::Query | Intent::Ambiguous)
    }
}

const QUESTION_STARTERS: &[&str] = &[
    "what", "when", "where", "who", "why", "how",
    "did", "do", "does", "is", "are", "was", "were",
    "can", "could", "should", "would", "will",
    "have", "has", "had",
    "tell", "show", "list", "find", "search",
    "remind",
];

/// Heuristic intent classifier. Fast (string ops only) so it runs on
/// every turn without measurable cost. The embedding-based variant
/// (per ROADMAP Phase 3) is a future upgrade once telemetry shows
/// where the heuristic mis-classifies.
pub(crate) fn classify_intent(raw: &str) -> Intent {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Intent::Ambiguous;
    }

    // A question mark anywhere is a strong query signal.
    if trimmed.contains('?') {
        return Intent::Query;
    }

    // First word check — questions usually open with a question word.
    let first_word = trimmed
        .split(|c: char| c.is_whitespace() || c == ',' || c == '.')
        .next()
        .unwrap_or("")
        .to_lowercase();
    if QUESTION_STARTERS.contains(&first_word.as_str()) {
        return Intent::Query;
    }

    // Long captures (3+ sentences or 30+ words) are clearly notes.
    let word_count = trimmed.split_whitespace().count();
    let sentence_endings = trimmed
        .chars()
        .filter(|c| matches!(c, '.' | '!'))
        .count();
    if word_count >= 30 || sentence_endings >= 3 {
        return Intent::Capture;
    }

    // Short narrative cues: past-tense / capture verbs in the first
    // few tokens. Conservative — only the most common ones.
    const CAPTURE_VERBS: &[&str] = &[
        "met", "called", "spoke", "talked", "saw", "visited",
        "finished", "completed", "wrapped", "sent", "drafted",
        "logged", "noted", "captured",
    ];
    let leading_tokens: Vec<String> = trimmed
        .split_whitespace()
        .take(3)
        .map(|t| t.to_lowercase())
        .collect();
    if leading_tokens
        .iter()
        .any(|t| CAPTURE_VERBS.iter().any(|v| t.starts_with(*v)))
    {
        return Intent::Capture;
    }

    Intent::Ambiguous
}

#[cfg(test)]
mod intent_tests {
    use super::*;

    #[test]
    fn questions_classify_as_query() {
        for input in &[
            "What did I capture about Maria last week?",
            "When did I last meet Carlos",
            "How many hours have I logged this month?",
            "Show me all open invoices",
            "Find anything related to PS 142",
            "Remind me about the meeting",
        ] {
            assert_eq!(classify_intent(input), Intent::Query, "{input:?}");
        }
    }

    #[test]
    fn captures_classify_as_capture() {
        for input in &[
            "Met with Maria today. Discussed her March hours at PS 142.",
            "Called Carlos. He wants to push the session to Friday.",
            "Finished the invoice draft for January.",
            "Logged 3 hours at Bronx Science yesterday.",
        ] {
            assert_eq!(classify_intent(input), Intent::Capture, "{input:?}");
        }
    }

    #[test]
    fn ambiguous_short_inputs_stay_ambiguous() {
        for input in &["Maria", "follow up", "PS 142 invoice"] {
            assert_eq!(classify_intent(input), Intent::Ambiguous, "{input:?}");
        }
    }

    #[test]
    fn long_inputs_classify_as_capture_even_without_verb() {
        let input = "Some very long capture with lots of detail about \
                     several things that happened and a few people and \
                     places and notes that span several sentences. \
                     Another sentence here. And one more.";
        assert_eq!(classify_intent(input), Intent::Capture);
    }
}

#[cfg(test)]
mod fast_path_tests {
    use super::*;
    use std::collections::HashSet;

    fn open_set(ids: &[i64]) -> HashSet<i64> {
        ids.iter().copied().collect()
    }

    #[test]
    fn matches_pure_greetings() {
        let opens = open_set(&[]);
        for input in &["hi", "Hi.", "hello!", "Good morning", "morning,", "hey"] {
            let r = try_fast_path(input, &opens, "Mike");
            assert!(r.is_some(), "should match greeting: {input:?}");
            let e = r.unwrap();
            assert_eq!(e.intent, "conversational");
            assert!(e.response.as_ref().map(|s| !s.is_empty()).unwrap_or(false));
        }
    }

    #[test]
    fn skips_long_greetings() {
        let opens = open_set(&[]);
        for input in &[
            "good morning, lots to do today",
            "hey can you remind me about the invoice",
            "hi maria texted",
        ] {
            assert!(
                try_fast_path(input, &opens, "Mike").is_none(),
                "should fall through: {input:?}"
            );
        }
    }

    #[test]
    fn matches_acknowledgments() {
        let opens = open_set(&[]);
        for input in &["thanks", "thx", "ok", "okay!", "got it"] {
            assert!(
                try_fast_path(input, &opens, "").is_some(),
                "should match ack: {input:?}"
            );
        }
    }

    #[test]
    fn parses_completion_commands() {
        let opens = open_set(&[5, 12, 13]);
        let cases = &[
            ("done 12", vec![12]),
            ("done 12, 13", vec![12, 13]),
            ("complete 5", vec![5]),
            ("finished 5 12", vec![5, 12]),
            ("mark 12 done", vec![12]),
            ("done #5", vec![5]),
        ];
        for (input, expected) in cases {
            let r = try_fast_path(input, &opens, "");
            assert!(r.is_some(), "should match: {input:?}");
            let e = r.unwrap();
            assert_eq!(e.intent, "operational");
            assert_eq!(&e.completed_task_ids, expected, "for input {input:?}");
        }
    }

    #[test]
    fn rejects_completion_when_id_not_open() {
        // Fast-path requires every parsed id to be in the open set —
        // otherwise the LLM is better placed to reason about typos.
        let opens = open_set(&[5]);
        assert!(try_fast_path("done 99", &opens, "").is_none());
        assert!(try_fast_path("done 5 99", &opens, "").is_none());
    }

    #[test]
    fn rejects_anything_with_extra_words() {
        let opens = open_set(&[5]);
        // Extra context means the LLM should handle this — could be a
        // task creation, a question, etc.
        for input in &[
            "done 5 and need to follow up on Maria",
            "complete the invoice for PS 142",
            "marked 5 done because Maria called",
        ] {
            assert!(
                try_fast_path(input, &opens, "").is_none(),
                "should fall through: {input:?}"
            );
        }
    }
}

fn extract_entity_hints(raw: &str) -> Vec<String> {
    // Cheap capitalized-word + multi-word noun extraction so memory retrieval
    // gets a small entity-match boost. Skips common sentence-leading words.
    let skip: &[&str] = &[
        "I", "The", "A", "An", "What", "When", "Why", "Who", "How", "Did", "Do",
        "My", "Our", "Their", "His", "Her", "Yes", "No", "OK", "Ok", "Need", "Want",
    ];
    let mut out: Vec<String> = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    for tok in raw.split(|c: char| !c.is_alphanumeric() && c != '-' && c != '\'') {
        if tok.is_empty() {
            continue;
        }
        let starts_upper = tok.chars().next().map(|c| c.is_uppercase()).unwrap_or(false);
        let is_skip = skip.iter().any(|s| s.eq_ignore_ascii_case(tok));
        if starts_upper && !is_skip {
            current.push(tok);
        } else {
            if !current.is_empty() {
                out.push(current.join(" "));
                current.clear();
            }
        }
    }
    if !current.is_empty() {
        out.push(current.join(" "));
    }
    out
}

fn format_memory(hits: &[memory::MemoryHit]) -> String {
    if hits.is_empty() {
        return "(none)".to_string();
    }
    hits.iter()
        .enumerate()
        .map(|(i, h)| {
            let date = h.created_at.split('T').next().unwrap_or(&h.created_at);
            let snippet = h.text.chars().take(180).collect::<String>();
            format!("[{}] {date} · {snippet}", i + 1)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_open_tasks(tasks: &[Task]) -> String {
    if tasks.is_empty() {
        return "(none)".to_string();
    }
    tasks
        .iter()
        .map(|t| {
            let due = t
                .due_at
                .as_deref()
                .map(|d| format!(" (due {d})"))
                .unwrap_or_default();
            format!("- [{}] {}{}", t.id, t.title, due)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[tauri::command]
pub async fn journal_ingest(
    app: AppHandle,
    state: State<'_, AppState>,
    text: String,
    conversation_id: Option<i64>,
) -> Result<JournalIngestResult, String> {
    let raw = text.trim().to_string();
    if raw.is_empty() {
        return Err("empty journal entry".into());
    }

    // Determine the conversation: continue an existing open one if provided, else open a new one.
    let conv_id: i64 = match conversation_id {
        Some(cid) => {
            let existing = conversation::fetch(&state.db.pool, cid)
                .await
                .map_err(|e| e.to_string())?;
            if existing.status == "resolved" {
                let title = raw.chars().take(60).collect::<String>();
                conversation::open(&state.db.pool, state.workspace.read().await.active_id, "journal", Some(&title))
                    .await
                    .map_err(|e| e.to_string())?
                    .id
            } else {
                cid
            }
        }
        None => {
            let title = raw.chars().take(60).collect::<String>();
            let active_ws = state.workspace.read().await.active_id;
            conversation::open(&state.db.pool, active_ws, "journal", Some(&title))
                .await
                .map_err(|e| e.to_string())?
                .id
        }
    };

    let active_ws_id = state.workspace.read().await.active_id;
    let entry_id: i64 = sqlx::query(
        "INSERT INTO journal_entry (raw_text, workspace_id) VALUES (?1, ?2)",
    )
    .bind(&raw)
    .bind(active_ws_id)
    .execute(&state.db.pool)
    .await
    .map_err(|e| e.to_string())?
    .last_insert_rowid();

    // Append the user's message to the conversation thread.
    let user_msg = conversation::append(&state.db.pool, conv_id, "user", &raw, None).await;
    // v0.17.0 — dual-write to the event log substrate. Best-effort;
    // never blocks the user-facing flow if the substrate hiccups.
    let user_msg_id = user_msg.as_ref().ok().map(|m| m.id);
    let _ = crate::events::append_or_warn(
        &state.db.pool,
        conv_id,
        crate::events::EventKind::UserMessage,
        Some(&serde_json::json!({
            "text": raw,
            "entry_id": entry_id,
        })),
        None,
        user_msg_id,
    )
    .await;

    let _ = behavioral::log_event(
        &state.db.pool,
        "journal_ingested",
        Some("journal"),
        Some(entry_id),
        None,
    )
    .await;

    let profile = state
        .db
        .user_profile()
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "no user profile yet".to_string())?;

    let api_key = match profile.llm_provider.as_str() {
        "claude" | "openai" => secrets::get_api_key(&profile.llm_provider),
        _ => None,
    };
    tracing::info!(
        "journal_ingest: provider={} has_key={}",
        profile.llm_provider,
        api_key.is_some()
    );

    // v0.14.5: model tiering disabled. Every turn gets the full
    // default model (Sonnet/Opus for Claude). Capture-style turns
    // used to drop to Haiku for cost, but that traded model quality
    // for cents — and the recurring "Travis didn't drive the
    // process" issues are partly model-quality issues. Use the
    // strongest model for every turn until we know what we're
    // willing to trade. We can re-introduce tiering once the
    // background-capture split lands and capture truly runs
    // separately.
    let intent = classify_intent(&raw);
    let chosen_model: Option<String> = profile.model.clone();
    tracing::debug!(
        "journal_ingest: intent={:?} model={:?} (tiering disabled in v0.14.5)",
        intent,
        chosen_model
    );
    let provider = llm::build(
        &profile.llm_provider,
        api_key.as_deref(),
        profile.ollama_url.as_deref(),
        chosen_model.as_deref(),
        state.http.clone(),
    )
    .map_err(|e| e.to_string())?;

    // Pull current open tasks so the LLM can detect completions.
    let ws_state = state.workspace.read().await.clone();
    let open_tasks = task::list(
        &state.db.pool,
        &ws_state,
        TaskFilter {
            status: Some("open".into()),
            link_kind: None,
            link_id: None,
        },
    )
    .await
    .map_err(|e| e.to_string())?;
    let open_ids: std::collections::HashSet<i64> = open_tasks.iter().map(|t| t.id).collect();

    // Build LLM message history from the conversation: prior assistant + user
    // turns (capped) + the current contextual user message that includes the
    // open tasks list. We don't include the just-appended user note here — it
    // goes in the contextualized user message at the end.
    let prior = conversation::messages(&state.db.pool, conv_id, Some(20))
        .await
        .map_err(|e| e.to_string())?;
    let mut messages: Vec<Message> = Vec::new();
    let take = prior.len().saturating_sub(1); // drop the just-appended user turn
    for m in prior.iter().take(take) {
        let role = match m.role.as_str() {
            "user" => Role::User,
            "assistant" => Role::Assistant,
            _ => continue,
        };
        messages.push(Message {
            role,
            content: m.content.clone(),
            ..Default::default()
        });
    }

    // Intent-aware memory retrieval. Pure capture-style inputs skip
    // the embedding scan entirely — saves the fastembed call + a
    // table scan on every "captured X today" note. Questions and
    // ambiguous inputs still get the full retrieval so the LLM has
    // grounding context. (`intent` was classified earlier for model
    // tiering; we reuse it here.)
    let entities_hint = extract_entity_hints(&raw);
    let ws_snapshot = state.workspace.read().await.clone();
    let mem_hits = if intent.needs_memory_retrieval() {
        memory::retrieve(
            &state.db.pool,
            &ws_snapshot.visible_ids,
            &raw,
            &entities_hint,
            5,
        )
        .await
        .unwrap_or_default()
    } else {
        tracing::debug!("intent=Capture, skipping memory retrieval");
        Vec::new()
    };

    // Graph-aware retrieval — names that resolve to known entities
    // get a tight summary of their recent events, mention snippets,
    // and co-mentioned entities. Cheap (indexed lookups), runs
    // alongside text retrieval, complements rather than replaces it.
    // Fires for any turn where the user wrote a proper noun, not
    // just queries — captures with named entities benefit from the
    // model knowing the prior context too.
    let mut graph_hits = if entities_hint.is_empty() {
        Vec::new()
    } else {
        memory::graph::retrieve(
            &state.db.pool,
            &ws_snapshot.visible_ids,
            &entities_hint,
        )
        .await
    };

    // Semantic-similarity fallback (BRAIN.md Phase 4.5 #1). Runs on
    // the raw user text whenever the intent could benefit from
    // grounding context (same gate as the text-embedding retrieval).
    // Catches references that aren't proper-noun-shaped: "the coach
    // who teaches PS 142", "that parent from last month", pronouns
    // resolved by surrounding conversation. Tight min_score keeps
    // precision high — better to miss than to dump unrelated
    // entities into the prompt.
    if intent.needs_memory_retrieval() {
        let seen: std::collections::HashSet<i64> =
            graph_hits.iter().map(|h| h.entity_id).collect();
        let semantic = memory::graph::retrieve_semantic(
            &state.db.pool,
            &ws_snapshot.visible_ids,
            &raw,
            3,
            0.55,
        )
        .await;
        for hit in semantic {
            if !seen.contains(&hit.entity_id) {
                graph_hits.push(hit);
            }
        }
    }

    let graph_block = memory::graph::format_for_prompt(&graph_hits);

    // Working memory block (BRAIN.md Phase 4.5 #6). Surfaces the
    // hypotheses Travis has written to itself earlier in this
    // conversation, so multi-turn reasoning can revise/firm up rather
    // than re-derive from scratch.
    let working_hypotheses = state.working_memory.for_conversation(conv_id).await;
    let working_block = memory::working::format_for_prompt(&working_hypotheses);

    // Active initiatives (BRAIN.md capability #4). When the user's
    // note touches a long-running theme, Travis picks up where the
    // last session left off — last decision, open questions, who's
    // holding — rather than re-deriving context per turn.
    let active_initiatives = crate::initiatives::list_active(
        &state.db.pool,
        &ws_snapshot.visible_ids,
        5,
    )
    .await;
    let initiatives_block = crate::initiatives::format_for_prompt(&active_initiatives);

    // v0.14.0 — active cases. Long-running multi-session work units;
    // injected so Travis can resume coherently if the user references
    // one by name.
    let active_cases = crate::cases::db::list_open(
        &state.db.pool,
        &ws_snapshot.visible_ids,
        5,
    )
    .await;
    let cases_block = crate::cases::db::format_for_prompt(&active_cases);

    // v0.16.0 — auto-detect + per-conversation case linkage.
    //
    // Parse `doc#N` markers from the raw user message early so the
    // case-detection heuristic can use them. (The doc preload block
    // further below re-uses this vector.)
    let inbound_doc_ids_for_case: Vec<i64> = raw
        .split("doc#")
        .skip(1)
        .filter_map(|s| {
            let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
            digits.parse().ok()
        })
        .collect();

    // If this conversation is already linked to an open case, surface
    // it; otherwise evaluate the three triggers (active workflow,
    // multi-doc upload ≥2, conversation depth ≥3). If at least two
    // fire, auto-open a case named after the active workflow's
    // recipe (or fall back to the first-turn user note) and link
    // this conversation to it. Best-effort: case-detection failures
    // never block the chat path.
    let current_case = build_or_resume_case(
        &state.db.pool,
        conv_id,
        ws_snapshot.active_id,
        &raw,
        &inbound_doc_ids_for_case,
        prior.len() as i64,
    )
    .await;
    let current_case_block = current_case
        .as_ref()
        .map(|c| format_active_case_block(c))
        .unwrap_or_default();

    // Workflow dialogue state ([[feedback-workflow-led]]). If a
    // workflow is in flight for this conversation, Travis sees what's
    // filled / what's still missing / what to ask next. The catalog
    // tells the LLM which recipes exist so it can emit a {start, ...}
    // op when the user states intent.
    let active_workflow = crate::workflows::state::get_active(
        &state.db.pool,
        conv_id,
    )
    .await;
    let workflow_block = crate::workflows::dialogue::format_for_prompt(active_workflow.as_ref());
    let workflow_catalog_block = if active_workflow.is_none() {
        // Only show the catalog when nothing's running — otherwise the
        // active block is the relevant context.
        let all = crate::workflows::registry::all_recipes();
        if all.is_empty() {
            String::new()
        } else {
            let mut s = String::from(
                "WORKFLOW CATALOG (emit workflowOps with kind:\"start\" + recipe name when user states matching intent):\n",
            );
            for r in all {
                s.push_str(&format!("- {} ({}): {}\n", r.display_name, r.name, r.description));
            }
            s
        }
    } else {
        String::new()
    };

    let workspace_block = crate::workspaces::prompt_context_block(
        &state.db.pool,
        ws_snapshot.active_id,
    )
    .await;

    // v0.19.0 — pack memory recall. Loads user-stated rules /
    // preferences for the enabled packs.
    // v0.19.1 — entity-scoped: also pull memories tied to entities
    // currently in scope (mentioned in the last 20 messages of this
    // conversation, looked up via spine entity name matching).
    let pack_memory_block = {
        let enabled_slugs: Vec<&str> = state
            .enabled_packs
            .iter()
            .map(|p| p.slug())
            .collect();
        let in_scope: Vec<(String, i64)> =
            crate::spine::entity::in_conversation_scope(
                &state.db.pool,
                ws_snapshot.active_id,
                conv_id,
            )
            .await
            .unwrap_or_default();
        match crate::packs::memory::recall_for_prompt(
            &state.db.pool,
            ws_snapshot.active_id,
            &enabled_slugs,
            &in_scope,
            20,
        )
        .await
        {
            Ok(memories) => crate::packs::memory::format_for_prompt(&memories),
            Err(e) => {
                tracing::warn!("pack memory recall failed: {e}");
                String::new()
            }
        }
    };

    // List the visible workspaces so the LLM can route. Sensitive
    // workspaces are excluded from this list — they're never
    // auto-routed into; the user must switch into them explicitly.
    let visible_workspaces =
        crate::workspaces::list_all(&state.db.pool).await.unwrap_or_default();
    let routable_workspaces: Vec<_> = visible_workspaces
        .iter()
        .filter(|w| !w.is_archived() && ws_snapshot.visible_ids.contains(&w.id))
        .filter(|w| !w.is_sensitive())
        .collect();
    let active_slug = visible_workspaces
        .iter()
        .find(|w| w.id == ws_snapshot.active_id)
        .map(|w| w.slug.clone())
        .unwrap_or_else(|| "personal".to_string());
    let workspace_options_block = if routable_workspaces.len() > 1 {
        let mut s =
            String::from("WORKSPACE OPTIONS (set workspaceRouting.targetSlug to one of these — current active is marked):\n");
        for w in &routable_workspaces {
            let marker = if w.slug == active_slug { " ← ACTIVE" } else { "" };
            s.push_str(&format!(
                "- {} ({}) [{}]{}\n",
                w.name, w.category, w.slug, marker
            ));
        }
        s
    } else {
        String::new()
    };

    // Doc preload (v0.14.3). When the user attached files (via doc#N
    // markers), pre-extract their content into the user message so the
    // LLM has it on iteration 1. This means it doesn't have to spend
    // a tool-call iteration on read_document just to see what's there;
    // it can spend that iteration on run_python or styling analysis
    // instead. The LLM may still call read_document for the full body
    // if our summary is missing something — this is augmentation, not
    // replacement.
    let inbound_doc_ids: Vec<i64> = raw
        .split("doc#")
        .skip(1)
        .filter_map(|s| {
            let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
            digits.parse().ok()
        })
        .collect();
    let doc_preload_block: String = if inbound_doc_ids.is_empty() {
        String::new()
    } else {
        // Wrap doc preload in a Step so the user sees Travis actively
        // working — without this, the LLM call begins with no visible
        // activity and the user perceives "thinking…" with nothing
        // happening.
        let preload_step = crate::steps::Step::start(
            &app,
            &state.db.pool,
            conv_id,
            crate::steps::StepKind::Action,
            "Reading attached documents",
            Some(format!(
                "{} doc{}",
                inbound_doc_ids.len(),
                if inbound_doc_ids.len() == 1 { "" } else { "s" }
            )),
            None,
        )
        .await
        .ok();

        let mut block = String::from("== ATTACHED DOCUMENTS (pre-extracted summary) ==\n");
        for id in &inbound_doc_ids {
            if let Ok(Some(doc)) = crate::documents::db::get(&state.db.pool, *id).await {
                if let Some(step) = preload_step.as_ref() {
                    step.note(
                        &app,
                        &state.db.pool,
                        format!("doc#{}: {}", id, doc.display_name),
                    )
                    .await;
                }
                // Spreadsheets get a TIGHT summary (filename + mime + a
                // sentence telling the LLM to use run_python with pandas).
                // Dropping a 380KB master sheet into the prompt blew up
                // the LLM's context in v0.14.4 testing and produced an
                // error; spreadsheets are meant to be processed in code,
                // not consumed as text.
                let is_spreadsheet = matches!(
                    doc.mime_type.as_str(),
                    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
                        | "application/vnd.ms-excel"
                        | "text/csv"
                ) || doc.original_filename.to_lowercase().ends_with(".xlsx")
                    || doc.original_filename.to_lowercase().ends_with(".xls")
                    || doc.original_filename.to_lowercase().ends_with(".csv");

                block.push_str(&format!(
                    "\n--- doc#{} · {} ({}) ---\n",
                    id, doc.display_name, doc.kind
                ));
                if is_spreadsheet {
                    block.push_str(&format!(
                        "Spreadsheet — mounted at /inputs/{}. Use run_python with pandas (pd.read_excel or pd.read_csv) to read it. DO NOT request the full content here; query it in Python.\n",
                        sanitize_filename_for_mount(&doc.original_filename)
                    ));
                    // Tiny structural preview to help the LLM decide what
                    // to filter on (sheet names, top-line summary). Cap
                    // tight — 400 chars max.
                    if let Some(ej) = doc.extracted_json.as_deref() {
                        let preview: String = ej.chars().take(400).collect();
                        if !preview.trim().is_empty() {
                            block.push_str("Structural preview (first 400 chars of extracted summary):\n");
                            block.push_str(&preview);
                            block.push('\n');
                        }
                    }
                } else {
                    match doc.extracted_json.as_deref() {
                        Some(ej) if !ej.trim().is_empty() => {
                            let truncated: String = ej.chars().take(2000).collect();
                            block.push_str(&truncated);
                            if ej.chars().count() > 2000 {
                                block.push_str("\n…(truncated; call read_document with this doc id for the full body)");
                            }
                        }
                        _ => {
                            block.push_str("(not yet extracted — call read_document to read it)");
                        }
                    }
                    block.push('\n');
                }
            }
        }
        block.push_str("\nThese files are also mounted at /inputs/ inside the Python interpreter — pass their doc ids to run_python.\n\n");
        if let Some(step) = preload_step {
            let _ = step
                .complete_ok(
                    &app,
                    &state.db.pool,
                    Some(format!("{} doc{} loaded", inbound_doc_ids.len(), if inbound_doc_ids.len() == 1 { "" } else { "s" })),
                )
                .await;
        }
        block
    };

    let user_msg = format!(
        "Today is {today}.\n\nOPEN TASKS (id · title):\n{open}\n\nRELEVANT MEMORY:\n{mem}\n\n{graph}{working}{initiatives}{cases}{current_case}{workflow}{catalog}{ws}{docs_preload}New turn:\n{raw}",
        today = today_local(),
        open = format_open_tasks(&open_tasks),
        mem = format_memory(&mem_hits),
        graph = if graph_block.is_empty() {
            String::new()
        } else {
            format!("{graph_block}\n")
        },
        working = if working_block.is_empty() {
            String::new()
        } else {
            format!("{working_block}\n")
        },
        initiatives = if initiatives_block.is_empty() {
            String::new()
        } else {
            format!("{initiatives_block}\n")
        },
        cases = if cases_block.is_empty() {
            String::new()
        } else {
            format!("{cases_block}\n")
        },
        current_case = current_case_block,
        workflow = if workflow_block.is_empty() {
            String::new()
        } else {
            format!("{workflow_block}\n")
        },
        catalog = if workflow_catalog_block.is_empty() {
            String::new()
        } else {
            format!("{workflow_catalog_block}\n")
        },
        ws = if workspace_options_block.is_empty() {
            String::new()
        } else {
            format!("{workspace_options_block}\n")
        },
        docs_preload = doc_preload_block,
        raw = raw
    );
    messages.push(Message::user(user_msg));

    // Heuristic fast-path: short greetings, acks, and direct task
    // completions skip the LLM entirely. Returns None on anything
    // remotely interesting; the LLM stays the default path.
    let fast_path_first_name = profile
        .name
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_string();
    let fast_path_extraction = try_fast_path(&raw, &open_ids, &fast_path_first_name);

    // Agent loop: LLM may call read-only tools (web_fetch, etc.) before
    // finalizing via `report_extraction`. We loop, dispatching tool calls and
    // feeding results back, until the model emits report_extraction or we hit
    // the iteration cap (then we force the extraction tool).
    let action_kinds: Vec<&'static str> = state.actions.kinds();
    let entity_kinds: Vec<&'static str> = state
        .enabled_packs
        .iter()
        .flat_map(|p| p.entity_kinds().iter().copied())
        .collect();
    let extraction_tool = build_extraction_tool(&action_kinds, &entity_kinds);
    let extraction_name = extraction_tool.name.clone();
    let read_registry = tools::read_only_registry(&state.enabled_packs);
    let mut tool_defs: Vec<ToolDef> = vec![extraction_tool.clone()];
    tool_defs.extend(read_registry.definitions());

    let tool_ctx = ToolContext {
        http: state.http.clone(),
        app: app.clone(),
        db: state.db.clone(),
        conversation_id: Some(conv_id),
        parent_step_id: None,
    };
    // v0.16.7: bumped 8 → 16. The PS556→IS217 invoice-derivation flow
    // hit the cap on a 5-doc turn (PO + WO + master sign-in xlsx +
    // service catalog xlsx + an existing invoice for styling). The
    // worker reasonably calls read_document on each new file, then
    // analyze_document_styling for layout match, then 2-3 run_python
    // passes to parse Excel structure → find rows → derive line items
    // — easily 10+ iterations before extraction is callable. 16 leaves
    // room for that AND the forced-extraction last iter without
    // shipping a "ran out of tool-call iterations" error.
    //
    // Earlier note (v0.14.3): bumped 4 → 8 once we moved capture
    // extraction off the primary pass. 8 was right for 1-2 doc turns;
    // not for 5.
    const MAX_ITER: usize = 16;

    // Clone the message stack before the agent loop takes ownership,
    // so the empty-response retry path below can re-run the LLM with
    // the same context (system prompt is cached, so the second call
    // shares the prefix and only pays for the forcing-message tail).
    let messages_for_retry = messages.clone();

    // v0.15.1 manager loop. Wraps the existing agent loop in an outer
    // layer that refuses to let the worker bail with a placeholder
    // reply. Up to 3 manager iterations; each iteration runs the
    // full agent loop with up to MAX_ITER tool-call rounds. Between
    // iterations, if the worker handed off without progress, we
    // append the worker's prior attempt + a continuation directive
    // and re-run.
    const MAX_MANAGER_ITER: usize = 3;
    let mut working_messages = messages;
    let mut extraction: Extraction = Default::default();
    let mut ok: bool = false;
    let mut err_msg: Option<String> = None;
    let mut raw_response: String = String::new();
    let mut manager_iter: usize = 0;

    if let Some(fp) = fast_path_extraction {
        tracing::info!("journal_ingest fast-path hit (intent={})", fp.intent);
        extraction = fp;
        ok = true;
        raw_response = "<fast-path>".to_string();
    } else { 'manager: loop {
        // Emit a visible "Thinking" step per manager pass so the
        // chat shows the same loop-doesn't-quit texture Claude.ai
        // has. First pass is "Working on it"; re-runs surface that
        // the manager is forcing progress.
        let mgr_step_name = if manager_iter == 0 {
            "Working on it".to_string()
        } else {
            format!("Forcing progress (pass {})", manager_iter + 1)
        };
        let mgr_step = crate::steps::Step::start(
            &app,
            &state.db.pool,
            conv_id,
            crate::steps::StepKind::Thinking,
            mgr_step_name,
            None,
            None,
        )
        .await
        .ok();

        let (this_ext, this_ok, this_err, this_raw) = 'outer: {
        let mut current_messages = working_messages.clone();
        let mut last_dump = String::new();
        for iter in 0..MAX_ITER {
            // Last iteration forces the extraction tool to ensure we always finalize.
            let choice = if iter == MAX_ITER - 1 {
                ToolChoice::Specific(extraction_name.clone())
            } else {
                ToolChoice::Auto
            };
            // v0.16.7: tiered thinking budget. The 4000-token blanket
            // from v0.15.2 was great for one-shot synthesis but slow
            // when applied to every mid-loop "which tool next?"
            // decision — extended thinking is roughly proportional to
            // budget in latency. Tier it:
            //
            //   iter 0          → 4000  (initial plan + first tool)
            //   iters 1..N-2    → 1500  (decide next tool given new
            //                            tool results; doesn't need
            //                            full re-derivation)
            //   iter N-1        → 4000  (forced extraction, real
            //                            synthesis happens here)
            //
            // Net: a 10-iteration turn drops from 10×4000=40,000 thinking
            // tokens to 4000 + 8×1500 + 4000 = 20,000 — roughly half
            // the latency without losing depth where it matters.
            let thinking = if iter == 0 || iter == MAX_ITER - 1 {
                4000
            } else {
                1500
            };
            let opts = ChatWithToolsOptions {
                system: Some(build_system_prompt(
                    &profile,
                    &crate::packs::prompt_fragment(&state.enabled_packs),
                    &workspace_block,
                    &pack_memory_block,
                )),
                cache_system: true,
                // v0.15.2: with extended thinking enabled, temperature
                // must be unset (or 1). The Claude provider strips it
                // automatically when `thinking_budget` is set; leaving
                // it here is harmless but redundant.
                temperature: Some(0.3),
                max_tokens: Some(8000),
                tools: tool_defs.clone(),
                tool_choice: Some(choice),
                thinking_budget: Some(thinking),
            };
            let turn_res = provider.chat_with_tools(current_messages.clone(), opts).await;
            match turn_res {
                Err(e) => {
                    let kind = crate::health::classify_llm_error(&e.to_string());
                    state.health.report(&app, kind, format!("LLM call failed: {e}"));
                    break 'outer (
                        fallback_extraction(&raw),
                        false,
                        Some(e.to_string()),
                        last_dump,
                    );
                }
                Ok(turn) => {
                    // First successful call clears whatever degraded state
                    // was set. Subsequent iterations within this same ingest
                    // hit the no-op path inside Health::clear.
                    state.health.clear(&app);

                    // v0.15.2 — extended-thinking visibility. Each
                    // `thinking` content block becomes a Note on the
                    // active manager step, so the user sees the
                    // worker's reasoning stream into the chat as it
                    // happens — the same loop-doesn't-quit texture as
                    // Claude.ai's chat surface.
                    //
                    // v0.19.0 — substantive thinking ALSO emits its
                    // own child step so it lands BETWEEN the tool
                    // calls in the chat surface (the user wanted
                    // to see Travis's reasoning between actions, not
                    // buried as a footnote on the manager step). The
                    // note-on-manager-step path stays for the
                    // collapsible reasoning archive.
                    for thought in &turn.thinking_blocks {
                        if let Some(s) = mgr_step.as_ref() {
                            let snippet: String =
                                thought.chars().take(500).collect();
                            s.note(&app, &state.db.pool, snippet).await;
                        }
                        let cleaned = thought.trim();
                        if cleaned.chars().count() >= 80 {
                            let label = summarise_thinking(cleaned);
                            let mut detail: String =
                                cleaned.chars().take(280).collect();
                            if cleaned.chars().count() > 280 {
                                detail.push('…');
                            }
                            let parent_id =
                                mgr_step.as_ref().map(|s| s.id.clone());
                            if let Ok(thinking_step) =
                                crate::steps::Step::start(
                                    &app,
                                    &state.db.pool,
                                    conv_id,
                                    crate::steps::StepKind::Thinking,
                                    label,
                                    Some(detail),
                                    parent_id,
                                )
                                .await
                            {
                                let _ = thinking_step
                                    .complete_ok(
                                        &app,
                                        &state.db.pool,
                                        None,
                                    )
                                    .await;
                            }
                        }
                    }

                    last_dump = serde_json::json!({
                        "iter": iter,
                        "content": turn.content,
                        "tool_calls": turn.tool_calls,
                        "thinking_blocks": turn.thinking_blocks.len(),
                    })
                    .to_string();

                    if let Some(call) = turn
                        .tool_calls
                        .iter()
                        .find(|c| c.name == extraction_name)
                    {
                        match serde_json::from_value::<Extraction>(call.input.clone()) {
                            Ok(ex) => break 'outer (ex, true, None, last_dump),
                            Err(e) => {
                                break 'outer (
                                    fallback_extraction(&raw),
                                    false,
                                    Some(format!("tool input parse error: {e}")),
                                    last_dump,
                                );
                            }
                        }
                    }

                    if turn.tool_calls.is_empty() {
                        // Model returned only text — try to salvage it as JSON.
                        match parse_extraction(&turn.content) {
                            Ok(ex) => break 'outer (ex, true, None, last_dump),
                            Err(e) => {
                                break 'outer (
                                    fallback_extraction(&raw),
                                    false,
                                    Some(format!("model didn't call any tool: {e}")),
                                    last_dump,
                                );
                            }
                        }
                    }

                    // Append the assistant turn (preserving its tool calls) and
                    // dispatch each call, then feed results back as tool messages.
                    current_messages.push(Message {
                        role: Role::Assistant,
                        content: turn.content,
                        tool_calls: turn.tool_calls.clone(),
                        tool_call_id: None,
                    });
                    for call in turn.tool_calls {
                        // v0.17.1 — per-tool-call step so the chat
                        // shows real-time dispatch on long turns
                        // (e.g. 20-min invoice flows). v0.18.2 —
                        // humanise the labels so the user sees
                        // "Generating file" not "run_python", and
                        // "Checking the records" not "search_memory".
                        let tool_name = humanize_tool_name(&call.name, &call.input);
                        // v0.18.2 — for document-touching tools,
                        // resolve the document id(s) to their actual
                        // filenames so the chat shows e.g.
                        // "Reading attachment — IS 217 (1).pdf"
                        // instead of leaking the integer id. Best-
                        // effort: a missing doc row falls back to a
                        // bare label.
                        let tool_detail = resolve_tool_detail(
                            &state.db.pool,
                            &call.name,
                            &call.input,
                        )
                        .await;
                        let tool_step = crate::steps::Step::start(
                            &app,
                            &state.db.pool,
                            conv_id,
                            crate::steps::StepKind::ToolCall,
                            tool_name,
                            Some(tool_detail),
                            mgr_step.as_ref().map(|s| s.id.clone()),
                        )
                        .await
                        .ok();

                        let result = match read_registry
                            .execute(&tool_ctx, &call.name, call.input.clone())
                            .await
                        {
                            Ok(s) => s,
                            Err(e) => format!("error: {e}"),
                        };
                        let truncated: String = result.chars().take(8000).collect();

                        if let Some(step) = tool_step {
                            // Surface a short snippet of the result
                            // as the step's summary so the user can
                            // see at a glance what came back.
                            let summary: String =
                                result.chars().take(140).collect();
                            let _ = step
                                .complete_ok(&app, &state.db.pool, Some(summary))
                                .await;
                        }

                        current_messages.push(Message::tool_result(call.id, truncated));
                    }
                }
            }
        }
        // Hit the cap without finalizing
        (
            fallback_extraction(&raw),
            false,
            Some("agent loop exceeded max iterations".into()),
            last_dump,
        )
        };

        // v0.15.1 manager evaluation. Stash this pass's result then
        // decide whether to stop (delivered or asked a real question)
        // or kick the worker again with a forcing directive.
        extraction = this_ext;
        ok = this_ok;
        err_msg = this_err;
        raw_response = this_raw;

        let progress = crate::manager::evaluate_progress(&extraction, &[], false);
        let progress_label = match progress {
            crate::manager::ProgressKind::Delivered => "delivered",
            crate::manager::ProgressKind::AskedBlocker => "asked specific question",
            crate::manager::ProgressKind::Handoff => "handoff — re-running",
        };
        if let Some(s) = mgr_step {
            let _ = s
                .complete_ok(&app, &state.db.pool, Some(progress_label.to_string()))
                .await;
        }
        tracing::info!(
            "manager pass {}: progress={:?} response_len={}",
            manager_iter + 1,
            progress,
            extraction.response.as_deref().map(|s| s.len()).unwrap_or(0)
        );

        match progress {
            crate::manager::ProgressKind::Delivered
            | crate::manager::ProgressKind::AskedBlocker => {
                break 'manager;
            }
            crate::manager::ProgressKind::Handoff => {
                if manager_iter + 1 >= MAX_MANAGER_ITER {
                    tracing::warn!(
                        "manager: hit cap of {} iterations without progress; surfacing last attempt",
                        MAX_MANAGER_ITER
                    );
                    break 'manager;
                }
                // v0.16.4: pick a more targeted directive when the
                // worker manufactured a Pyodide-cold excuse — the
                // generic "do the work" directive doesn't address
                // the specific hallucination.
                let prior_response = extraction.response.clone().unwrap_or_default();
                let directive = if crate::manager::is_pyodide_excuse(&prior_response) {
                    "Your previous reply claimed the Pyodide interpreter is 'still cold-loading' or 'not ready' — but you NEVER actually called run_python. That excuse is hallucinated. The interpreter pre-warms at app launch and is reliably ready in 3-5 seconds; by the time any conversation reaches your turn it is fully ready.\n\n\
                     CALL run_python NOW with your actual work code. Generate the artifact. If a real error comes back (extremely rare), THEN you may report it — but never refuse to call the tool with a manufactured 'not ready' excuse before trying.".to_string()
                } else {
                    crate::manager::continuation_directive().to_string()
                };
                // Inject the worker's prior reply + continuation
                // directive so the next agent-loop pass sees what it
                // said before and is told to actually progress.
                working_messages.push(Message {
                    role: Role::Assistant,
                    content: prior_response,
                    tool_calls: vec![],
                    tool_call_id: None,
                });
                working_messages.push(Message::user(directive));
                manager_iter += 1;
            }
        }
    }};

    let is_conversational = extraction.intent.eq_ignore_ascii_case("conversational");

    // Workflow-continuation detection.
    // v0.14.3: any capture fields the LLM emitted (despite the
    // "leave empty" prompt directive) are still persisted via the
    // existing loops below — they just NEVER appear in the chat reply.
    // The synthesis fallback no longer narrates captures, and the
    // governing-principle prompt forbids Travis from mentioning them
    // in `response`. Architectural split to a background LLM call
    // ships in v0.14.4.
    if !extraction.tasks.is_empty() || !extraction.reminders.is_empty() {
        tracing::info!(
            "primary pass produced silent capture ({} tasks, {} reminders) — persisted, not narrated",
            extraction.tasks.len(),
            extraction.reminders.len()
        );
    }

    // Apply workspace routing. The LLM may have nominated a different
    // workspace via `workspaceRouting`; we honour it for high/medium
    // confidence picks that target a non-sensitive, non-archived
    // workspace in the visible set. Anything else (sensitive target,
    // unknown slug, low confidence, archived) falls back to the
    // active workspace and the rationale becomes a clarifying ask.
    let mut dest_ws_id = active_ws_id;
    let mut routing_result: Option<RoutingResult> = None;
    if let Some(routing) = &extraction.workspace_routing {
        let target_slug = routing
            .target_slug
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let confidence = routing.confidence.as_deref().unwrap_or("low").to_lowercase();
        if let Some(slug) = target_slug {
            if let Some(target) = visible_workspaces.iter().find(|w| w.slug == slug) {
                let is_active_target = target.id == active_ws_id;
                let acceptable = !target.is_archived()
                    && !target.is_sensitive()
                    && (confidence == "high" || confidence == "medium");
                if acceptable && !is_active_target {
                    dest_ws_id = target.id;
                    routing_result = Some(RoutingResult {
                        workspace_slug: target.slug.clone(),
                        workspace_name: target.name.clone(),
                        routed: true,
                        confidence: Some(confidence.clone()),
                        rationale: routing.rationale.clone(),
                    });
                } else if target.is_sensitive() && !is_active_target {
                    // Sensitive target + not already active — never route;
                    // surface a clarifying question instead.
                    let rationale = routing
                        .rationale
                        .clone()
                        .unwrap_or_else(|| "looks like sensitive content".to_string());
                    extraction.clarifying_questions.push(format!(
                        "{rationale} — save to {}, or stay in this workspace?",
                        target.name
                    ));
                }
            }
        }
    }

    // If routing changed the destination, restamp the journal entry
    // and the conversation it lives in. Skipping conversation move
    // when the conversation already had prior turns in another
    // workspace would split-brain a thread, but this is a fresh
    // capture-driven thread so a single rewrite is fine.
    if dest_ws_id != active_ws_id {
        let _ = sqlx::query("UPDATE journal_entry SET workspace_id = ?1 WHERE id = ?2")
            .bind(dest_ws_id)
            .bind(entry_id)
            .execute(&state.db.pool)
            .await;
        let _ = sqlx::query("UPDATE conversation SET workspace_id = ?1 WHERE id = ?2")
            .bind(dest_ws_id)
            .bind(conv_id)
            .execute(&state.db.pool)
            .await;
    }

    // Build a `WorkspaceState` rooted at dest_ws_id so all downstream
    // writes (tasks, reminders) land in the routed workspace.
    let dest_ws_state = if dest_ws_id == active_ws_id {
        ws_state.clone()
    } else {
        crate::workspaces::State {
            active_id: dest_ws_id,
            visible_ids: ws_state.visible_ids.clone(),
        }
    };

    let mut created: Vec<Task> = Vec::new();
    let mut completed: Vec<Task> = Vec::new();
    let mut mention_chips: Vec<MentionChip> = Vec::new();

    // Operational pass — skipped entirely for conversational input so we never
    // manufacture todos from chit-chat.
    if !is_conversational {
        // v0.15.2: task + reminder persistence moved to the
        // background capture module. The spawn happens after the
        // assistant message is appended (further down this function)
        // so the chat path returns immediately and capture runs
        // asynchronously — never blocking or polluting the
        // conversation.
        //
        // Other capture fields (capability_gaps, entities,
        // entity_facts, hypotheses, affect_signals,
        // workspace_routing) still run inline for now; they touch
        // more shared state and are higher refactoring risk.
        // Queued for the v0.15.3 capture-module expansion.
        //
        // `created` / `completed` vectors stay declared but
        // unfilled — telemetry will show 0/0 until the background
        // pipeline learns to emit per-turn counts.

        for gap in &extraction.capability_gaps {
            let cap = gap.capability.trim();
            if cap.is_empty() {
                continue;
            }
            if let Err(e) = feedback::record(
                &state.db.pool,
                &AppFeedbackInput {
                    capability: cap.to_string(),
                    context: gap.context.clone(),
                    source_kind: Some("journal".into()),
                    source_id: Some(entry_id),
                },
            )
            .await
            {
                tracing::warn!("failed to record capability gap: {e}");
            }
        }

        if ok {
            // Record mentions for every entity kind declared by an enabled
            // pack. Bucket name in the JSON is pluralised entity kind. For
            // each successfully upserted entity we also append a
            // `mentioned` event to the spine so the entity-detail timeline
            // (slice 13) can render the mention history without a join
            // through journal_entry text. Every successfully recorded
            // entity id is also collected in `mentioned_entities` so the
            // tail of this block can write co-mention edges.
            let mut mentioned_entities: Vec<i64> = Vec::new();
            let snippet: String = {
                let mut s = String::new();
                for ch in raw.chars().take(120) {
                    s.push(ch);
                }
                if raw.chars().count() > 120 {
                    s.push_str("…");
                }
                s
            };
            for pack in &state.enabled_packs {
                let pack_slug = pack.slug();
                for kind in pack.entity_kinds() {
                    let bucket = format!("{kind}s");
                    if let Some(names) = extraction.entities.0.get(&bucket) {
                        for name in names {
                            // v0.19.3 — pack-table auto-population
                            // moved to the background capture
                            // pipeline (via PackHandle::ensure_entity)
                            // so it never blocks the chat reply.
                            // Only spine mention recording happens
                            // inline here.

                            let entity_id = identity::record_mention(
                                &state.db.pool,
                                dest_ws_id,
                                kind,
                                name,
                                identity::confidence::PACK_KINDED_AMBIENT,
                            )
                            .await;
                            if let Some(eid) = entity_id {
                                mentioned_entities.push(eid);
                                let attrs = serde_json::json!({
                                    "journal_entry_id": entry_id,
                                    "snippet": snippet,
                                })
                                .to_string();
                                if let Err(e) = crate::spine::event::record(
                                    &state.db.pool,
                                    crate::spine::event::RecordParams {
                                        entity_id: Some(eid),
                                        kind: "mentioned",
                                        pack_slug: Some(pack_slug),
                                        occurred_at: None,
                                        attributes_json: Some(&attrs),
                                        workspace_id: dest_ws_id,
                                    },
                                )
                                .await
                                {
                                    tracing::warn!(
                                        "spine event sync (mention) for entity {eid}: {e}"
                                    );
                                }
                            }
                        }
                    }
                }
            }

            // Ambient generic entities — names the LLM saw in the
            // note that didn't fit any pack-declared bucket. Before
            // creating a `<kind>:unknown` row, check whether an
            // existing entity in this workspace already matches the
            // normalised name (e.g. coach Maria); if so, dedup onto
            // that entity instead of duplicating it as person:unknown.
            for ge in &extraction.generic_entities {
                let base_kind = ge.kind.trim().to_lowercase();
                let scoped_kind = match base_kind.as_str() {
                    "person" => "person:unknown",
                    "place" => "place:unknown",
                    "org" => "org:unknown",
                    _ => continue, // schema enforces these three; skip junk silently
                };

                let (entity_id, pack_slug_for_event) = match identity::find_by_normalized_name(
                    &state.db.pool,
                    dest_ws_id,
                    &ge.name,
                )
                .await
                {
                    Some((eid, _existing_kind, existing_pack_slug)) => {
                        // Dedup onto the existing entity. We don't
                        // change kind here — the existing kind wins.
                        identity::bump_mention(&state.db.pool, eid).await;
                        (Some(eid), existing_pack_slug)
                    }
                    None => {
                        // No match — record as a fresh
                        // <kind>:unknown ambient entity.
                        let id = identity::record_mention(
                            &state.db.pool,
                            dest_ws_id,
                            scoped_kind,
                            &ge.name,
                            identity::confidence::GENERIC,
                        )
                        .await;
                        (id, None)
                    }
                };

                if let Some(eid) = entity_id {
                    mentioned_entities.push(eid);
                    let mention_snippet = ge
                        .context_snippet
                        .as_deref()
                        .map(|s| s.trim())
                        .filter(|s| !s.is_empty())
                        .unwrap_or(&snippet);
                    let attrs = serde_json::json!({
                        "journal_entry_id": entry_id,
                        "snippet": mention_snippet,
                    })
                    .to_string();
                    if let Err(e) = crate::spine::event::record(
                        &state.db.pool,
                        crate::spine::event::RecordParams {
                            entity_id: Some(eid),
                            kind: "mentioned",
                            // When dedup'd onto a pack entity, attribute
                            // the event to the owning pack so the timeline
                            // colours correctly; truly generic entities
                            // get None.
                            pack_slug: pack_slug_for_event.as_deref(),
                            occurred_at: None,
                            attributes_json: Some(&attrs),
                            workspace_id: dest_ws_id,
                        },
                    )
                    .await
                    {
                        tracing::warn!(
                            "spine event sync (generic mention) for entity {eid}: {e}"
                        );
                    }
                }
            }

            // Persist affect_signals (BRAIN.md capability #7 — wellbeing).
            // Light tone + themes pulled by the LLM, scoped to this
            // workspace. Privacy: stays in core's affect_signal table —
            // not in any pack-queryable surface, not in data exports.
            if let Some(affect) = &extraction.affect_signals {
                let tone = affect
                    .tone
                    .as_deref()
                    .map(|s| s.trim().to_lowercase())
                    .filter(|s| matches!(
                        s.as_str(),
                        "concerned" | "energised" | "drained" | "stuck" | "neutral"
                    ))
                    .unwrap_or_else(|| "neutral".to_string());
                let cleaned_themes: Vec<String> = affect
                    .themes
                    .iter()
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty())
                    .take(3)
                    .collect();
                // Skip writes for the null-shaped case (neutral + no themes) —
                // we don't want the table filling with no-signal rows.
                if tone != "neutral" || !cleaned_themes.is_empty() {
                    let themes_json = if cleaned_themes.is_empty() {
                        None
                    } else {
                        Some(serde_json::to_string(&cleaned_themes).unwrap_or_default())
                    };
                    if let Err(e) = sqlx::query(
                        "INSERT INTO affect_signal
                            (workspace_id, journal_entry_id, tone, themes_json)
                         VALUES (?1, ?2, ?3, ?4)",
                    )
                    .bind(dest_ws_id)
                    .bind(entry_id)
                    .bind(&tone)
                    .bind(&themes_json)
                    .execute(&state.db.pool)
                    .await
                    {
                        tracing::warn!("affect_signal insert: {e}");
                    }
                }
            }

            // Persist hypotheses into working memory (Phase 4.5 #6).
            // Hypothesis-grade notes-to-self for the next ~30 minutes
            // of this conversation; lost on restart, not facts.
            for h in &extraction.hypotheses {
                let topic = h.topic.trim();
                let note = h.note.trim();
                if topic.is_empty() || note.is_empty() {
                    continue;
                }
                let conf = h
                    .confidence
                    .as_deref()
                    .map(|c| c.trim().to_lowercase())
                    .filter(|c| matches!(c.as_str(), "high" | "medium" | "low"))
                    .unwrap_or_else(|| "medium".to_string());
                state
                    .working_memory
                    .record(
                        conv_id,
                        topic.to_string(),
                        note.to_string(),
                        conf,
                        mentioned_entities.clone(),
                    )
                    .await;
            }

            // Persist entity_facts as claims (BRAIN.md Phase 4.5 #2).
            // The LLM extracted typed facts about entities Travis already
            // recognised this turn; map each fact back to the matching
            // entity by case-insensitive name and write it as a claim.
            // Facts that don't match a known entity are dropped silently
            // — better to miss than to attach to the wrong row.
            for f in &extraction.entity_facts {
                let entity_id = match identity::find_by_normalized_name(
                    &state.db.pool,
                    dest_ws_id,
                    &f.entity,
                )
                .await
                {
                    Some((eid, _, _)) => eid,
                    None => continue,
                };
                let other_id = if let Some(other) = f.other_entity.as_deref() {
                    identity::find_by_normalized_name(&state.db.pool, dest_ws_id, other)
                        .await
                        .map(|(eid, _, _)| eid)
                } else {
                    None
                };
                let predicate = f.predicate.trim().to_lowercase();
                let value = f.value.trim().to_string();
                if predicate.is_empty() || value.is_empty() {
                    continue;
                }
                let confidence = f
                    .confidence
                    .as_deref()
                    .map(|c| c.trim().to_lowercase())
                    .filter(|c| matches!(c.as_str(), "high" | "medium" | "low"));
                if let Err(e) = crate::memory::claims::upsert(
                    &state.db.pool,
                    crate::memory::claims::ClaimInput {
                        workspace_id: dest_ws_id,
                        entity_id,
                        other_entity_id: other_id,
                        predicate,
                        value,
                        confidence,
                        source: Some("extraction".into()),
                        source_journal_entry_id: Some(entry_id),
                    },
                )
                .await
                {
                    tracing::warn!("claims upsert for entity {entity_id}: {e}");
                }
            }

            // Co-mention edges. Every unordered pair of mentioned
            // entities gets a `mentioned_with` relation; existing
            // edges have their co_mention_count bumped via the
            // attributes_json payload. Workspace-scoped — sensitive
            // workspaces don't share edges with non-sensitive ones.
            mentioned_entities.sort_unstable();
            mentioned_entities.dedup();
            for i in 0..mentioned_entities.len() {
                for j in (i + 1)..mentioned_entities.len() {
                    let a = mentioned_entities[i];
                    let b = mentioned_entities[j]; // canonical: a < b
                    if let Err(e) = upsert_co_mention(&state.db.pool, dest_ws_id, a, b, entry_id)
                        .await
                    {
                        tracing::warn!("co-mention edge ({a},{b}): {e}");
                    }
                }
            }

            // Capture chips. Pull display_name / kind / mentions_count
            // for every entity touched this turn; emit a chip only
            // when mentions_count > 1 (i.e. Travis recognised the
            // name from before, not a fresh-this-turn record).
            if !mentioned_entities.is_empty() {
                let placeholders = (1..=mentioned_entities.len())
                    .map(|i| format!("?{i}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                let sql = format!(
                    "SELECT id, display_name, kind, mentions_count
                     FROM entity
                     WHERE id IN ({placeholders})
                       AND archived_at IS NULL
                       AND mentions_count > 1
                     ORDER BY mentions_count DESC, last_seen DESC"
                );
                let mut q = sqlx::query_as::<_, (i64, String, String, i64)>(&sql);
                for eid in &mentioned_entities {
                    q = q.bind(eid);
                }
                match q.fetch_all(&state.db.pool).await {
                    Ok(rows) => {
                        mention_chips = rows
                            .into_iter()
                            .map(|(id, display_name, kind, mentions_count)| MentionChip {
                                entity_id: id,
                                display_name,
                                kind,
                                mentions_count,
                            })
                            .collect();
                    }
                    Err(e) => {
                        tracing::warn!("capture chip query: {e}");
                    }
                }
            }
        }
    }

    // Semantic indexing happens for both — conversational notes are still memory.
    // The embedding row inherits the journal entry's workspace_id so
    // retrieval can scope by workspace at scan time.
    if let Err(e) =
        memory::index_journal_entry(&state.db.pool, dest_ws_id, entry_id, &raw).await
    {
        tracing::warn!("failed to index journal entry #{entry_id} for semantic memory: {e}");
    }

    let extraction_value = serde_json::to_value(&extraction).unwrap_or(Value::Null);
    let extraction_record = serde_json::json!({
        "extraction": extraction_value,
        "raw": raw_response,
    })
    .to_string();

    sqlx::query(
        "UPDATE journal_entry SET extraction_json=?1, extraction_ok=?2, error_message=?3,
            provider=?4, model=?5 WHERE id=?6",
    )
    .bind(&extraction_record)
    .bind(if ok { 1 } else { 0 })
    .bind(&err_msg)
    .bind(&profile.llm_provider)
    .bind(profile.model.as_deref().unwrap_or(llm::default_model(&profile.llm_provider)))
    .bind(entry_id)
    .execute(&state.db.pool)
    .await
    .map_err(|e| e.to_string())?;

    let final_intent = if is_conversational {
        "conversational".to_string()
    } else {
        "operational".to_string()
    };

    // Persist proposed actions tied to this conversation. We only persist kinds
    // we actually know how to apply; unknown kinds get discarded with a log.
    let mut persisted_actions: Vec<ProposedAction> = Vec::new();
    if !is_conversational {
        for a in &extraction.proposed_actions {
            let kind = a.kind.trim();
            if !action_kinds.contains(&kind) {
                tracing::warn!("ignoring unsupported proposed action kind: {kind}");
                continue;
            }
            let params_str = a.params.to_string();
            match actions::record(
                &state.db.pool,
                conv_id,
                kind,
                a.rationale.as_deref(),
                &params_str,
            )
            .await
            {
                Ok(row) => persisted_actions.push(row),
                Err(e) => tracing::warn!("failed to record proposed action: {e}"),
            }
        }
    }

    // v0.19.3 — document classification + coach_hours persistence
    // ran inline here in earlier drafts; both now live in the
    // background capture pipeline (capture::run_background) via
    // PackHandle::apply_extraction_observations. Core stays
    // pack-agnostic; pack code owns its own bucket of the
    // extraction.

    // v0.19.0 — persist pack memories. The LLM proactively picked
    // rules / preferences / constraints / facts / corrections out of
    // this turn; write each to pack_memory so they recall into
    // future system prompts. Dedup happens inside `remember()` so
    // re-emitting the same memory just bumps relevance, not a new row.
    if !extraction.pack_memories.is_empty() {
        let enabled_slug_set: std::collections::HashSet<String> = state
            .enabled_packs
            .iter()
            .map(|p| p.slug().to_string())
            .collect();
        for m in &extraction.pack_memories {
            if !enabled_slug_set.contains(&m.pack_slug) {
                tracing::warn!(
                    "extraction emitted pack_memory for disabled/unknown pack {}",
                    m.pack_slug
                );
                continue;
            }
            let kind = crate::packs::memory::MemoryKind::from_str(
                m.kind.as_deref().unwrap_or("rule"),
            );
            if let Err(e) = crate::packs::memory::remember(
                &state.db.pool,
                ws_snapshot.active_id,
                &m.pack_slug,
                kind,
                m.target_kind.as_deref(),
                m.target_id,
                &m.content,
                "extraction",
                Some(conv_id),
            )
            .await
            {
                tracing::warn!("pack memory persist failed: {e}");
            }
        }
    }

    // Apply workflow ops in order ([[feedback-workflow-led]]). The LLM
    // emits a `Start` then any number of `FillSlot`s; later turns might
    // add more fills, then a `Complete` once finalization is proposed.
    // Track the current active workflow id locally so multiple ops in
    // one turn compose.
    if !extraction.workflow_ops.is_empty() {
        use crate::workflows::state::{self as wstate, SlotSource};
        let mut current_id: Option<i64> = wstate::get_active(&state.db.pool, conv_id)
            .await
            .map(|w| w.id);
        for op in &extraction.workflow_ops {
            match op {
                ExtractedWorkflowOp::Start { recipe, intent } => {
                    // Validate the recipe exists; ignore if not.
                    if crate::workflows::registry::find_recipe(recipe).is_none() {
                        tracing::warn!("workflow start ignored — unknown recipe: {recipe}");
                        continue;
                    }
                    match wstate::start(
                        &state.db.pool,
                        conv_id,
                        recipe,
                        intent.as_deref(),
                    )
                    .await
                    {
                        Ok(w) => {
                            tracing::info!("workflow started: {} (id {})", recipe, w.id);
                            current_id = Some(w.id);
                        }
                        Err(e) => tracing::warn!("workflow start failed ({recipe}): {e}"),
                    }
                }
                ExtractedWorkflowOp::FillSlot {
                    slot_name,
                    value,
                    source,
                } => {
                    let Some(id) = current_id else {
                        tracing::warn!(
                            "workflow fillSlot ignored — no active workflow for conv {conv_id}"
                        );
                        continue;
                    };
                    let src = match source.as_deref() {
                        Some("user_typed") => SlotSource::UserTyped,
                        Some("extracted") => SlotSource::Extracted,
                        Some("user_dropped") => SlotSource::UserDropped,
                        Some("graph_resolved") => SlotSource::GraphResolved,
                        _ => SlotSource::UserTyped,
                    };
                    if let Err(e) = wstate::fill_slot(
                        &state.db.pool,
                        id,
                        slot_name,
                        value.clone(),
                        src,
                    )
                    .await
                    {
                        tracing::warn!("workflow fillSlot failed ({slot_name}): {e}");
                    }
                }
                ExtractedWorkflowOp::Complete => {
                    if let Some(id) = current_id {
                        if let Err(e) = wstate::mark_completed(&state.db.pool, id).await {
                            tracing::warn!("workflow complete failed: {e}");
                        } else {
                            current_id = None;
                        }
                    }
                }
                ExtractedWorkflowOp::Abandon { reason } => {
                    if let Some(id) = current_id {
                        if let Err(e) = wstate::mark_abandoned(&state.db.pool, id).await {
                            tracing::warn!("workflow abandon failed: {e}");
                        } else {
                            tracing::info!(
                                "workflow abandoned (id {id}): {}",
                                reason.as_deref().unwrap_or("no reason")
                            );
                            current_id = None;
                        }
                    }
                }
            }
        }

        // Tell the UI to refresh its workflow indicator. The frontend
        // re-fetches via get_active_workflow on this event so the pill
        // stays in sync with state changes from the LLM's ops.
        let _ = app.emit("workflow-state-changed", conv_id);
    }

    // v0.14.4 retry-on-empty (Approach A). When the primary agent
    // loop returns no response (either fallback_extraction fired or
    // the LLM literally produced empty/whitespace), give it ONE more
    // chance with a forcing prompt before we surface an error. The
    // system prompt is cached so the retry only pays for the forcing
    // tail tokens. Logs `err_msg` from the original failure so the
    // dev console can show why the first attempt died.
    let initial_response_empty = extraction
        .response
        .as_deref()
        .map(str::trim)
        .unwrap_or("")
        .is_empty();
    if !ok || initial_response_empty {
        tracing::warn!(
            "primary pass empty/failed (ok={}, err={:?}); raw_response_len={}; running retry",
            ok,
            err_msg,
            raw_response.len()
        );
        let mut retry_msgs = messages_for_retry;
        retry_msgs.push(Message::user(
            "Your previous attempt returned no `response` value. Re-read the HOW YOUR TURN ENDS rules at the top of your system prompt — you MUST end your turn with an artifact, a specific question, or a real blocker report. Generic acknowledgements are not acceptable.\n\nIf you needed tools (read_document, run_python, etc.) but couldn't fit them, call them now. If you can't fit the work in the iterations remaining, ask a SPECIFIC question instead — name the field or doc you need.\n\nCall report_extraction now with a substantive `response` value.".to_string(),
        ));
        let retry_opts = ChatWithToolsOptions {
            system: Some(build_system_prompt(
                &profile,
                &crate::packs::prompt_fragment(&state.enabled_packs),
                &workspace_block,
                &pack_memory_block,
            )),
            cache_system: true,
            temperature: Some(0.3),
            max_tokens: Some(4000),
            tools: tool_defs.clone(),
            tool_choice: Some(ToolChoice::Specific(extraction_name.clone())),
            // Smaller thinking budget on retry — the model has more
            // explicit guidance, less open-ended reasoning needed.
            thinking_budget: Some(2000),
        };
        match provider.chat_with_tools(retry_msgs, retry_opts).await {
            Ok(turn) => {
                if let Some(call) = turn
                    .tool_calls
                    .iter()
                    .find(|c| c.name == extraction_name)
                {
                    if let Ok(retry_ext) =
                        serde_json::from_value::<Extraction>(call.input.clone())
                    {
                        let retry_response_ok = !retry_ext
                            .response
                            .as_deref()
                            .map(str::trim)
                            .unwrap_or("")
                            .is_empty();
                        if retry_response_ok {
                            tracing::info!(
                                "retry succeeded with substantive response (len={})",
                                retry_ext.response.as_deref().map(|s| s.len()).unwrap_or(0)
                            );
                            extraction.response = retry_ext.response;
                        } else {
                            tracing::warn!("retry also returned empty response");
                        }
                    } else {
                        tracing::warn!("retry tool input parse failed");
                    }
                } else {
                    tracing::warn!("retry returned no report_extraction call");
                }
            }
            Err(e) => {
                tracing::warn!("retry LLM call errored: {e}");
            }
        }
    }

    // Prefer the LLM's own free-form reply. Synthesis fallback is
    // reserved for the truly-broken case where even the retry left
    // response empty.
    let response_text = extraction
        .response
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let assistant_visible: String = match response_text {
        Some(r) => {
            // Belt-and-suspenders: the system prompt asks Travis to voice
            // capability gaps in its own words. If somehow it didn't (no gap
            // verb appears in the response), append a discreet marker so the
            // user is never unaware of an unmet ask.
            let mut text = r.clone();
            if !extraction.capability_gaps.is_empty() {
                let lower = text.to_lowercase();
                let mentioned = extraction.capability_gaps.iter().any(|g| {
                    let cap = g.capability.to_lowercase();
                    !cap.is_empty() && lower.contains(&cap)
                });
                if !mentioned {
                    let first = &extraction.capability_gaps[0].capability;
                    let extra = if extraction.capability_gaps.len() > 1 {
                        format!(" (+{} more)", extraction.capability_gaps.len() - 1)
                    } else {
                        String::new()
                    };
                    text.push_str(&format!(
                        "\n\n(Logged: '{first}'{extra} — I can't do that yet but I'm tracking it.)"
                    ));
                }
            }
            text
        }
        None => {
            // v0.14.4: both primary AND retry produced empty.
            // Surface a more informative error so the user has
            // something to act on.
            tracing::error!(
                "both primary and retry returned empty (orig_err={:?})",
                err_msg
            );
            let (hint, kind) = match err_msg.as_deref() {
                Some(e) if e.contains("max iterations") => (
                    "Travis ran out of tool-call iterations on this turn. Try sending fewer documents at once, or break the request into smaller steps.",
                    crate::diagnostics::ErrorKind::IterCap,
                ),
                Some(e) if e.contains("parse") => (
                    "Travis's reply couldn't be parsed. This is usually a transient model issue — please try again in a moment.",
                    crate::diagnostics::ErrorKind::Parse,
                ),
                Some(_) => (
                    "Travis hit an error while thinking through that turn. Try again, or rephrase the request.",
                    crate::diagnostics::ErrorKind::LlmApi,
                ),
                None => (
                    "Travis didn't produce a reply on that turn. Try again or rephrase the request.",
                    crate::diagnostics::ErrorKind::Other,
                ),
            };
            // v0.15.4 — persist the underlying err_msg + raw response
            // so the Diagnostics UI + the chat-side expandable error
            // detail have something concrete to show.
            crate::diagnostics::record_error(
                &state.db.pool,
                Some(conv_id),
                kind,
                "journal::synthesis_fallback",
                err_msg.clone().unwrap_or_else(|| "empty response".into()),
                Some(serde_json::json!({
                    "errMsg": err_msg,
                    "rawResponseSnippet": raw_response.chars().take(2000).collect::<String>(),
                    "rawResponseLength": raw_response.len(),
                })),
            )
            .await;
            hint.to_string()
        }
    };

    // v0.15.4 — when the assistant message is a synthesised error
    // hint, attach errorDetail to the payload so the chat UI can
    // render it as a collapsed expandable trace. Normal turns use
    // the unmodified extraction_record.
    let assistant_payload: String = if !ok || extraction.response.as_deref()
        .map(str::trim)
        .unwrap_or("")
        .is_empty()
    {
        let mut combined: serde_json::Value =
            serde_json::from_str(&extraction_record).unwrap_or_else(|_| serde_json::json!({}));
        if let Some(obj) = combined.as_object_mut() {
            obj.insert(
                "errorDetail".into(),
                serde_json::json!({
                    "errMsg": err_msg.clone(),
                    "rawResponseSnippet": raw_response.chars().take(4000).collect::<String>(),
                    "rawResponseLength": raw_response.len(),
                }),
            );
        }
        combined.to_string()
    } else {
        extraction_record.clone()
    };

    // v0.17.0 — classify this turn for the chat UI's reasoning card.
    // Pull thinking_blocks + tool_calls counts from raw_response (it's
    // the serialized last_dump JSON the agent loop emitted).
    let (thinking_count, tool_count) = parse_turn_stats(&raw_response);
    let response_kind = crate::events::classify_response(
        &assistant_visible,
        thinking_count,
        tool_count,
        ok,
    );
    let assistant_msg = conversation::append_with_kind(
        &state.db.pool,
        conv_id,
        "assistant",
        &assistant_visible,
        Some(&assistant_payload),
        Some(response_kind.as_str()),
    )
    .await;
    let assistant_msg_id = assistant_msg.as_ref().ok().map(|m| m.id);
    let _ = crate::events::append_or_warn(
        &state.db.pool,
        conv_id,
        crate::events::EventKind::AgentResponse,
        Some(&serde_json::json!(crate::events::AgentResponsePayload {
            response_kind,
            thinking_blocks: thinking_count,
            tool_calls: tool_count,
            iterations: manager_iter + 1,
        })),
        None,
        assistant_msg_id,
    )
    .await;

    // v0.15.2 — background capture. Spawn task + reminder
    // persistence onto a Tokio worker so the chat command returns
    // immediately. Best-effort: any failure logs but never blocks
    // the user-facing reply.
    if !is_conversational {
        let snap = crate::capture::CaptureSnapshot {
            pool: state.db.pool.clone(),
            app: app.clone(),
            conv_id,
            tasks: extraction.tasks.clone(),
            reminders: extraction.reminders.clone(),
            dest_ws_state: dest_ws_state.clone(),
            // v0.19.3 — packs travel as &'static dyn refs so this is a
            // cheap clone; the background task picks up each pack's
            // ensure_entity + apply_extraction_observations.
            enabled_packs: state.enabled_packs.clone(),
            extraction: extraction.clone(),
            entities_snapshot: extraction.entities.0.clone(),
        };
        tauri::async_runtime::spawn(async move {
            crate::capture::run_background(snap).await;
        });
    }

    // Inference-driven clarifying question (BRAIN.md Phase 4.5 #10).
    // When the LLM didn't already produce a question and we have room,
    // poll the inference helpers for a high-leverage one to slot in:
    // refinement candidates (`*:unknown` entities with enough mentions
    // to warrant categorisation), then name conflicts. One per turn,
    // chosen by highest mentions_count.
    if extraction.clarifying_questions.len() < 2 {
        if let Ok(candidates) = crate::graph_inference::recurring_mention_candidates(
            &state.db.pool,
            &ws_snapshot.visible_ids,
            1,
        )
        .await
        {
            if let Some(c) = candidates.into_iter().next() {
                let base_kind = c.kind.split(':').next().unwrap_or("entity");
                let suggestions = match base_kind {
                    "person" => "coach, parent, teacher, principal, or someone else",
                    "place" => "school, district office, vendor site, or somewhere else",
                    "org" => "school, district, vendor, or something else",
                    _ => "what kind of thing",
                };
                let q = format!(
                    "I've seen \"{}\" come up {} times now — is this a {}?",
                    c.display_name, c.mentions_count, suggestions
                );
                extraction.clarifying_questions.push(q);
                // Stamp so we don't ask again within the cooldown window.
                let _ = crate::graph_inference::mark_clarification_prompted(
                    &state.db.pool,
                    c.entity_id,
                )
                .await;
            }
        }
    }

    // Self-advocacy: recurring capability gaps (BRAIN.md capability #6).
    // When the same gap (Gmail-not-connected, calendar-write, etc.) has
    // fired ≥3 times in the last 14 days without being addressed and
    // hasn't been surfaced in the last 7, slot in one Travis-voice ask
    // — "I keep stalling on X because Y isn't set up — want to fix
    // that?". Stamps so the cooldown holds. One advocacy per turn max.
    if extraction.clarifying_questions.len() < 2 {
        let gaps = crate::feedback::recurring_unaddressed_gaps(&state.db.pool, 1).await;
        if let Some(g) = gaps.into_iter().next() {
            let cap = g.capability.trim();
            if !cap.is_empty() {
                let q = format!(
                    "I've punted on \"{cap}\" {} times now — usually because something isn't set up on my side. Want to address it together so I can actually do this for you?",
                    g.hit_count,
                );
                extraction.clarifying_questions.push(q);
                let _ = crate::feedback::mark_advocacy_surfaced(&state.db.pool, cap).await;
            }
        }
    }

    // Conversation status: awaiting the user if there are clarifying questions
    // OR pending action proposals, resolved on conversational small-talk
    // (without gaps to surface), otherwise open for next note.
    let next_status = if !extraction.clarifying_questions.is_empty()
        || !persisted_actions.is_empty()
    {
        "awaiting_user"
    } else if is_conversational {
        "resolved"
    } else {
        "open"
    };
    let _ = conversation::set_status(&state.db.pool, conv_id, next_status).await;

    let _ = app.emit("domain-changed", "journal");

    // Telemetry — metadata only, never raw text.
    telemetry::emit(
        &state.db.pool,
        "journal_ingested",
        serde_json::json!({
            "intent": final_intent,
            "ok": ok,
            "created": created.len(),
            "completed": completed.len(),
            "questions": extraction.clarifying_questions.len(),
            "gaps": extraction.capability_gaps.len(),
            "provider": profile.llm_provider,
        }),
    )
    .await;

    let thread = Thread {
        conversation: conversation::fetch(&state.db.pool, conv_id)
            .await
            .map_err(|e| e.to_string())?,
        messages: conversation::messages(&state.db.pool, conv_id, None)
            .await
            .map_err(|e| e.to_string())?,
    };

    Ok(JournalIngestResult {
        journal_entry_id: entry_id,
        conversation_id: conv_id,
        thread,
        intent: final_intent,
        response: extraction.response.clone(),
        tasks_created: created,
        tasks_completed: completed,
        entities: if is_conversational {
            EntityMentions::default()
        } else {
            extraction.entities
        },
        reminders: if is_conversational {
            Vec::new()
        } else {
            extraction.reminders
        },
        clarifying_questions: extraction.clarifying_questions,
        capability_gaps: extraction.capability_gaps,
        proposed_actions: persisted_actions,
        routing: routing_result,
        mention_chips,
        extraction_ok: ok,
        error: err_msg,
    })
}

/// v0.18.2 — user-facing label for the step row. Maps technical tool
/// names to plain-English verbs so the chat doesn't expose
/// "run_python" / "search_memory" / "analyze_document_styling" to
/// users. For `run_python` and `edit_python_artifact` we look at the
/// declared purpose to pick a verb (e.g. a purpose that mentions
/// "invoice" → "Generating invoice").
fn humanize_tool_name(name: &str, input: &serde_json::Value) -> String {
    let purpose = input
        .get("purpose")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_lowercase();
    match name {
        "read_document" => "Reading attachment".to_string(),
        "preview_document" => "Skimming attachment".to_string(),
        "analyze_document_styling" => "Studying the layout".to_string(),
        "reconcile_documents" => "Cross-referencing attachments".to_string(),
        "find_documents" | "find_matching_documents" => "Finding matching documents".to_string(),
        "search_memory" => "Checking the records".to_string(),
        "search_conversations" => "Looking through past threads".to_string(),
        "remember_constraint" => "Remembering that for next time".to_string(),
        "find_case" => "Looking up the case".to_string(),
        "web_fetch" => "Fetching from the web".to_string(),
        "delegate" => "Asking a focused side-question".to_string(),
        "run_python" | "edit_python_artifact" => {
            if name == "edit_python_artifact" {
                "Refining the file".to_string()
            } else if purpose.contains("invoice") {
                "Generating invoice".to_string()
            } else if purpose.contains("sign") && purpose.contains("sheet") {
                "Building sign-in sheet".to_string()
            } else if purpose.contains("pdf") {
                "Generating PDF".to_string()
            } else if purpose.contains("excel") || purpose.contains("xlsx") {
                "Working with spreadsheet".to_string()
            } else if purpose.contains("parse") || purpose.contains("read") || purpose.contains("extract") {
                "Pulling data out of the sheet".to_string()
            } else if purpose.contains("filter") || purpose.contains("find") {
                "Filtering the data".to_string()
            } else {
                "Working on the file".to_string()
            }
        }
        // Pack-supplied tools — fall through with a leading verb so
        // they read like an action rather than a snake_case identifier.
        other => {
            let pretty = other
                .replace('_', " ")
                .chars()
                .enumerate()
                .map(|(i, c)| if i == 0 { c.to_ascii_uppercase() } else { c })
                .collect::<String>();
            pretty
        }
    }
}

/// v0.19.0 — produce a short label for a Thinking-kind child step
/// from the raw thinking block. We don't want to dump the whole
/// chain-of-thought in the step name, just a verb-led summary so the
/// chat surface reads like a narration. Heuristic: the first sentence
/// or first 60 chars, with a leading "Thinking about" prefix.
fn summarise_thinking(thought: &str) -> String {
    let first_sentence = thought
        .split(|c: char| c == '.' || c == '?' || c == '!' || c == '\n')
        .find(|s| !s.trim().is_empty())
        .unwrap_or(thought)
        .trim();
    let trimmed: String = first_sentence.chars().take(60).collect();
    if trimmed.is_empty() {
        "Thinking…".to_string()
    } else if trimmed.to_lowercase().starts_with("i ")
        || trimmed.to_lowercase().starts_with("let me ")
        || trimmed.to_lowercase().starts_with("looking ")
    {
        format!("Reasoning · {trimmed}")
    } else {
        format!("Reasoning · {trimmed}")
    }
}

/// v0.18.2 — resolve the step's detail string from the tool input,
/// hitting the documents table when needed so that document-touching
/// tools surface the actual filename instead of "doc#1".
async fn resolve_tool_detail(
    pool: &sqlx::SqlitePool,
    name: &str,
    input: &serde_json::Value,
) -> String {
    let pick = |key: &str| {
        input
            .get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.chars().take(80).collect::<String>())
    };
    match name {
        "read_document" | "analyze_document_styling" | "preview_document" => {
            if let Some(id) = input
                .get("documentId")
                .or_else(|| input.get("document_id"))
                .and_then(|v| v.as_i64())
            {
                match crate::documents::db::get(pool, id).await {
                    Ok(Some(doc)) => doc.original_filename,
                    _ => String::new(),
                }
            } else {
                String::new()
            }
        }
        "run_python" | "edit_python_artifact" => {
            // Use the worker-supplied purpose; if it lists document
            // ids, we trust the purpose to already describe them in
            // English rather than re-resolving every one (could be
            // up to 5).
            pick("purpose").unwrap_or_default()
        }
        "search_memory" => pick("query").unwrap_or_default(),
        "search_conversations" => pick("query").unwrap_or_default(),
        "web_fetch" => pick("url").unwrap_or_default(),
        "reconcile_documents" => {
            // Multi-doc tool — list filenames joined by "+", capped
            // at three so the step row stays compact.
            let ids: Vec<i64> = input
                .get("documentIds")
                .or_else(|| input.get("document_ids"))
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_i64())
                        .take(3)
                        .collect()
                })
                .unwrap_or_default();
            if ids.is_empty() {
                return pick("purpose").unwrap_or_default();
            }
            let mut names = Vec::new();
            for id in ids {
                if let Ok(Some(d)) = crate::documents::db::get(pool, id).await {
                    names.push(d.original_filename);
                }
            }
            names.join(" + ")
        }
        _ => pick("purpose")
            .or_else(|| pick("query"))
            .or_else(|| pick("name"))
            .unwrap_or_default(),
    }
}

/// v0.17.0 — pull `(thinking_blocks, tool_calls)` from the agent
/// loop's serialized last_dump. Best-effort: returns (0, 0) on parse
/// failure so the response classifier degrades to `text_response`
/// rather than crashing the turn.
fn parse_turn_stats(raw_response: &str) -> (usize, usize) {
    let v: serde_json::Value = match serde_json::from_str(raw_response) {
        Ok(v) => v,
        Err(_) => return (0, 0),
    };
    let thinking = v
        .get("thinking_blocks")
        .and_then(|n| n.as_u64())
        .unwrap_or(0) as usize;
    let tool_calls = v
        .get("tool_calls")
        .and_then(|n| n.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    (thinking, tool_calls)
}

fn fallback_extraction(raw: &str) -> Extraction {
    let title: String = raw.chars().take(120).collect();
    Extraction {
        tasks: vec![ExtractedTask {
            title,
            due_at: None,
            priority: Some(0),
            notes: None,
        }],
        ..Default::default()
    }
}

#[derive(Debug, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct JournalEntryRow {
    pub id: i64,
    pub raw_text: String,
    pub extraction_ok: i64,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub created_at: String,
}

#[tauri::command]
pub async fn list_journal_entries(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> Result<Vec<JournalEntryRow>, String> {
    let limit = limit.unwrap_or(50).clamp(1, 500);
    let rows = sqlx::query_as::<_, JournalEntryRow>(
        "SELECT id, raw_text, extraction_ok, provider, model, created_at
         FROM journal_entry ORDER BY id DESC LIMIT ?1",
    )
    .bind(limit)
    .fetch_all(&state.db.pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_code_fences() {
        assert_eq!(strip_code_fences("```json\n{\"a\":1}\n```"), "{\"a\":1}");
        assert_eq!(strip_code_fences("```\n{\"a\":1}\n```"), "{\"a\":1}");
        assert_eq!(strip_code_fences("{\"a\":1}"), "{\"a\":1}");
    }
}
