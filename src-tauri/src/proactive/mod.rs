//! Proactive nudges. A tokio task wakes up periodically (every hour) and, if
//! enabled, asks the LLM whether the user's current state warrants a brief
//! nudge. If yes, it persists the nudge to a singleton "Proactive nudges"
//! conversation and fires an OS notification. The user opts in via Settings.
//!
//! Cadence: hourly check. Throttled to one nudge per 4h. Waking hours
//! enforced (08:00–22:00 local). Default OFF.
//!
//! Cost guardrails: when enabled, this is at most 4 LLM calls per day
//! (waking-hours / 4h throttle), with prompt caching on the system prompt.
//! On Sonnet 4.6 that's ~$0.02/day.

use std::sync::Arc;
use std::time::Duration;

use chrono::{Datelike, Timelike};
use serde::Deserialize;
use sqlx::SqlitePool;
use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;
use tokio::time::sleep;

use crate::conversation;
use crate::db::{Db, UserProfile};
use crate::health::{self, Health};
use crate::llm::{
    self, ChatWithToolsOptions, Message, ToolChoice, ToolDef,
};
pub mod observer;

use crate::secrets;
use crate::telemetry;

const TOOL_NAME: &str = "report_nudge";
const NUDGE_THREAD_TITLE: &str = "Proactive nudges";
const MIN_GAP_SECS: i64 = 4 * 60 * 60;
/// Defaults applied when the user hasn't customised their schedule.
/// Active 8am-10pm every day of the week.
const DEFAULT_START_HOUR: u32 = 8;
const DEFAULT_END_HOUR: u32 = 22;
const DEFAULT_ACTIVE_DAYS: &str = "1,2,3,4,5,6,7"; // ISO weekday: Mon=1..Sun=7

/// Don't ask for context until the user has clearly engaged with the app.
const CONTEXT_PROMPT_MIN_JOURNAL_ENTRIES: i64 = 5;
/// Don't re-ask the same question more than once a week.
const CONTEXT_PROMPT_GAP_DAYS: i64 = 7;

pub fn spawn(app: AppHandle, db: Arc<Db>, http: reqwest::Client, health: Arc<Health>) {
    tauri::async_runtime::spawn(async move {
        // Initial delay so we don't compete with onboarding/migrations.
        sleep(Duration::from_secs(120)).await;
        loop {
            if let Err(e) = tick(&app, &db, &http, &health).await {
                tracing::warn!("proactive tick: {e}");
            }
            sleep(Duration::from_secs(60 * 60)).await;
        }
    });
}

async fn is_enabled(pool: &SqlitePool) -> anyhow::Result<bool> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT value FROM meta WHERE key = ?1")
            .bind("proactive_enabled")
            .fetch_optional(pool)
            .await?;
    // Default ON: only off if the user has explicitly set the meta to "false".
    Ok(row.map(|(v,)| v != "false").unwrap_or(true))
}

/// User-configurable schedule for when proactive nudges may fire.
/// `active_days` holds ISO weekday numbers (Mon=1..Sun=7); the hour
/// window is [start, end) in the user's local timezone.
#[derive(Debug, Clone)]
pub(crate) struct Schedule {
    pub active_days: Vec<u32>,
    pub start_hour: u32,
    pub end_hour: u32,
}

impl Schedule {
    pub fn allows(&self, weekday: u32, hour: u32) -> bool {
        if !self.active_days.contains(&weekday) {
            return false;
        }
        if self.start_hour <= self.end_hour {
            // Simple range, e.g. 8..22.
            hour >= self.start_hour && hour < self.end_hour
        } else {
            // Wrap-around range, e.g. 22..6 (overnight).
            hour >= self.start_hour || hour < self.end_hour
        }
    }
}

fn parse_active_days(raw: &str) -> Vec<u32> {
    raw.split(',')
        .filter_map(|s| s.trim().parse::<u32>().ok())
        .filter(|n| (1..=7).contains(n))
        .collect()
}

