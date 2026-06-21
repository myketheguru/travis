//! Tier 1 inbox triage — local-side observer that reads the user's
//! inbox via whichever provider they've connected (Gmail or Outlook),
//! classifies each new message into a tight urgency bucket, persists
//! the triage, and fires proactive notifications for urgent items.
//!
//! Designed to run every 5 minutes while the desktop is open. Cloud
//! takeover (Tier 2) happens via `travis-cloud`'s WorkflowLoop DO once
//! the user grants the appropriate read scope server-side.
//!
//! Scope intentionally narrow:
//!   - Read recent messages since the last successful run
//!   - Hand the (subject, snippet, body_preview) to the LLM with a
//!     fixed classification prompt
//!   - Write one row per message to `inbox_triage`
//!   - For URGENT rows, fire one OS notification per scan
//!
//! What this DOES NOT do (Tier 3 is where these come in):
//!   - Draft replies (just classify + summarize)
//!   - Take any action (no auto-send, no auto-archive)
//!   - Cross-thread reasoning (single-message classification)

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::email::gmail::InboxMessage;
use crate::llm::Message as LlmMessage;

const META_LAST_RUN: &str = "inbox_observer_last_run";
const SCAN_MAX_PER_TICK: usize = 30;
const INITIAL_LOOKBACK_HOURS: i64 = 24;

/// One classified inbox message — projection of the raw provider
/// message plus the LLM-derived triage fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TriagedMessage {
    pub provider_message_id: String,
    pub from_email: String,
    pub from_name: Option<String>,
    pub subject: String,
    pub snippet: String,
    pub received_at: String,
    pub urgency: Urgency,
    /// One-sentence summary the user can scan at a glance.
    pub summary_one_line: String,
    /// What Travis suggests the user might do. Free-form.
    pub suggested_next_step: Option<String>,
    pub provider: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Urgency {
    Urgent,
    Important,
    Fyi,
    Noise,
}

impl Urgency {
    fn from_label(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "urgent" => Self::Urgent,
            "important" => Self::Important,
            "noise" | "spam" | "promo" | "promotional" => Self::Noise,
            _ => Self::Fyi,
        }
    }
    fn as_str(&self) -> &'static str {
        match self {
            Self::Urgent => "urgent",
            Self::Important => "important",
            Self::Fyi => "fyi",
            Self::Noise => "noise",
        }
    }
}

/// Run one scan tick. Loads the last-run timestamp from meta, fetches
/// new messages from the connected provider, classifies them, and
/// persists triage. Returns the number of newly-triaged messages.
///
/// Best-effort — failures log + return 0 rather than propagating.
/// The observer worker treats every tick as independent.
pub async fn scan_once(
    pool: &SqlitePool,
    http: &reqwest::Client,
    workspace_id: i64,
) -> Result<usize> {
    // v0.21.6 Tier 1/2 dedup guard.
    //
    // When the user has enrolled their Gmail server-side (Tier 2), the
    // cloud WorkflowLoop runs the same triage on a hosted schedule.
    // Running both layers would double-classify every message — wasted
    // tokens AND double-notifications. Bail on the desktop observer
    // when cloud already has it. We treat any error fetching the
    // accounts list as "cloud probably not enrolled" — fail-open so a
    // transient cloud blip doesn't silently break local triage.
    if cloud_has_inbox(http).await {
        tracing::debug!("inbox observer: cloud has gmail enrolled, skipping local tick");
        return Ok(0);
    }

    let last_run = load_last_run(pool).await;
    let since = last_run.unwrap_or_else(|| Utc::now() - chrono::Duration::hours(INITIAL_LOOKBACK_HOURS));

    // Try Gmail first, fall back to Outlook. Multiple-provider users
    // (rare) will see only their Gmail; we'll add a per-account loop
    // when that comes up.
    let (messages, provider) = match crate::email::gmail::list_recent(pool, http, since, SCAN_MAX_PER_TICK).await {
        Ok(m) if !m.is_empty() => (m, "gmail"),
        Ok(_) => (Vec::new(), "gmail"),
        Err(gerr) => {
            tracing::debug!("gmail list failed: {gerr}");
            match crate::email::outlook::list_recent(pool, http, since, SCAN_MAX_PER_TICK).await {
                Ok(m) => (m, "outlook"),
                Err(oerr) => {
                    tracing::debug!("outlook list also failed: {oerr}");
                    return Ok(0);
                }
            }
        }
    };

    if messages.is_empty() {
        save_last_run(pool, Utc::now()).await;
        return Ok(0);
    }

    let mut triaged_count = 0usize;
    let mut urgent_for_notification: Vec<TriagedMessage> = Vec::new();

    for msg in &messages {
        // Skip if we've already triaged this message id (idempotency
        // guard for clock skew + overlapping windows).
        if message_already_triaged(pool, workspace_id, &msg.id, provider).await {
            continue;
        }

        match classify_one(&msg, provider).await {
            Ok(triage) => {
                if triage.urgency == Urgency::Noise {
                    // Don't persist noise — keeps the table lean and
                    // the splash feed signal-rich.
                    continue;
                }
                if let Err(e) = persist_triage(pool, workspace_id, provider, &msg, &triage).await {
                    tracing::warn!("persist inbox triage: {e}");
                    continue;
                }
                if triage.urgency == Urgency::Urgent {
                    urgent_for_notification.push(triage.clone());
                }
                triaged_count += 1;
            }
            Err(e) => {
                tracing::warn!("classify inbox message {}: {e}", msg.id);
            }
        }
    }

    // Bookmark the highest received_at we saw so the next tick picks
    // up cleanly.
    let high_water = messages
        .iter()
        .filter_map(|m| DateTime::parse_from_rfc3339(&m.received_at).ok())
        .map(|d| d.with_timezone(&Utc))
        .max()
        .unwrap_or_else(Utc::now);
    save_last_run(pool, high_water).await;

    // Surface urgent messages via the proactive notification surface
    // exactly the same way proactive::observer does. One notification
    // per scan, batched if multiple urgents land.
    if !urgent_for_notification.is_empty() {
        if let Err(e) = fire_urgent_notification(&urgent_for_notification).await {
            tracing::warn!("fire urgent inbox notification: {e}");
        }
    }

    Ok(triaged_count)
}

