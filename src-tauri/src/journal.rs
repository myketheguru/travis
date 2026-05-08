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

fn build_system_prompt(profile: &UserProfile, pack_fragment: &str) -> String {
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
                }
            },
            "required": ["intent", "response"]
        }),
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
                conversation::open(&state.db.pool, "journal", Some(&title))
                    .await
                    .map_err(|e| e.to_string())?
                    .id
            } else {
                cid
            }
        }
        None => {
            let title = raw.chars().take(60).collect::<String>();
            conversation::open(&state.db.pool, "journal", Some(&title))
                .await
                .map_err(|e| e.to_string())?
                .id
        }
    };

    let entry_id: i64 = sqlx::query("INSERT INTO journal_entry (raw_text) VALUES (?1)")
        .bind(&raw)
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

    let provider = llm::build(
        &profile.llm_provider,
        api_key.as_deref(),
        profile.ollama_url.as_deref(),
        profile.model.as_deref(),
        state.http.clone(),
    )
    .map_err(|e| e.to_string())?;

    // Pull current open tasks so the LLM can detect completions.
    let open_tasks = task::list(
        &state.db.pool,
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

    // Pull semantic memory hits so Travis can answer questions grounded in past notes.
    let entities_hint = extract_entity_hints(&raw);
    let mem_hits = memory::retrieve(&state.db.pool, &raw, &entities_hint, 5)
        .await
        .unwrap_or_default();

    let user_msg = format!(
        "Today is {today}.\n\nOPEN TASKS (id · title):\n{open}\n\nRELEVANT MEMORY:\n{mem}\n\nNew turn:\n{raw}",
        today = today_local(),
        open = format_open_tasks(&open_tasks),
        mem = format_memory(&mem_hits),
        raw = raw
    );
    messages.push(Message::user(user_msg));

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

    let (extraction, ok, err_msg, raw_response) = 'outer: {
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
    };

    let is_conversational = extraction.intent.eq_ignore_ascii_case("conversational");

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
            match task::set_status(&state.db.pool, *tid, "done").await {
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
    if let Err(e) = memory::index_journal_entry(&state.db.pool, entry_id, &raw).await {
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