pub(crate) async fn load_schedule(pool: &SqlitePool) -> anyhow::Result<Schedule> {
    let row_days: Option<(String,)> = sqlx::query_as("SELECT value FROM meta WHERE key = ?1")
        .bind("proactive_active_days")
        .fetch_optional(pool)
        .await?;
    let row_start: Option<(String,)> = sqlx::query_as("SELECT value FROM meta WHERE key = ?1")
        .bind("proactive_start_hour")
        .fetch_optional(pool)
        .await?;
    let row_end: Option<(String,)> = sqlx::query_as("SELECT value FROM meta WHERE key = ?1")
        .bind("proactive_end_hour")
        .fetch_optional(pool)
        .await?;

    let active_days = row_days
        .map(|(v,)| parse_active_days(&v))
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| parse_active_days(DEFAULT_ACTIVE_DAYS));
    let start_hour = row_start
        .and_then(|(v,)| v.parse::<u32>().ok())
        .filter(|h| *h <= 24)
        .unwrap_or(DEFAULT_START_HOUR);
    let end_hour = row_end
        .and_then(|(v,)| v.parse::<u32>().ok())
        .filter(|h| *h <= 24)
        .unwrap_or(DEFAULT_END_HOUR);

    Ok(Schedule {
        active_days,
        start_hour,
        end_hour,
    })
}

async fn last_nudge_at(pool: &SqlitePool) -> anyhow::Result<Option<chrono::DateTime<chrono::Utc>>> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT value FROM meta WHERE key = ?1")
            .bind("proactive_last_at")
            .fetch_optional(pool)
            .await?;
    Ok(row.and_then(|(v,)| chrono::DateTime::parse_from_rfc3339(&v).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc)))
}

async fn record_nudge_at(pool: &SqlitePool, when: chrono::DateTime<chrono::Utc>) -> anyhow::Result<()> {
    let iso = when.to_rfc3339();
    sqlx::query(
        "INSERT INTO meta(key, value, updated_at) VALUES ('proactive_last_at', ?1, CURRENT_TIMESTAMP)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = CURRENT_TIMESTAMP",
    )
    .bind(&iso)
    .execute(pool)
    .await?;
    Ok(())
}

async fn last_context_prompt_at(
    pool: &SqlitePool,
) -> anyhow::Result<Option<chrono::DateTime<chrono::Utc>>> {
    let row: Option<(String,)> = sqlx::query_as("SELECT value FROM meta WHERE key = ?1")
        .bind("proactive_context_prompt_last_at")
        .fetch_optional(pool)
        .await?;
    Ok(row
        .and_then(|(v,)| chrono::DateTime::parse_from_rfc3339(&v).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc)))
}

async fn record_context_prompt_at(
    pool: &SqlitePool,
    when: chrono::DateTime<chrono::Utc>,
) -> anyhow::Result<()> {
    let iso = when.to_rfc3339();
    sqlx::query(
        "INSERT INTO meta(key, value, updated_at)
         VALUES ('proactive_context_prompt_last_at', ?1, CURRENT_TIMESTAMP)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = CURRENT_TIMESTAMP",
    )
    .bind(&iso)
    .execute(pool)
    .await?;
    Ok(())
}

async fn total_journal_count(pool: &SqlitePool) -> anyhow::Result<i64> {
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM journal_entry")
        .fetch_one(pool)
        .await?;
    Ok(n)
}

/// If the user's profile is missing the free-form context blurb but they've
/// clearly been using the app, surface a one-shot nudge asking for it. Runs
/// at most once per `CONTEXT_PROMPT_GAP_DAYS`. No LLM call — text is
/// authored here so it's deterministic and free.
///
/// Returns Ok(true) if we sent a nudge (caller should short-circuit the
/// regular state-driven nudge for this tick).
async fn maybe_send_context_request(
    app: &AppHandle,
    pool: &SqlitePool,
    profile: &UserProfile,
) -> anyhow::Result<bool> {
    let blurb_set = profile
        .context_blurb
        .as_deref()
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);
    if blurb_set {
        return Ok(false);
    }

    let total = total_journal_count(pool).await?;
    if total < CONTEXT_PROMPT_MIN_JOURNAL_ENTRIES {
        return Ok(false);
    }

    let now = chrono::Utc::now();
    if let Some(last) = last_context_prompt_at(pool).await? {
        let days = (now - last).num_days();
        if days < CONTEXT_PROMPT_GAP_DAYS {
            return Ok(false);
        }
    }

    let first = profile.first_name();
    let text = format!(
        "Hey {first} — I'd be more useful with a bit of context about your work. \
         What does your org do, who do you serve, and what activities should I pay attention to? \
         You can either reply here and I'll pull it from your message, or drop it into \
         Settings → Context for Travis."
    );

    let conv_id = get_or_create_nudge_conversation(pool).await?;
    let payload = serde_json::json!({
        "kind": "context_request",
        "severity": "info",
    })
    .to_string();
    let _ = conversation::append(pool, conv_id, "assistant", &text, Some(&payload)).await;
    let _ = conversation::set_status(pool, conv_id, "awaiting_user").await;

    let _ = app
        .notification()
        .builder()
        .title("Travis")
        .body("Tell me a bit about your work — I'll work better with context.")
        .show();

    record_context_prompt_at(pool, now).await?;
    telemetry::emit(
        pool,
        "context_request_sent",
        serde_json::json!({"journal_total": total}),
    )
    .await;
    Ok(true)
}

