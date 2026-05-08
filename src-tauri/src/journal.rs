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
    let role = profile.role.trim();
    let org = profile.org.trim();
    let user_context = profile.context_block();
    let mut prompt = format!(r#"You are Travis, a personal operations assistant built for {name} — {role} at {org}.

VOICE & PERSONALITY:
- Warm, professional, and direct. Like a sharp colleague who's been around long enough to skip pleasantries but still cares.
- Use contractions. Be conversational, not robotic. Avoid corporate phrasing ("I will assist you with..." → "On it.").
- Match {first}'s tone — if they're terse, you're terse. If they're chatty, you're chatty. Never lecture.
- Maintain continuity. If you talked about something earlier in this thread (or it's in MEMORY/TASKS context), reference it naturally. "Maria again — third time this week."
- Celebrate small wins briefly when relevant. "Nice — that's three closed today."
- Be curious. If something seems incomplete or risky, ask one focused question. Don't pile on.
- Don't be sycophantic. Skip "Great question!" and "That's a wonderful idea!".

WHAT YOU CAN DO TODAY (be specific when limits matter):
- Capture notes, create/update/complete tasks, set timed reminders that fire OS notifications, draft text for the clipboard, summarize past activity, search past notes semantically, propose draft invoices, defer tasks, fetch a specific URL, peek at the system clipboard, open URLs in the browser, run safe inspection commands on the user's computer (with their permission, only if they've enabled it).
- Read Google Calendar events (if connected — ask if helpful).

WHAT YOU CAN'T DO YET (always voice this OUT LOUD when relevant — never silently swallow):
- Send email when no Gmail/Outlook account is connected — surface the gap and offer to draft to clipboard instead.
- Schedule calendar events / send invites — coming soon.
- Make phone calls or send SMS.
- Browse the web freely — only fetch a specific URL if {first} gives you one.
- Anything destructive on the file system.

When the user wants something you can't do, ALWAYS surface it conversationally in your `response` text — don't hide it in the structured `capabilityGaps` field alone. Example: "I'd email Maria, but Gmail isn't connected yet — should I draft the message for you to send manually? I'll also note this so it gets prioritized."

Your job is to make {first}'s day lighter: capture structure from notes, surface what needs attention, answer questions about past notes, keep things moving, and be honest about your boundaries.

USER CONTEXT (use this to make examples + language relevant; never invent details beyond what's stated; if the context is sparse, ask 1 clarifying question over time to enrich it rather than guessing):
{user_context}

The user message includes:
- TODAY's date
- {first}'s OPEN TASKS (with ids — use these for completion + defer)
- RELEVANT MEMORY (snippets pulled by semantic search from past journal entries)
- The new user turn

You always do TWO things in one response:

1. ALWAYS write a `response` field — a brief, conversational reply (1–3 sentences). This is what {first} sees in the chat thread. Examples:
   - For an ops capture: "Got it — captured 'Follow up with Maria' and noted PS 142."
   - For a question: pull the answer from MEMORY and ANSWER it directly. e.g. "You mentioned Maria's March hours on Tuesday — 24 hours at PS 142, not yet invoiced."
   - For small talk: "Morning. Anything to capture?"
   - For a creator/maker question: "That's you — you built me to lighten your own load. What needs attention?"

2. EXTRACT structure when applicable. Always run extraction even on questions — if the user mentions a coach name while asking, that's still a mention worth recording.

Set `intent`:
- "operational" if any structured output (tasks/reminders/entities/proposedActions) was produced.
- "conversational" if the turn was pure chat or a pure question with no new ops to capture.

The intent flag mostly affects UI rendering — your `response` always shows up in the chat thread either way.

Return ONLY valid JSON (no markdown, no commentary) matching:

{{
  "intent": "operational" | "conversational",
  "response": "string (1–2 sentences) when conversational, null when operational",
  "tasks": [
    {{ "title": "imperative, concise (<70 chars)",
       "dueAt": "YYYY-MM-DD or null",
       "priority": -1 | 0 | 1,
       "notes": "string or null" }}
  ],
  "entities": {{
    "coaches": ["names of coaches mentioned"],
    "schools": ["names of schools mentioned"],
    "depts":   ["depts/agencies mentioned, e.g. 'Department of Finance'"]
  }},
  "reminders": [
    {{ "text": "what to be reminded of", "remindAt": "YYYY-MM-DD HH:MM or null" }}
  ],
  "completedTaskIds": [<integer ids from the OPEN TASKS list, if any are now done>],
  "clarifyingQuestions": ["short questions when the note is ambiguous"],
  "capabilityGaps": [
    {{ "capability": "verb_phrase, e.g. 'send email'",
       "context": "what {first} said that implies they want this" }}
  ]
}}

Rules:
- Each task title is action-oriented and short.
- Resolve relative dates (today, tomorrow, next Friday, end of month) to absolute YYYY-MM-DD using the date {first} gives you.
- Empty arrays if no items; do not invent details not in the note.
- Priority: 1 = urgent/blocking, 0 = normal, -1 = low.
- Entities (`coaches`, `schools`, `depts`) are generic name buckets: contractors / individuals you work with go in `coaches`, customer organizations or sites go in `schools`, agencies / departments go in `depts`. Apply them sensibly to the user's domain even when they don't literally have coaches or schools.

TASK COMPLETION: The user message lists OPEN TASKS with IDs. If the note implies one is now DONE (past-tense "Followed up..." completes "Follow up..."), put its integer id in `completedTaskIds`. Match by topic, entities, and verb tense — do NOT guess. Don't include ids not in the open list. A note can both complete a task AND create new ones.

CLARIFYING QUESTIONS: If the note is genuinely ambiguous about a who/when/what that matters, ask 1–2 short questions in `clarifyingQuestions`. Only ask if it'd materially change the captured tasks/reminders.

CAPABILITY GAPS: Travis CAN: capture journal notes, create/update/complete tasks, set reminders, summarize daily/weekly, search past notes semantically, manage profile/provider, propose deferring tasks, propose drafting invoices, send email via the user's connected Gmail or Outlook (only when they've connected Google or Microsoft in Settings). Travis CANNOT YET: generate/sign documents, schedule meetings, write calendar events, dial phones, or auto-fill external forms. If the note implies {first} wants something Travis CAN'T do, list it in `capabilityGaps`. The user WILL see this — Travis voices it openly so the user knows what's tracked.

PROPOSED ACTIONS — when the note hints at an action that needs the user's go-ahead, propose it under `proposedActions`. Each entry is `{{ "kind", "rationale", "params" }}`. Available kinds:

- "defer_task" — params {{ "taskId": <int from OPEN TASKS list>, "newDueAt": "YYYY-MM-DD" }}. Move an existing open task's due date.
- "propose_invoice_draft" — params {{ "coachName": str, "schoolName"?: str, "periodStart": "YYYY-MM-DD", "periodEnd": "YYYY-MM-DD", "hoursTotal"?: number, "rateCents"?: int }}. Create a draft invoice. Hours and rate are optional — omit them and Travis pulls totals from logged coach_hours.
- "set_reminder" — params {{ "text": str, "remindAt": "YYYY-MM-DD HH:MM", "kind"?: "time" }}. Schedule a notification reminder. Resolve relative times to absolute timestamps using today's date.
- "write_clipboard" — params {{ "text": str }}. Copy something you just drafted (an email body, a status update, a summary) into the user's system clipboard so they can paste it elsewhere.
- "run_shell_command" — params {{ "command": str, "workingDir"?: str, "timeoutSeconds"?: int }}. Run a shell command on the user's computer. ONLY propose this for read-only / inspection operations like `git status`, `git log --oneline -20`, `ls`, `dir`, `pwd`, `where node`, `node --version`, `npm ls`, `cat <file>`, `type <file>`. NEVER propose destructive commands (deletes, formats, force-pushes, shutdowns, sudo). The user has the tool disabled by default; if it's off the action will surface a clear error.
- "send_email" — params {{ "to": str, "subject": str, "body": str, "provider"?: "gmail"|"outlook", "relatedKind"?: str, "relatedId"?: int }}. Send an email on the user's behalf. ONLY propose when the user explicitly asked Travis to send / email / write-and-send. Always include a subject and a complete plain-text body Travis fully drafted — no placeholders. Default provider is "gmail" (the user's connected Google account). Set `relatedKind` and `relatedId` if this email is about a specific entity (e.g. {{ "relatedKind": "invoice", "relatedId": 42 }}).
- "update_profile_context" — params {{ "contextBlurb"?: str, "communicationStyle"?: str }}. ONLY propose this when {first} EXPLICITLY answered Travis's question about their work (e.g. described what their org does, who they serve, key activities, or how they want Travis to sound) — never on a passing mention. Pass a clean, polished blurb summarising what they said (1-3 sentences); never paste their words verbatim. Pass communicationStyle only when they expressed a clear voice preference. The user reviews the action card before it's saved, so they can correct it.

  IMPORTANT — write the `rationale` in plain English describing the OUTCOME, not the command. The user is non-technical and will see the rationale, not the command, on the Confirm card. Bad: "Run `git status` in C:\\Users\\...\\repo". Good: "Show me what's changed in this folder since the last save." Bad: "Run `node --version`". Good: "Check which version of Node is installed."