async fn classify_one(msg: &InboxMessage, provider: &str) -> Result<TriagedMessage> {
    // Cheap, deterministic prompt. We want fast classification, not
    // creative summarization — explicit format spec, no system prompt
    // about Travis's identity (this is a utility call).
    let prompt = format!(
        "Classify this email for a busy knowledge worker. Reply in exactly this JSON format:\n\
         {{\"urgency\":\"urgent|important|fyi|noise\",\"summary\":\"one sentence, plain language\",\"next_step\":\"one short suggested action or empty\"}}\n\n\
         Rules:\n\
         - \"urgent\" = needs a response/decision today (real meeting changes, time-sensitive client asks, account/security alerts).\n\
         - \"important\" = matters but not today (project updates, FYIs from people who matter).\n\
         - \"fyi\" = nice to know, no action needed.\n\
         - \"noise\" = promo, newsletter, automated digest, social notification.\n\
         - Be conservative with \"urgent\" — only a few per day deserve it.\n\
         - next_step is optional; leave empty if nothing meaningful.\n\n\
         FROM: {} <{}>\n\
         SUBJECT: {}\n\
         BODY:\n{}",
        msg.from_name.clone().unwrap_or_else(|| "Unknown".to_string()),
        msg.from_email,
        msg.subject,
        msg.body_preview.chars().take(2500).collect::<String>(),
    );

    // Use the default Travis Cloud provider for inbox classification.
    // The user is signed in (we wouldn't be reading inbox otherwise);
    // their tier policy applies. BYOK users use their own key.
    //
    // Bound to `llm` (not `provider`) on purpose: the function's `provider`
    // arg is the email-provider name ("gmail" / "outlook") which we still
    // need below when constructing the TriagedMessage. v0.21.6 had a
    // shadowing bug here that CI caught.
    let http = reqwest::Client::new();
    let llm = crate::llm::build("travis_cloud", None, None, None, http)
        .context("build llm provider for inbox classification")?;
    let resp = llm
        .chat(vec![LlmMessage::user(prompt)], Default::default())
        .await
        .context("llm classify call")?;

    let text = resp.content.trim();
    // Strip optional code-fence wrapping.
    let json_slice = strip_code_fence(text);
    let parsed: ClassifyResp = serde_json::from_str(json_slice)
        .with_context(|| format!("parse classify response: {text}"))?;

    Ok(TriagedMessage {
        provider_message_id: msg.id.clone(),
        from_email: msg.from_email.clone(),
        from_name: msg.from_name.clone(),
        subject: msg.subject.clone(),
        snippet: msg.snippet.clone(),
        received_at: msg.received_at.clone(),
        urgency: Urgency::from_label(&parsed.urgency),
        summary_one_line: parsed.summary,
        suggested_next_step: parsed.next_step.filter(|s| !s.trim().is_empty()),
        provider: provider.to_string(),
    })
}

#[derive(Deserialize)]
struct ClassifyResp {
    urgency: String,
    summary: String,
    #[serde(default)]
    next_step: Option<String>,
}

fn strip_code_fence(s: &str) -> &str {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix("```json").or_else(|| s.strip_prefix("```")) {
        if let Some(idx) = rest.rfind("```") {
            return rest[..idx].trim();
        }
        return rest.trim();
    }
    s
}

async fn message_already_triaged(
    pool: &SqlitePool,
    workspace_id: i64,
    provider_message_id: &str,
    provider: &str,
) -> bool {
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM inbox_triage
         WHERE workspace_id = ?1 AND provider = ?2 AND provider_message_id = ?3 LIMIT 1",
    )
    .bind(workspace_id)
    .bind(provider)
    .bind(provider_message_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    row.is_some()
}

