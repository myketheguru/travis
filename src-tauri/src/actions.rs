use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::conversation;
use crate::domain::task::{self, TaskInput};
use crate::reminders::{self, ReminderInput};
use crate::AppState;

// ---------- Stored action ----------

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ProposedAction {
    pub id: i64,
    pub conversation_id: i64,
    pub kind: String,
    pub rationale: Option<String>,
    pub params_json: String,
    pub status: String,
    pub result_json: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProposedActionFilter {
    pub conversation_id: Option<i64>,
    pub status: Option<String>,
}

pub async fn record(
    pool: &SqlitePool,
    conversation_id: i64,
    kind: &str,
    rationale: Option<&str>,
    params_json: &str,
) -> Result<ProposedAction, sqlx::Error> {
    let id = sqlx::query(
        "INSERT INTO proposed_action (conversation_id, kind, rationale, params_json)
         VALUES (?1, ?2, ?3, ?4)",
    )
    .bind(conversation_id)
    .bind(kind)
    .bind(rationale)
    .bind(params_json)
    .execute(pool)
    .await?
    .last_insert_rowid();
    fetch(pool, id).await
}

pub async fn fetch(pool: &SqlitePool, id: i64) -> Result<ProposedAction, sqlx::Error> {
    sqlx::query_as::<_, ProposedAction>(
        "SELECT id, conversation_id, kind, rationale, params_json, status, result_json,
                created_at, updated_at
         FROM proposed_action WHERE id = ?1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
}

async fn list(pool: &SqlitePool, filter: ProposedActionFilter) -> Result<Vec<ProposedAction>, sqlx::Error> {
    sqlx::query_as::<_, ProposedAction>(
        "SELECT id, conversation_id, kind, rationale, params_json, status, result_json,
                created_at, updated_at
         FROM proposed_action
         WHERE (?1 IS NULL OR conversation_id = ?1)
           AND (?2 IS NULL OR status = ?2)
         ORDER BY id DESC LIMIT 200",
    )
    .bind(filter.conversation_id)
    .bind(filter.status)
    .fetch_all(pool)
    .await
}

async fn set_status(
    pool: &SqlitePool,
    id: i64,
    status: &str,
    result_json: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE proposed_action SET status = ?1, result_json = ?2,
            updated_at = CURRENT_TIMESTAMP WHERE id = ?3",
    )
    .bind(status)
    .bind(result_json)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

// ---------- Action params ----------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeferTaskParams {
    task_id: i64,
    new_due_at: String,
}

// ---------- Apply outcomes ----------

/// Outcome of applying a proposed action. Returned by every
/// [`ActionHandler`] implementation; `confirm_action` writes
/// `message` to the thread and `json` to `proposed_action.result_json`.
pub struct Applied {
    /// Human-readable assistant message appended to the thread.
    pub message: String,
    /// JSON serialized into proposed_action.result_json for audit + later UI lookup.
    pub json: String,
}

async fn apply_defer_task(pool: &SqlitePool, params_json: &str) -> anyhow::Result<Applied> {
    let p: DeferTaskParams = serde_json::from_str(params_json)?;
    if p.new_due_at.trim().is_empty() {
        anyhow::bail!("newDueAt is required");
    }
    let existing = task::fetch_one(pool, p.task_id)
        .await
        .map_err(|e| anyhow::anyhow!("task {} not found: {e}", p.task_id))?;

    let updated = task::upsert(
        pool,
        TaskInput {
            id: Some(existing.id),
            title: existing.title.clone(),
            description: existing.description.clone(),
            priority: Some(existing.priority),
            due_at: Some(p.new_due_at.clone()),
            entity_id: existing.entity_id,
            link_kind: existing.link_kind.clone(),
            link_id: existing.link_id,
            source: Some(existing.source.clone()),
        },
    )
    .await
    .map_err(|e| anyhow::anyhow!("update task: {e}"))?;

    Ok(Applied {
        message: format!("Moved \"{}\" to {}.", updated.title, p.new_due_at),
        json: serde_json::json!({
            "taskId": updated.id,
            "newDueAt": p.new_due_at,
        })
        .to_string(),
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetReminderParams {
    text: String,
    remind_at: String,
    #[serde(default)]
    kind: Option<String>,
}

async fn apply_set_reminder(pool: &SqlitePool, params_json: &str) -> anyhow::Result<Applied> {
    let p: SetReminderParams = serde_json::from_str(params_json)?;
    let text = p.text.trim();
    if text.is_empty() {
        anyhow::bail!("reminder text required");
    }
    let when = p.remind_at.trim();
    if when.is_empty() {
        anyhow::bail!("remindAt required");
    }
    let reminder = reminders::upsert(
        pool,
        ReminderInput {
            id: None,
            text: text.to_string(),
            kind: p.kind.or_else(|| Some("time".into())),
            remind_at: Some(when.to_string()),
            source: Some("agent".into()),
            link_kind: None,
            link_id: None,
        },
    )
    .await
    .map_err(|e| anyhow::anyhow!("create reminder: {e}"))?;

    Ok(Applied {
        message: format!("Reminder set for {when} — {text}"),
        json: serde_json::json!({
            "reminderId": reminder.id,
            "remindAt": when,
        })
        .to_string(),
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WriteClipboardParams {
    text: String,
}

async fn apply_write_clipboard(
    app: &AppHandle,
    params_json: &str,
) -> anyhow::Result<Applied> {
    let p: WriteClipboardParams = serde_json::from_str(params_json)?;
    let text = p.text;
    if text.is_empty() {
        anyhow::bail!("text required");
    }
    app.clipboard()
        .write_text(text.clone())
        .map_err(|e| anyhow::anyhow!("clipboard write: {e}"))?;
    let preview: String = text.chars().take(80).collect();
    let elided = if text.chars().count() > 80 { "…" } else { "" };
    Ok(Applied {
        message: format!("Copied to clipboard: \"{preview}{elided}\""),
        json: serde_json::json!({
            "chars": text.chars().count(),
        })
        .to_string(),
    })
}

// ---------- Update profile context ----------
//
// Lets the LLM propose writing what the user just told it about their work
// into user_profile.context_blurb / communication_style. The user has to
// confirm via the action card — we don't write to the profile silently.

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateProfileContextParams {
    /// New free-form work blurb. Optional — pass only when updating.
    #[serde(default)]
    context_blurb: Option<String>,
    /// New voice preference. Optional — pass only when updating.
    #[serde(default)]
    communication_style: Option<String>,
}

async fn apply_update_profile_context(
    pool: &SqlitePool,
    params_json: &str,
) -> anyhow::Result<Applied> {
    let p: UpdateProfileContextParams = serde_json::from_str(params_json)?;

    let new_blurb = p.context_blurb.as_ref().map(|s| s.trim().to_string());
    let new_style = p.communication_style.as_ref().map(|s| s.trim().to_string());

    if new_blurb.is_none() && new_style.is_none() {
        anyhow::bail!("nothing to update — pass contextBlurb and/or communicationStyle");
    }

    // Pull existing profile so we can preserve fields we're not touching.
    let existing: Option<crate::db::UserProfile> = {
        let row: Option<(
            String,
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
        )> = sqlx::query_as(
            "SELECT name, role, org, llm_provider, ollama_url, model,
                    context_blurb, communication_style
             FROM user_profile WHERE id = 1",
        )
        .fetch_optional(pool)
        .await?;
        row.map(
            |(
                name,
                role,
                org,
                llm_provider,
                ollama_url,
                model,
                context_blurb,
                communication_style,
            )| crate::db::UserProfile {
                name,
                role,
                org,
                llm_provider,
                ollama_url,
                model,
                context_blurb,
                communication_style,
            },
        )
    };
    let mut prof = existing.ok_or_else(|| anyhow::anyhow!("no user profile yet"))?;
    if let Some(b) = new_blurb {
        prof.context_blurb = if b.is_empty() { None } else { Some(b) };
    }
    if let Some(s) = new_style {
        prof.communication_style = if s.is_empty() { None } else { Some(s) };
    }

    sqlx::query(
        "UPDATE user_profile
         SET context_blurb = ?1,
             communication_style = ?2,
             updated_at = CURRENT_TIMESTAMP
         WHERE id = 1",
    )
    .bind(&prof.context_blurb)
    .bind(&prof.communication_style)
    .execute(pool)
    .await?;

    Ok(Applied {
        message: "Saved that to your profile. I'll use it from now on.".to_string(),
        json: serde_json::json!({
            "contextBlurbSet": prof.context_blurb.is_some(),
            "communicationStyleSet": prof.communication_style.is_some(),
        })
        .to_string(),
    })
}

// ---------- Send email ----------
//
// Provider-agnostic shell that delegates to email::gmail (or, once wired,
// email::outlook). The user sees a preview card with To/Subject/snippet and
// must click Confirm before anything is actually sent.

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SendEmailParams {
    to: String,
    subject: String,
    body: String,
    /// "gmail" | "outlook". Defaults to "gmail".
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    related_kind: Option<String>,
    #[serde(default)]
    related_id: Option<i64>,
}

async fn apply_send_email(
    pool: &SqlitePool,
    app: &AppHandle,
    params_json: &str,
) -> anyhow::Result<Applied> {
    let p: SendEmailParams = serde_json::from_str(params_json)?;
    let to = p.to.trim();
    let subject = p.subject.trim();
    if to.is_empty() {
        anyhow::bail!("recipient required");
    }
    if subject.is_empty() {
        anyhow::bail!("subject required");
    }
    let provider = p
        .provider
        .as_deref()
        .map(|s| s.trim().to_lowercase())
        .unwrap_or_else(|| "gmail".into());

    let state = app.state::<AppState>();

    let sent = match provider.as_str() {
        "gmail" => crate::email::gmail::send(
            pool,
            &state.http,
            to,
            subject,
            &p.body,
            Some("agent"),
            p.related_kind.as_deref(),
            p.related_id,
        )
        .await
        .map_err(|e| anyhow::anyhow!("gmail send: {e}"))?,
        "outlook" => crate::email::outlook::send(
            pool,
            &state.http,
            to,
            subject,
            &p.body,
            Some("agent"),
            p.related_kind.as_deref(),
            p.related_id,
        )
        .await
        .map_err(|e| anyhow::anyhow!("outlook send: {e}"))?,
        other => anyhow::bail!("unknown email provider: {other}"),
    };

    Ok(Applied {
        message: format!("Sent email to {} via {}.", sent.recipient, provider),
        json: serde_json::json!({
            "emailId": sent.id,
            "recipient": sent.recipient,
            "provider": provider,
        })
        .to_string(),
    })
}

// ---------- Shell command ----------
//
// Defaults: OFF. User must flip `meta.shell_enabled = "true"` via Settings.
// Every invocation is gated by the user's Confirm click on the proposed_action
// card — so even with the toggle ON, no command runs without explicit consent.

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunShellParams {
    /// The literal shell command line. Will be passed to `cmd /c` on Windows
    /// or `sh -c` on Unix. The user sees this exact string before approving.
    command: String,
    #[serde(default)]
    working_dir: Option<String>,
    #[serde(default)]
    timeout_seconds: Option<u64>,
}

/// Static denylist of patterns that never run, even if the user confirmed.
/// This is a belt-and-suspenders layer — the primary gate is user Confirm.
fn shell_command_is_dangerous(cmd: &str) -> Option<&'static str> {
    let lower = cmd.trim().to_lowercase();
    // Patterns matched as substrings of the lowercased command. Conservative.
    const PATTERNS: &[(&str, &str)] = &[
        ("rm -rf",    "destructive recursive delete"),
        ("rm -fr",    "destructive recursive delete"),
        ("rmdir /s",  "destructive recursive delete"),
        ("del /s",    "destructive recursive delete"),
        ("del /q /s", "destructive recursive delete"),
        ("format ",   "formats a disk"),
        ("format c:", "formats system drive"),
        ("dd if=",    "raw disk write"),
        ("mkfs",      "filesystem create"),
        ("shutdown",  "powers off the machine"),
        ("reboot",    "reboots the machine"),
        ("halt",      "halts the machine"),
        ("poweroff",  "powers off"),
        (":(){",      "fork bomb"),
        ("chmod 777", "world-writable permission"),
        ("chmod -r 777", "recursive world-writable"),
        ("chown -r",  "recursive ownership change"),
        ("sudo ",     "privilege escalation"),
        ("doas ",     "privilege escalation"),
        ("runas ",    "privilege escalation"),
        ("> /dev/",   "raw device write"),
        ("curl | sh", "remote-script-pipe-to-shell"),
        ("curl | bash", "remote-script-pipe-to-shell"),
        ("wget | sh", "remote-script-pipe-to-shell"),
        ("wget | bash", "remote-script-pipe-to-shell"),
        ("drop database", "destructive sql"),
        ("drop table",    "destructive sql"),
        ("git push --force", "force push"),
        ("git reset --hard", "destroys local history"),
    ];
    for (p, why) in PATTERNS {
        if lower.contains(p) {
            return Some(why);
        }
    }
    None
}

async fn apply_run_shell_command(
    pool: &SqlitePool,
    params_json: &str,
) -> anyhow::Result<Applied> {
    let p: RunShellParams = serde_json::from_str(params_json)?;
    let command = p.command.trim();
    if command.is_empty() {
        anyhow::bail!("command is required");
    }

    // Hard gate: the toggle must be on.
    let row: Option<(String,)> =
        sqlx::query_as("SELECT value FROM meta WHERE key = ?1")
            .bind("shell_enabled")
            .fetch_optional(pool)
            .await
            .map_err(|e| anyhow::anyhow!("read shell_enabled meta: {e}"))?;
    let enabled = row.as_ref().map(|(v,)| v == "true").unwrap_or(false);
    if !enabled {
        anyhow::bail!("Shell tool is disabled. Enable it in Settings → Advanced → Shell tool.");
    }

    // Denylist check.
    if let Some(reason) = shell_command_is_dangerous(command) {
        anyhow::bail!("Refusing to run — matched safety pattern ({reason}). If you really need this, run it manually.");
    }

    let timeout_secs = p.timeout_seconds.unwrap_or(30).clamp(1, 120);

    let mut cmd = if cfg!(target_os = "windows") {
        let mut c = tokio::process::Command::new("cmd.exe");
        c.arg("/C").arg(command);
        c
    } else {
        let mut c = tokio::process::Command::new("sh");
        c.arg("-c").arg(command);
        c
    };

    if let Some(wd) = p.working_dir.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        cmd.current_dir(wd);
    }
    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);

    let output = match tokio::time::timeout(
        std::time::Duration::from_secs(timeout_secs),
        cmd.output(),
    )
    .await
    {
        Err(_) => anyhow::bail!("Command timed out after {timeout_secs}s"),
        Ok(Err(e)) => anyhow::bail!("Spawn failed: {e}"),
        Ok(Ok(o)) => o,
    };

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let exit_code = output.status.code().unwrap_or(-1);

    const CAP: usize = 4000;
    let truncate = |s: &str| -> String {
        let chars: Vec<char> = s.chars().collect();
        if chars.len() <= CAP {
            chars.into_iter().collect()
        } else {
            let head: String = chars[..CAP].iter().collect();
            format!("{head}\n[…+{} more chars truncated]", chars.len() - CAP)
        }
    };

    let stdout_trim = truncate(&stdout);
    let stderr_trim = truncate(&stderr);

    let preview: String = command.chars().take(60).collect();
    let preview_elided = if command.chars().count() > 60 { "…" } else { "" };

    let mut message = format!(
        "Ran `{preview}{preview_elided}` (exit {exit_code})"
    );
    if !stdout_trim.is_empty() {
        message.push_str(&format!("\n\n--- stdout ---\n{stdout_trim}"));
    }
    if !stderr_trim.is_empty() {
        message.push_str(&format!("\n\n--- stderr ---\n{stderr_trim}"));
    }
    if stdout_trim.is_empty() && stderr_trim.is_empty() {
        message.push_str("\n\n(no output)");
    }

    Ok(Applied {
        message,
        json: serde_json::json!({
            "command": command,
            "exitCode": exit_code,
            "stdoutChars": stdout.chars().count(),
            "stderrChars": stderr.chars().count(),
        })
        .to_string(),
    })
}

// ---------- Action registry ----------
//
// Runtime registry of action handlers. Built-in handlers register in
// `builtin_registry()`; packs add their own handlers at startup (e.g.
// the L2E pack will register `propose_invoice_draft` after step 8 of
// the pack refactor — see PACKS_AUDIT.md).
//
// Why a runtime registry instead of a static match: pack handlers
// can't be enumerated at compile time. The registry replaces the old
// `dispatch(...)` free function with `ActionRegistry::dispatch`.

use std::collections::HashMap;

#[async_trait::async_trait]
pub trait ActionHandler: Send + Sync {
    /// Stable identifier — the value of `proposed_action.kind` this
    /// handler applies to. Must be unique across the registry.
    fn kind(&self) -> &'static str;

    /// Apply the action. Returns the message to surface in the thread
    /// and the JSON to persist as `proposed_action.result_json`.
    async fn apply(
        &self,
        pool: &SqlitePool,
        app: &AppHandle,
        params_json: &str,
    ) -> anyhow::Result<Applied>;
}

pub struct ActionRegistry {
    handlers: HashMap<&'static str, Box<dyn ActionHandler>>,
}

impl ActionRegistry {
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }

    pub fn register(&mut self, h: Box<dyn ActionHandler>) {
        let kind = h.kind();
        if self.handlers.insert(kind, h).is_some() {
            tracing::warn!(
                "ActionRegistry: replacing existing handler for kind {kind}"
            );
        }
    }

    /// Sorted list of registered kinds. Used by callers that need to
    /// enumerate the surface (e.g. the journal extraction prompt
    /// schema, currently still hardcoded — see step 9 in
    /// PACKS_AUDIT.md).
    pub fn kinds(&self) -> Vec<&'static str> {
        let mut v: Vec<_> = self.handlers.keys().copied().collect();
        v.sort_unstable();
        v
    }

    pub async fn dispatch(
        &self,
        pool: &SqlitePool,
        app: &AppHandle,
        action: &ProposedAction,
    ) -> anyhow::Result<Applied> {
        let h = self
            .handlers
            .get(action.kind.as_str())
            .ok_or_else(|| anyhow::anyhow!("unsupported action kind: {}", action.kind))?;
        h.apply(pool, app, &action.params_json).await
    }
}

impl Default for ActionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Built-in action handlers shipped with Travis core. Packs extend
/// this at startup via `PackHandle::register_actions` — for example
/// the L2E pack registers `propose_invoice_draft`.
pub fn builtin_registry() -> ActionRegistry {
    let mut r = ActionRegistry::new();
    r.register(Box::new(DeferTaskHandler));
    r.register(Box::new(SetReminderHandler));
    r.register(Box::new(WriteClipboardHandler));
    r.register(Box::new(RunShellCommandHandler));
    r.register(Box::new(SendEmailHandler));
    r.register(Box::new(UpdateProfileContextHandler));
    r
}

// ---------- Handler wrappers ----------
//
// Each handler is a unit struct that delegates to the existing
// `apply_*` private functions. Once the L2E pack lifts in step 8,
// `ProposeInvoiceDraftHandler` and its `apply_propose_invoice_draft`
// move to the pack and register through `PackHandle::register_actions`.

struct DeferTaskHandler;
#[async_trait::async_trait]
impl ActionHandler for DeferTaskHandler {
    fn kind(&self) -> &'static str { "defer_task" }
    async fn apply(&self, pool: &SqlitePool, _app: &AppHandle, params_json: &str) -> anyhow::Result<Applied> {
        apply_defer_task(pool, params_json).await
    }
}

struct SetReminderHandler;
#[async_trait::async_trait]
impl ActionHandler for SetReminderHandler {
    fn kind(&self) -> &'static str { "set_reminder" }
    async fn apply(&self, pool: &SqlitePool, _app: &AppHandle, params_json: &str) -> anyhow::Result<Applied> {
        apply_set_reminder(pool, params_json).await
    }
}

struct WriteClipboardHandler;
#[async_trait::async_trait]
impl ActionHandler for WriteClipboardHandler {
    fn kind(&self) -> &'static str { "write_clipboard" }
    async fn apply(&self, _pool: &SqlitePool, app: &AppHandle, params_json: &str) -> anyhow::Result<Applied> {
        apply_write_clipboard(app, params_json).await
    }
}

struct RunShellCommandHandler;
#[async_trait::async_trait]
impl ActionHandler for RunShellCommandHandler {
    fn kind(&self) -> &'static str { "run_shell_command" }
    async fn apply(&self, pool: &SqlitePool, _app: &AppHandle, params_json: &str) -> anyhow::Result<Applied> {
        apply_run_shell_command(pool, params_json).await
    }
}

struct SendEmailHandler;
#[async_trait::async_trait]
impl ActionHandler for SendEmailHandler {
    fn kind(&self) -> &'static str { "send_email" }
    async fn apply(&self, pool: &SqlitePool, app: &AppHandle, params_json: &str) -> anyhow::Result<Applied> {
        apply_send_email(pool, app, params_json).await
    }
}

struct UpdateProfileContextHandler;
#[async_trait::async_trait]
impl ActionHandler for UpdateProfileContextHandler {
    fn kind(&self) -> &'static str { "update_profile_context" }
    async fn apply(&self, pool: &SqlitePool, _app: &AppHandle, params_json: &str) -> anyhow::Result<Applied> {
        apply_update_profile_context(pool, params_json).await
    }
}

// ---------- Legacy free-function shim (journal extraction still uses) ----------
//
// Step 9 generalises the journal extraction prompt to derive its action
// list from the live registry. Until then this thin shim returns the
// built-in kinds so journal.rs keeps working. Pack-supplied kinds will
// not flow through this list — but they don't yet exist.

pub fn supported_kinds() -> &'static [&'static str] {
    &[
        "defer_task",
        "propose_invoice_draft",
        "set_reminder",
        "write_clipboard",
        "run_shell_command",
        "send_email",
        "update_profile_context",
    ]
}

// ---------- IPC ----------

#[tauri::command]
pub async fn list_proposed_actions(
    state: State<'_, AppState>,
    filter: Option<ProposedActionFilter>,
) -> Result<Vec<ProposedAction>, String> {
    list(&state.db.pool, filter.unwrap_or_default())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn confirm_action(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
) -> Result<ProposedAction, String> {
    let action = fetch(&state.db.pool, id).await.map_err(|e| e.to_string())?;
    if action.status != "proposed" {
        return Err(format!("action #{id} is {} — not pending", action.status));
    }

    let outcome = state.actions.dispatch(&state.db.pool, &app, &action).await;
    match outcome {
        Ok(applied) => {
            set_status(&state.db.pool, id, "applied", Some(&applied.json))
                .await
                .map_err(|e| e.to_string())?;
            let _ = conversation::append(
                &state.db.pool,
                action.conversation_id,
                "assistant",
                &applied.message,
                Some(
                    &serde_json::json!({
                        "kind": "action_applied",
                        "actionId": id,
                        "actionKind": action.kind,
                        "result": serde_json::from_str::<serde_json::Value>(&applied.json)
                            .unwrap_or(serde_json::Value::Null),
                    })
                    .to_string(),
                ),
            )
            .await;
            let _ = app.emit("domain-changed", "action_applied");
            fetch(&state.db.pool, id).await.map_err(|e| e.to_string())
        }
        Err(e) => {
            let err = e.to_string();
            let _ = set_status(
                &state.db.pool,
                id,
                "failed",
                Some(
                    &serde_json::json!({
                        "error": err,
                    })
                    .to_string(),
                ),
            )
            .await;
            let _ = conversation::append(
                &state.db.pool,
                action.conversation_id,
                "assistant",
                &format!("Couldn't apply that — {err}"),
                None,
            )
            .await;
            Err(err)
        }
    }
}

#[tauri::command]
pub async fn decline_action(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
) -> Result<ProposedAction, String> {
    let action = fetch(&state.db.pool, id).await.map_err(|e| e.to_string())?;
    if action.status != "proposed" {
        return Err(format!("action #{id} is {} — not pending", action.status));
    }
    set_status(&state.db.pool, id, "declined", None)
        .await
        .map_err(|e| e.to_string())?;
    let _ = app.emit("domain-changed", "action_declined");
    fetch(&state.db.pool, id).await.map_err(|e| e.to_string())
}