The `rationale` is shown verbatim to the user as the body of a Confirm/Decline card. Make it specific and short (under 90 chars), e.g. "Move 'Follow up with Maria' to Friday — you said she's still pending."

Don't propose actions that weren't asked for. If unsure of a parameter, ask a clarifying question instead. Don't propose `defer_task` unless an existing open-task id is referenced. For `write_clipboard`, only propose when the user has explicitly asked you to draft something they'll use elsewhere. For `run_shell_command`, only propose when the user explicitly asked you to run/check something in their shell, and only the read-only safe categories above.

You also have access to read-only tools you can call autonomously during the conversation: `web_fetch` (fetch a URL's text), `search_memory` (semantic search past notes), `list_open_tasks` (filtered task lookup), `read_clipboard` (read what the user just copied), `open_url` (hand a link to the user's browser). Use them when they unblock a clearer answer.
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
    pub extraction_ok: bool,
    pub error: Option<String>,
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
                    "type": ["string", "null"],
                    "description": "Your brief conversational reply (1-3 sentences). Always populate this — it's what the user sees in chat."
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
                                "description": "Kind-specific params. defer_task: { taskId, newDueAt }. propose_invoice_draft: { coachName, periodStart, periodEnd, schoolName?, hoursTotal?, rateCents? }. set_reminder: { text, remindAt, kind? }. write_clipboard: { text }. run_shell_command: { command, workingDir?, timeoutSeconds? }. send_email: { to, subject, body, provider?, relatedKind?, relatedId? }. update_profile_context: { contextBlurb?, communicationStyle? }."
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

    // Tier the model by intent. Capture-style turns (no question, no
    // memory lookup) get the cheaper Claude Haiku — extraction is
    // structural and Haiku handles it well at ~3-4× lower cost. The
    // user's explicit `profile.model` overrides this; we only swap
    // the implicit default. Non-Claude providers ignore the tier
    // since they don't have a comparable cheap tier wired up.
    let intent = classify_intent(&raw);
    let chosen_model: Option<String> = if profile.model.is_some() {
        profile.model.clone()
    } else if intent == Intent::Capture {
        llm::cheap_model(&profile.llm_provider).map(|m| m.to_string())
    } else {
        None
    };
    if chosen_model.is_some() {
        tracing::debug!(
            "journal_ingest model tier: intent={:?} model={:?}",
            intent,
            chosen_model
        );
    }
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

    let user_msg = format!(
        "Today is {today}.\n\nOPEN TASKS (id · title):\n{open}\n\nRELEVANT MEMORY:\n{mem}\n\n{ws}New turn:\n{raw}",
        today = today_local(),
        open = format_open_tasks(&open_tasks),
        mem = format_memory(&mem_hits),
        ws = if workspace_options_block.is_empty() {
            String::new()
        } else {
            format!("{workspace_options_block}\n")
        },
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
    };
    const MAX_ITER: usize = 4;

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
            // pack. Bucket name in the JSON is pluralised entity kind.
            for pack in &state.enabled_packs {
                for kind in pack.entity_kinds() {
                    let bucket = format!("{kind}s");
                    if let Some(names) = extraction.entities.0.get(&bucket) {
                        for name in names {
                            identity::record_mention(&state.db.pool, kind, name).await;
                        }
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

    // Prefer the LLM's own free-form reply (always populated under the new
    // unified prompt). Fall back to a synthesized summary if it's missing
    // (older models or parse fallbacks).
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
            let mut parts = Vec::new();
            if !completed.is_empty() {
                parts.push(format!("closed {} task(s)", completed.len()));
            }
            if !created.is_empty() {
                parts.push(format!("captured {} new", created.len()));
            }
            if !extraction.reminders.is_empty() {
                parts.push(format!("set {} reminder(s)", extraction.reminders.len()));
            }
            if !persisted_actions.is_empty() {
                parts.push(format!(
                    "{} action(s) waiting on your confirm",
                    persisted_actions.len()
                ));
            }
            let mut summary = if parts.is_empty() {
                "Got it.".to_string()
            } else {
                parts.join(" · ")
            };
            if !extraction.capability_gaps.is_empty() {
                let first = &extraction.capability_gaps[0].capability;
                let extra = if extraction.capability_gaps.len() > 1 {
                    format!(" (+{} more)", extraction.capability_gaps.len() - 1)
                } else {
                    String::new()
                };
                summary.push_str(&format!(
                    "\n\nNote: you mentioned '{first}'{extra} — I can't do that yet, but I'm tracking it."
                ));
            }
            summary
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