#[derive(Debug, Default)]
struct StateSummary {
    open_tasks_total: i64,
    overdue_tasks: Vec<(i64, String, Option<String>)>,
    awaiting_threads: i64,
    awaiting_first_title: Option<String>,
    journal_24h: i64,
    top_gaps: Vec<(String, i64)>,
}

impl StateSummary {
    fn is_quiet(&self) -> bool {
        self.open_tasks_total == 0
            && self.overdue_tasks.is_empty()
            && self.awaiting_threads == 0
            && self.journal_24h == 0
            && self.top_gaps.is_empty()
    }

    fn render(&self, today: &str) -> String {
        let overdue_block = if self.overdue_tasks.is_empty() {
            "(none)".to_string()
        } else {
            self.overdue_tasks
                .iter()
                .take(5)
                .map(|(id, title, due)| {
                    let due_str = due.as_deref().unwrap_or("—");
                    format!("- [{id}] {title} (due {due_str})")
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        let gaps_block = if self.top_gaps.is_empty() {
            "(none)".to_string()
        } else {
            self.top_gaps
                .iter()
                .map(|(cap, count)| format!("- {cap} ({count}×)"))
                .collect::<Vec<_>>()
                .join("\n")
        };
        let waiting = match (self.awaiting_threads, &self.awaiting_first_title) {
            (0, _) => "(none)".to_string(),
            (n, Some(t)) => format!("{n} (most recent: \"{t}\")"),
            (n, None) => format!("{n}"),
        };
        format!(
            "Today: {today}\n\nOpen tasks: {open}\nOverdue:\n{overdue}\n\nThreads waiting on the user: {waiting}\n\nNotes captured in last 24h: {j24}\n\nOutstanding capability requests:\n{gaps}",
            open = self.open_tasks_total,
            overdue = overdue_block,
            waiting = waiting,
            j24 = self.journal_24h,
            gaps = gaps_block,
            today = today,
        )
    }
}

async fn build_state_summary(pool: &SqlitePool, today_iso: &str) -> anyhow::Result<StateSummary> {
    let mut s = StateSummary::default();
    let (open,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM task WHERE status = 'open'")
        .fetch_one(pool)
        .await?;
    s.open_tasks_total = open;

    let overdue: Vec<(i64, String, Option<String>)> = sqlx::query_as(
        "SELECT id, title, due_at FROM task
         WHERE status = 'open' AND due_at IS NOT NULL AND due_at < ?1
         ORDER BY due_at ASC, priority DESC LIMIT 5",
    )
    .bind(today_iso)
    .fetch_all(pool)
    .await?;
    s.overdue_tasks = overdue;

    let (awaiting,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM conversation WHERE status = 'awaiting_user'",
    )
    .fetch_one(pool)
    .await?;
    s.awaiting_threads = awaiting;

    if awaiting > 0 {
        let row: Option<(Option<String>,)> = sqlx::query_as(
            "SELECT title FROM conversation
             WHERE status = 'awaiting_user'
             ORDER BY updated_at DESC LIMIT 1",
        )
        .fetch_optional(pool)
        .await?;
        s.awaiting_first_title = row.and_then(|(t,)| t);
    }

    let (j24,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM journal_entry
         WHERE datetime(created_at) >= datetime('now', '-24 hours')",
    )
    .fetch_one(pool)
    .await?;
    s.journal_24h = j24;

    let gaps: Vec<(String, i64)> = sqlx::query_as(
        "SELECT capability, COUNT(*) FROM app_feedback
         WHERE addressed_at IS NULL
         GROUP BY capability ORDER BY COUNT(*) DESC LIMIT 3",
    )
    .fetch_all(pool)
    .await?;
    s.top_gaps = gaps;

    Ok(s)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NudgeOutput {
    should_nudge: bool,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    severity: Option<String>,
}

fn build_nudge_tool() -> ToolDef {
    ToolDef {
        name: TOOL_NAME.into(),
        description:
            "Decide whether to send a proactive nudge to the user right now and, if so, what to say. Bias toward shouldNudge=false — quiet is good. Only nudge when there's a clear, specific reason.".into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "shouldNudge": {
                    "type": "boolean",
                    "description": "True only if there's a SPECIFIC, non-trivial reason worth interrupting the user."
                },
                "text": {
                    "type": ["string", "null"],
                    "description": "1-2 sentences max. Friendly, conversational, professional. Reference SPECIFIC items by name. Bad: 'You have things to do.' Good: 'Maria's hours are still pending review — third day. Want me to draft a follow-up?'"
                },
                "kind": {
                    "type": ["string", "null"],
                    "enum": ["check_in", "overdue", "follow_up", "celebration", null]
                },
                "severity": {
                    "type": ["string", "null"],
                    "enum": ["low", "med", "high", null]
                }
            },
            "required": ["shouldNudge"]
        }),
    }
}

fn build_system_prompt(
    profile: &UserProfile,
    pack_fragment: &str,
    workspace_block: &str,
) -> String {
    let first = profile.first_name();
    let mut prompt = format!(
        r#"You are Travis, a personal operations assistant.

{user_context}

You are running on a quiet timer in the background. {first} hasn't typed anything recently. Your job RIGHT NOW is to decide whether their current state warrants a brief nudge — or whether to stay silent.

GUIDING PRINCIPLE: quiet is the default. {first}'s attention is precious. Only nudge when there's a specific, concrete, useful thing to say. NEVER nudge with generic content like "Just checking in!" or "How's it going?".

GOOD reasons to nudge:
- An overdue task with a specific name and how long it's been overdue.
- A clarifying question Travis asked earlier that's been hanging in an awaiting_user thread for hours.
- A pattern in capability requests worth surfacing ("you've asked to send email three times — want a workaround?").
- A visible win to celebrate succinctly ("nice — three closed today").

BAD reasons to nudge:
- "You have N open tasks." (generic)
- Just to remind them you exist.
- Anything that doesn't reference a specific item by name.

Personality: warm, professional, terse. Reference items by name. Never sycophantic. Use contractions. Match the kind of brief check-in a sharp colleague would slip into chat — not corporate copy.

Output via the report_nudge tool exactly once. If unsure, set shouldNudge=false. There is no penalty for staying silent."#,
        user_context = profile.context_block(),
        first = first,
    );

    // Append vertical-pack guidance (PACKS_AUDIT.md step 10).
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

async fn get_or_create_nudge_conversation(pool: &SqlitePool) -> anyhow::Result<i64> {
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM conversation WHERE kind = 'nudge' ORDER BY id ASC LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;
    if let Some((id,)) = row {
        return Ok(id);
    }
    // Nudge thread lives in the active workspace at the time of
    // creation. Subsequent nudges land in the same thread regardless
    // of workspace switches (it's identified by kind='nudge'); a
    // future polish could create one nudge thread per workspace.
    let active_id = crate::workspaces::read_active_id(pool).await?;
    let conv = conversation::open(pool, active_id, "nudge", Some(NUDGE_THREAD_TITLE))
        .await
        .map_err(|e| anyhow::anyhow!("create nudge thread: {e}"))?;
    Ok(conv.id)
}

async fn tick(
    app: &AppHandle,
    db: &Db,
    http: &reqwest::Client,
    health: &Health,
) -> anyhow::Result<()> {
    let pool = &db.pool;

    if !is_enabled(pool).await? {
        return Ok(());
    }

    // Skip the tick entirely if we're offline or the LLM is in a known-bad
    // state. The user already has a banner; no point burning a retry that's
    // going to fail and re-fire the same notification.
    if health.is_blocked() {
        return Ok(());
    }

    // Schedule gate — user-configurable in Settings → Proactive nudges.
    let local_now = chrono::Local::now();
    let hour = local_now.time().hour();
    let weekday = local_now.weekday().number_from_monday(); // Mon=1..Sun=7
    let schedule = load_schedule(pool).await?;
    if !schedule.allows(weekday, hour) {
        return Ok(());
    }

    // Profile must exist before we can do anything — pulled early so the
    // context-request short-circuit below has access to it.
    let profile = db
        .user_profile()
        .await
        .map_err(|e| anyhow::anyhow!("read profile: {e}"))?
        .ok_or_else(|| anyhow::anyhow!("no profile"))?;

    // Context-request short-circuit. Independent of the regular operational
    // throttle: at most once per CONTEXT_PROMPT_GAP_DAYS regardless of when
    // the last regular nudge fired. If we send one, we're done for this tick.
    if maybe_send_context_request(app, pool, &profile).await? {
        return Ok(());
    }

    // Throttle for the regular state-driven nudges.
    let now_utc = chrono::Utc::now();
    if let Some(last) = last_nudge_at(pool).await? {
        let elapsed = (now_utc - last).num_seconds();
        if elapsed < MIN_GAP_SECS {
            return Ok(());
        }
    }

    // Pull state. If nothing interesting at all, stay quiet.
    let today = format!(
        "{:04}-{:02}-{:02}",
        local_now.year(),
        local_now.month(),
        local_now.day()
    );
    let state = build_state_summary(pool, &today).await?;
    if state.is_quiet() {
        return Ok(());
    }
    let api_key = match profile.llm_provider.as_str() {
        "claude" | "openai" => secrets::get_api_key(&profile.llm_provider),
        _ => None,
    };
    let provider = llm::build(
        &profile.llm_provider,
        api_key.as_deref(),
        profile.ollama_url.as_deref(),
        profile.model.as_deref(),
        http.clone(),
    )?;

    let tool = build_nudge_tool();
    let mut user_msg = state.render(&today);

    // Append observer signals (BRAIN.md capability #5). The state
    // summary captures task / reminder counts; the observer surfaces
    // graph-shaped events: mention spikes on previously-dormant
    // entities, signed sheets ready to invoice, stale invoice
    // drafts. These give the nudge LLM concrete, specific-by-name
    // candidate reasons instead of inventing them.
    let visible_ids: Vec<i64> = {
        let app_state = app.state::<crate::AppState>();
        let guard = app_state.workspace.read().await;
        guard.visible_ids.clone()
    };
    let observations = observer::scan(pool, &visible_ids).await;
    if !observations.is_empty() {
        user_msg.push_str("\n\n");
        user_msg.push_str(&observer::format_for_prompt(&observations));
    }

    // Rhythm-aware timing (BRAIN.md capability #5). If the user has
    // a derived peak_window and we're outside it by more than the
    // typical waking hours, bias the LLM toward silence — adds an
    // explicit hint so this isn't a hard block, just guidance.
    if let Some(model_json) = profile.derived_model_json.as_deref() {
        if let Some(model) = crate::persona::user_model::parse(model_json) {
            if !model.peak_window.is_empty() {
                let current_hour = local_now.hour() as i64;
                let in_peak = is_hour_in_window(current_hour, &model.peak_window);
                if !in_peak {
                    user_msg.push_str(&format!(
                        "\n\nRHYTHM HINT: it's {current_hour}:00 local; the user typically captures during {}. Outside that window, weight your decision toward staying silent unless the observation is urgent.\n",
                        model.peak_window
                    ));
                }
            }
        }
    }

    let turn_res = provider
        .chat_with_tools(
            vec![Message::user(user_msg)],
            ChatWithToolsOptions {
                system: Some({
                    // Pull the runtime-enabled pack list off AppState
                    // so disabled packs don't contribute to the
                    // proactive nudge prompt.
                    let app_state = app.state::<crate::AppState>();
                    let pack_fragment =
                        crate::packs::prompt_fragment(&app_state.enabled_packs);
                    let active_id = app_state.workspace.read().await.active_id;
                    let workspace_block =
                        crate::workspaces::prompt_context_block(pool, active_id).await;
                    build_system_prompt(&profile, &pack_fragment, &workspace_block)
                }),
                cache_system: true,
                temperature: Some(0.5),
                max_tokens: Some(400),
                tools: vec![tool],
                tool_choice: Some(ToolChoice::Specific(TOOL_NAME.into())),
            },
        )
        .await;

    let turn = match turn_res {
        Ok(t) => {
            // A successful background call means whatever degraded us is
            // resolved — clear so the banner goes away and proactive +
            // user-facing flows resume.
            health.clear(app);
            t
        }
        Err(e) => {
            let kind = health::classify_llm_error(&e.to_string());
            health.report(app, kind, format!("Proactive nudge failed: {e}"));
            return Err(e);
        }
    };

    let call = match turn.tool_calls.into_iter().find(|c| c.name == TOOL_NAME) {
        Some(c) => c,
        None => return Ok(()),
    };
    let nudge: NudgeOutput = match serde_json::from_value(call.input) {
        Ok(n) => n,
        Err(e) => {
            tracing::warn!("nudge parse: {e}");
            return Ok(());
        }
    };

    if !nudge.should_nudge {
        return Ok(());
    }
    let text = match nudge.text.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) {
        Some(t) => t,
        None => return Ok(()),
    };

    // Persist.
    let conv_id = get_or_create_nudge_conversation(pool).await?;
    let payload = serde_json::json!({
        "kind": nudge.kind,
        "severity": nudge.severity,
    })
    .to_string();
    let _ = conversation::append(pool, conv_id, "assistant", &text, Some(&payload)).await;
    let _ = conversation::set_status(pool, conv_id, "awaiting_user").await;

    // OS notification — clip to a sensible body length.
    let body: String = text.chars().take(200).collect();
    let _ = app
        .notification()
        .builder()
        .title("Travis")
        .body(&body)
        .show();

    record_nudge_at(pool, now_utc).await?;

    telemetry::emit(
        pool,
        "nudge_generated",
        serde_json::json!({
            "kind": nudge.kind,
            "severity": nudge.severity,
            "open_tasks": state.open_tasks_total,
            "overdue": state.overdue_tasks.len(),
        }),
    )
    .await;

    tracing::info!(
        "proactive: nudged ({}, {})",
        nudge.kind.as_deref().unwrap_or("?"),
        nudge.severity.as_deref().unwrap_or("?")
    );

    Ok(())
}

/// Is the given hour inside any of the windows in the
/// user_model.peak_window string? Format examples: "9am-11am",
/// "9am-11am, 4pm-6pm", "12pm". Returns true if `hour` matches any
/// single hour or falls within any range. Best-effort parser —
/// returns true on malformed input so a parse error doesn't
/// suppress nudges entirely.
fn is_hour_in_window(hour: i64, window: &str) -> bool {
    if window.trim().is_empty() {
        return true;
    }
    for part in window.split(|c| c == ',' || c == ';') {
        let p = part.trim().replace(['–', '—'], "-");
        if let Some((a, b)) = p.split_once('-') {
            let (Some(start), Some(end)) = (parse_clock(a), parse_clock(b)) else { continue };
            // The end hour is exclusive in our format (e.g. "9am-11am" = [9, 11)).
            if hour >= start && hour < end {
                return true;
            }
        } else if let Some(h) = parse_clock(&p) {
            if hour == h {
                return true;
            }
        }
    }
    false
}

fn parse_clock(s: &str) -> Option<i64> {
    let s = s.trim().to_lowercase();
    let (digits, suffix) = if let Some(rest) = s.strip_suffix("am") {
        (rest.trim(), "am")
    } else if let Some(rest) = s.strip_suffix("pm") {
        (rest.trim(), "pm")
    } else {
        (s.as_str(), "")
    };
    let h: i64 = digits.parse().ok()?;
    let h = match suffix {
        "am" => if h == 12 { 0 } else { h },
        "pm" => if h == 12 { 12 } else { h + 12 },
        _ => h,
    };
    if (0..=23).contains(&h) {
        Some(h)
    } else {
        None
    }
}
