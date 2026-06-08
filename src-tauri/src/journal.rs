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

When the user attaches documents (PDFs, spreadsheets, images, .docx), their content is pre-loaded into the user message under `== ATTACHED DOCUMENTS ==`. Spreadsheets show only a structural preview — read them in `run_python` with pandas (`pd.read_excel("/inputs/<filename>")`). Other docs are summarized; call `read_document` if you need the full body.

When the user gives you a sample and asks you to "match this", "make one like this", or "adapt this format": call `analyze_document_styling(document_id)` first so you have the colour/font/layout JSON to drive any generation code, then `run_python` to produce the output. Generated files in `/outputs/<filename>` automatically appear as file cards in the chat.

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
    let _ = conversation::append(&state.db.pool, conv_id, "user", &raw, None).await;

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
        "Today is {today}.\n\nOPEN TASKS (id · title):\n{open}\n\nRELEVANT MEMORY:\n{mem}\n\n{graph}{working}{initiatives}{cases}{workflow}{catalog}{ws}{docs_preload}New turn:\n{raw}",
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
    // v0.14.3: bumped 4 → 8. With capture extraction off the primary
    // pass the model has way more room to call tools — read_document,
    // analyze_document_styling, then one or two run_python passes —
    // before finalizing. Real loops won't burn the budget; this is
    // headroom so Travis doesn't run out of turns and dump a
    // placeholder reply instead of finishing the work.
    const MAX_ITER: usize = 8;

    // Clone the message stack before the agent loop takes ownership,
    // so the empty-response retry path below can re-run the LLM with
    // the same context (system prompt is cached, so the second call
    // shares the prefix and only pays for the forcing-message tail).
    let messages_for_retry = messages.clone();
    let (mut extraction, ok, err_msg, raw_response) = if let Some(fp) = fast_path_extraction {
        tracing::info!("journal_ingest fast-path hit (intent={})", fp.intent);
        (fp, true, None, "<fast-path>".to_string())
    } else { 'outer: {
        let mut current_messages = messages;
        let mut last_dump = String::new();
        for iter in 0..MAX_ITER {
            // Last iteration forces the extraction tool to ensure we always finalize.
            let choice = if iter == MAX_ITER - 1 {
                ToolChoice::Specific(extraction_name.clone())
            } else {
                ToolChoice::Auto
            };
            let opts = ChatWithToolsOptions {
                system: Some(build_system_prompt(
                    &profile,
                    &crate::packs::prompt_fragment(&state.enabled_packs),
                    &workspace_block,
                )),
                cache_system: true,
                temperature: Some(0.3),
                max_tokens: Some(1500),
                tools: tool_defs.clone(),
                tool_choice: Some(choice),
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
                    last_dump = serde_json::json!({
                        "iter": iter,
                        "content": turn.content,
                        "tool_calls": turn.tool_calls,
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
                        let result = match read_registry
                            .execute(&tool_ctx, &call.name, call.input.clone())
                            .await
                        {
                            Ok(s) => s,
                            Err(e) => format!("error: {e}"),
                        };
                        let truncated: String = result.chars().take(8000).collect();
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
        for t in &extraction.tasks {
            let title = t.title.trim();
            if title.is_empty() {
                continue;
            }
            let truncated = if title.chars().count() > 120 {
                title.chars().take(120).collect()
            } else {
                title.to_string()
            };
            let task = task::upsert(
                &state.db.pool,
                &dest_ws_state,
                TaskInput {
                    id: None,
                    title: truncated,
                    description: t.notes.clone(),
                    priority: t.priority,
                    due_at: t.due_at.clone(),
                    entity_id: None,
                    link_kind: None,
                    link_id: None,
                    source: Some("journal".into()),
                },
            )
            .await
            .map_err(|e| e.to_string())?;
            created.push(task);
        }

        for tid in &extraction.completed_task_ids {
            if !open_ids.contains(tid) {
                tracing::warn!("LLM returned completedTaskId {tid} not in open list — ignoring");
                continue;
            }
            match task::set_status(&state.db.pool, &dest_ws_state, *tid, "done").await {
                Ok(t) => completed.push(t),
                Err(e) => tracing::warn!("failed to mark task {tid} done: {e}"),
            }
        }

        for r in &extraction.reminders {
            let text = r.text.trim();
            if text.is_empty() {
                continue;
            }
            let remind_at = r
                .remind_at
                .as_ref()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            if remind_at.is_none() {
                continue;
            }
            if let Err(e) = reminders::upsert(
                &state.db.pool,
                dest_ws_state.active_id,
                ReminderInput {
                    id: None,
                    text: text.to_string(),
                    kind: Some("time".into()),
                    remind_at,
                    source: Some("journal".into()),
                    link_kind: None,
                    link_id: None,
                },
            )
            .await
            {
                tracing::warn!("failed to create reminder from journal extraction: {e}");
            }
        }

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
            )),
            cache_system: true,
            temperature: Some(0.3),
            max_tokens: Some(2000),
            tools: tool_defs.clone(),
            tool_choice: Some(ToolChoice::Specific(extraction_name.clone())),
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
            let hint = match err_msg.as_deref() {
                Some(e) if e.contains("max iterations") => {
                    "Travis ran out of tool-call iterations on this turn. Try sending fewer documents at once, or break the request into smaller steps."
                }
                Some(e) if e.contains("parse") => {
                    "Travis's reply couldn't be parsed. This is usually a transient model issue — please try again in a moment."
                }
                Some(_) => {
                    "Travis hit an error while thinking through that turn. Try again, or rephrase the request."
                }
                None => {
                    "Travis didn't produce a reply on that turn. Try again or rephrase the request."
                }
            };
            hint.to_string()
        }
    };
    let _ = conversation::append(
        &state.db.pool,
        conv_id,
        "assistant",
        &assistant_visible,
        Some(&extraction_record),
    )
    .await;

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