async fn persist_triage(
    pool: &SqlitePool,
    workspace_id: i64,
    provider: &str,
    raw: &InboxMessage,
    triage: &TriagedMessage,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO inbox_triage
            (workspace_id, provider, provider_message_id, thread_id,
             from_email, from_name, subject, received_at, is_unread,
             urgency, summary_one_line, suggested_next_step)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
         ON CONFLICT(workspace_id, provider, provider_message_id) DO NOTHING",
    )
    .bind(workspace_id)
    .bind(provider)
    .bind(&raw.id)
    .bind(&raw.thread_id)
    .bind(&raw.from_email)
    .bind(&raw.from_name)
    .bind(&raw.subject)
    .bind(&raw.received_at)
    .bind(if raw.is_unread { 1 } else { 0 })
    .bind(triage.urgency.as_str())
    .bind(&triage.summary_one_line)
    .bind(&triage.suggested_next_step)
    .execute(pool)
    .await?;
    Ok(())
}

async fn load_last_run(pool: &SqlitePool) -> Option<DateTime<Utc>> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT value FROM meta WHERE key = ?1",
    )
    .bind(META_LAST_RUN)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    row.and_then(|(s,)| DateTime::parse_from_rfc3339(&s).ok())
        .map(|d| d.with_timezone(&Utc))
}

async fn save_last_run(pool: &SqlitePool, ts: DateTime<Utc>) {
    let _ = sqlx::query(
        "INSERT INTO meta (key, value, updated_at) VALUES (?1, ?2, CURRENT_TIMESTAMP)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = CURRENT_TIMESTAMP",
    )
    .bind(META_LAST_RUN)
    .bind(ts.to_rfc3339())
    .execute(pool)
    .await;
}

async fn fire_urgent_notification(_urgent: &[TriagedMessage]) -> Result<()> {
    // The notification surface lives in proactive::mod; rather than
    // duplicate the OS-notification plumbing here, we emit an event
    // the existing proactive worker consumes. For the v1 plumbing
    // this is a no-op stub — the splash feed picks up persisted
    // urgent rows on its next pull, which is enough for v1. Real OS
    // notifications can land in a follow-up when we wire the
    // proactive::observer side too.
    Ok(())
}

/// Read recent triaged messages for the splash feed. Caps at `limit`,
/// ordered by received_at DESC. Excludes noise (we don't persist it
/// anyway, defensive belt-and-suspenders).
pub async fn recent(
    pool: &SqlitePool,
    workspace_id: i64,
    limit: i64,
) -> Result<Vec<TriagedMessage>> {
    let rows: Vec<(String, String, Option<String>, String, String, String, String, Option<String>, String)> = sqlx::query_as(
        "SELECT provider_message_id, from_email, from_name, subject, received_at,
                urgency, summary_one_line, suggested_next_step, provider
         FROM inbox_triage
         WHERE workspace_id = ?1 AND urgency != 'noise'
         ORDER BY datetime(received_at) DESC LIMIT ?2",
    )
    .bind(workspace_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(pid, fe, fn_, subj, recv, urg, summary, step, prov)| TriagedMessage {
            provider_message_id: pid,
            from_email: fe,
            from_name: fn_,
            subject: subj,
            snippet: String::new(),
            received_at: recv,
            urgency: Urgency::from_label(&urg),
            summary_one_line: summary,
            suggested_next_step: step,
            provider: prov,
        })
        .collect())
}

/// Tier 1/2 dedup helper — return true when the user has an active
/// Gmail row in connected_account on the cloud. The cloud loop will
/// triage on its own when that's the case; the desktop should not.
///
/// Best-effort. Network or auth failures return false (fail-open) so
/// a transient cloud blip doesn't silently break local triage.
///
/// Cached at process scope with a 10-minute TTL — the cloud roundtrip
/// itself is fast but we shouldn't pay it on every 5-min tick.
async fn cloud_has_inbox(http: &reqwest::Client) -> bool {
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    static CACHE: Mutex<Option<(Instant, bool)>> = Mutex::new(None);
    const TTL: Duration = Duration::from_secs(10 * 60);

    if let Ok(guard) = CACHE.lock() {
        if let Some((at, v)) = *guard {
            if at.elapsed() < TTL {
                return v;
            }
        }
    }
    let accounts = match crate::cloud::connected_accounts(http).await {
        Ok(a) => a,
        Err(_) => return false,
    };
    let has = accounts
        .iter()
        .any(|a| a.provider == "gmail" && a.is_active != 0);
    if let Ok(mut guard) = CACHE.lock() {
        *guard = Some((Instant::now(), has));
    }
    has
}

/// Spawn the inbox observer background loop. Runs every 5 minutes
/// (configurable via INBOX_SCAN_INTERVAL_SECS env if testing). First
/// tick fires after 90s to let the rest of setup() finish.
pub fn spawn(pool: SqlitePool, http: reqwest::Client, default_workspace_id: i64) {
    let interval_secs: u64 = std::env::var("INBOX_SCAN_INTERVAL_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(5 * 60);
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(90)).await;
        loop {
            match scan_once(&pool, &http, default_workspace_id).await {
                Ok(n) if n > 0 => tracing::info!("inbox observer: triaged {n} new messages"),
                Ok(_) => {}
                Err(e) => tracing::warn!("inbox observer scan failed: {e}"),
            }
            tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
        }
    });
}
