//! Tracks ambient operational health: are we online, did the LLM throw a
//! typed error recently, etc. Reactive only — we never poll. State is
//! updated by callers when they observe an issue, and broadcast to the UI
//! via a `health-changed` event. Background LLM-using work checks
//! `is_blocked()` and skips when the state is degraded.
//!
//! State lives in memory only; on app restart we assume online + healthy
//! until something says otherwise.

use std::sync::RwLock;

use serde::Serialize;
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum IssueKind {
    Offline,
    QuotaExhausted,
    RateLimited,
    Unauthorized,
    ServerError,
    NetworkError,
    Provider,
}

impl IssueKind {
    /// Whether background LLM-using work should pause when this issue is
    /// the current state. We're conservative: any LLM issue blocks until
    /// either a successful call clears it or the user dismisses the banner.
    pub fn blocks_background(self) -> bool {
        true
    }

    /// User-facing label for the banner.
    pub fn headline(self) -> &'static str {
        match self {
            IssueKind::Offline => "You're offline",
            IssueKind::QuotaExhausted => "LLM credits look exhausted",
            IssueKind::RateLimited => "LLM rate limit hit",
            IssueKind::Unauthorized => "LLM rejected the API key",
            IssueKind::ServerError => "LLM service is having trouble",
            IssueKind::NetworkError => "Couldn't reach the LLM",
            IssueKind::Provider => "LLM error",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub kind: IssueKind,
    pub message: String,
    pub since: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthState {
    pub online: bool,
    pub issue: Option<Issue>,
}

impl HealthState {
    fn new() -> Self {
        // Assume online at startup; the frontend pushes the real value
        // immediately via health_set_online.
        Self {
            online: true,
            issue: None,
        }
    }

    pub fn is_blocked(&self) -> bool {
        if !self.online {
            return true;
        }
        match &self.issue {
            Some(i) => i.kind.blocks_background(),
            None => false,
        }
    }
}

pub struct Health {
    state: RwLock<HealthState>,
}

impl Health {
    pub fn new() -> Self {
        Self {
            state: RwLock::new(HealthState::new()),
        }
    }

    pub fn current(&self) -> HealthState {
        self.state.read().unwrap().clone()
    }

    pub fn is_blocked(&self) -> bool {
        self.state.read().unwrap().is_blocked()
    }

    pub fn report(&self, app: &AppHandle, kind: IssueKind, message: impl Into<String>) {
        let now = chrono::Utc::now().to_rfc3339();
        let new_issue = Issue {
            kind,
            message: message.into(),
            since: now,
        };
        let snapshot = {
            let mut s = self.state.write().unwrap();
            let changed = match &s.issue {
                Some(prev) => prev.kind != new_issue.kind || prev.message != new_issue.message,
                None => true,
            };
            s.issue = Some(new_issue);
            if changed {
                Some(s.clone())
            } else {
                None
            }
        };
        if let Some(snap) = snapshot {
            let _ = app.emit("health-changed", snap);
        }
    }

    pub fn clear(&self, app: &AppHandle) {
        let snapshot = {
            let mut s = self.state.write().unwrap();
            if s.issue.is_some() {
                s.issue = None;
                Some(s.clone())
            } else {
                None
            }
        };
        if let Some(snap) = snapshot {
            let _ = app.emit("health-changed", snap);
        }
    }

    pub fn set_online(&self, app: &AppHandle, online: bool) {
        let snapshot = {
            let mut s = self.state.write().unwrap();
            if s.online != online {
                s.online = online;
                Some(s.clone())
            } else {
                None
            }
        };
        if let Some(snap) = snapshot {
            let _ = app.emit("health-changed", snap);
        }
    }
}

/// Pattern-match an LLM error string to identify what went wrong. Falls
/// back to `IssueKind::Provider` when nothing recognizable matches.
pub fn classify_llm_error(s: &str) -> IssueKind {
    let lower = s.to_lowercase();

    // Quota / billing first — these often arrive as 429 with a specific body,
    // and we want them distinct from plain rate-limit so the banner can
    // tell the user to top up rather than wait.
    if lower.contains("insufficient_quota")
        || lower.contains("credit balance")
        || lower.contains("quota exceeded")
        || lower.contains("billing_hard_limit")
        || lower.contains("you exceeded your current quota")
    {
        return IssueKind::QuotaExhausted;
    }

    // Auth.
    if lower.contains(" 401")
        || lower.contains("invalid_api_key")
        || lower.contains("invalid x-api-key")
        || lower.contains("invalid api key")
        || lower.contains("authentication")
        || lower.contains("unauthorized")
    {
        return IssueKind::Unauthorized;
    }

    // Plain rate limit.
    if lower.contains(" 429")
        || lower.contains("rate limit")
        || lower.contains("ratelimit")
        || lower.contains("too many requests")
    {
        return IssueKind::RateLimited;
    }

    // Server-side.
    if lower.contains(" 500")
        || lower.contains(" 502")
        || lower.contains(" 503")
        || lower.contains(" 504")
        || lower.contains(" 529")
        || lower.contains("overloaded")
    {
        return IssueKind::ServerError;
    }

    // Network / transport (reqwest errors).
    if lower.contains("error sending request")
        || lower.contains("connection")
        || lower.contains("dns")
        || lower.contains("timeout")
        || lower.contains("os error")
        || lower.contains("tcp")
    {
        return IssueKind::NetworkError;
    }

    IssueKind::Provider
}

// ---------- IPC ----------

#[tauri::command]
pub async fn health_status(
    state: tauri::State<'_, crate::AppState>,
) -> Result<HealthState, String> {
    Ok(state.health.current())
}

#[tauri::command]
pub fn health_set_online(
    app: AppHandle,
    state: tauri::State<'_, crate::AppState>,
    online: bool,
) -> Result<(), String> {
    state.health.set_online(&app, online);
    Ok(())
}

#[tauri::command]
pub fn health_dismiss(
    app: AppHandle,
    state: tauri::State<'_, crate::AppState>,
) -> Result<(), String> {
    state.health.clear(&app);
    Ok(())
}
