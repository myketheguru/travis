//! Travis Cloud client — Phase 1 of the v2 cloud-first architecture.
//!
//! Holds the user's session JWT (in the OS keychain), runs the Google
//! OAuth loopback flow against `api.usetravis.com`, and exposes typed
//! methods over the authenticated backend surface.
//!
//! Loopback flow (no deep-link plugin required):
//!   1. We bind a tokio listener on `127.0.0.1:<free-port>`.
//!   2. We call `api.usetravis.com/auth/oauth/google/init` with that
//!      loopback URL as the `redirect` param.
//!   3. The backend stashes our state token and returns Google's auth
//!      URL plus the state string.
//!   4. We open the user's default browser to the auth URL.
//!   5. User signs in to Google. Google redirects to the backend's
//!      `/auth/oauth/google/callback`. The backend upserts the user
//!      and issues a session JWT, then redirects the browser to our
//!      loopback URL with `?token=<jwt>&expires_in=<seconds>`.
//!   6. Our listener accepts that one request, parses the JWT, replies
//!      with a friendly "you can close this tab" page, and shuts down.
//!   7. We store the JWT in the keychain and the session is live.

use std::time::Duration;

use keyring::Entry;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

pub mod cmd;
pub mod engine;
pub mod files;
pub mod sync;
pub mod t2t;
pub mod t2t_cmd;
pub mod t2t_autodraft;
pub mod circles;
pub mod circles_cmd;

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

/// One-shot store for the app's data directory, set in setup() once
/// Tauri has resolved it. Read by the sync engine to find local files
/// for upload, and by the JWT file storage.
static APP_DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

pub fn init_app_data_dir(path: PathBuf) {
    let _ = APP_DATA_DIR.set(path);
}

pub fn app_data_dir() -> Option<PathBuf> {
    APP_DATA_DIR.get().cloned()
}

/// Build the branded HTTP response we serve from the loopback listener
/// after a successful sign-in or account-connect. Same look as the
/// rest of Travis (dark bg, orb gradient, eyebrow); auto-closes the
/// tab after a short delay so the user doesn't have to do anything.
///
/// `eyebrow` is the small monospace label at the top (e.g. "signed in",
/// "connected"). `title` is the larger headline (e.g. "You're signed in",
/// "Travis is now connected").
fn branded_loopback_response(eyebrow: &str, title: &str) -> String {
    // We embed the whole HTTP response — headers + body — as one
    // string so the existing write_all call writes the full thing in
    // one shot. The HTML is self-contained: no external CSS, no web
    // fonts, no external scripts. window.close() works for tabs
    // opened by JS (handoff/extend flows do open a new tab), with a
    // fallback message for tabs the browser won't auto-close.
    let body = format!(r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Travis</title>
<style>
  :root {{
    color-scheme: dark;
  }}
  html, body {{
    height: 100%;
    margin: 0;
    background: #07080b;
    color: #eceee1;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Inter, Roboto, sans-serif;
  }}
  body {{
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 24px;
  }}
  .wrap {{
    max-width: 420px;
    width: 100%;
    text-align: center;
    animation: fadeIn 0.6s ease both;
  }}
  .orb {{
    width: 56px;
    height: 56px;
    margin: 0 auto 24px;
    border-radius: 999px;
    background: radial-gradient(circle at 30% 30%, #bd9eff, #7c5cff 55%, #6ec4e8);
    box-shadow:
      0 0 24px rgba(124, 92, 255, 0.55),
      0 0 60px rgba(110, 196, 232, 0.25);
    animation: pulse 3s ease-in-out infinite;
  }}
  .eyebrow {{
    font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
    font-size: 11px;
    letter-spacing: 0.22em;
    text-transform: uppercase;
    color: rgba(236, 236, 225, 0.32);
    margin-bottom: 16px;
  }}
  h1 {{
    margin: 0 0 14px;
    font-size: 28px;
    font-weight: 300;
    letter-spacing: -0.02em;
    color: #eceee1;
  }}
  p {{
    margin: 0;
    font-size: 14px;
    line-height: 1.6;
    color: rgba(236, 236, 225, 0.55);
  }}
  .countdown {{
    font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
    color: rgba(236, 236, 225, 0.4);
    font-size: 12px;
    margin-top: 28px;
    letter-spacing: 0.04em;
  }}
  @keyframes fadeIn {{
    from {{ opacity: 0; transform: translateY(10px); }}
    to {{ opacity: 1; transform: translateY(0); }}
  }}
  @keyframes pulse {{
    0%, 100% {{
      box-shadow: 0 0 24px rgba(124, 92, 255, 0.55), 0 0 60px rgba(110, 196, 232, 0.25);
    }}
    50% {{
      box-shadow: 0 0 32px rgba(124, 92, 255, 0.75), 0 0 80px rgba(110, 196, 232, 0.4);
    }}
  }}
</style>
</head>
<body>
<main class="wrap">
  <div class="orb" aria-hidden="true"></div>
  <div class="eyebrow">// {eyebrow}</div>
  <h1>{title}</h1>
  <p>You can close this tab and return to Travis.</p>
  <div class="countdown" id="cd">closing in <span id="t">3</span>…</div>
</main>
<script>
  (function () {{
    var sec = 3;
    var tEl = document.getElementById("t");
    var cdEl = document.getElementById("cd");
    var iv = setInterval(function () {{
      sec--;
      if (sec <= 0) {{
        clearInterval(iv);
        try {{ window.close(); }} catch (e) {{}}
        // window.close() is blocked unless the tab was opened by JS.
        // If we're still here a moment later, switch the message so
        // the user knows to close manually.
        setTimeout(function () {{
          if (cdEl) cdEl.textContent = "you can close this tab";
        }}, 250);
      }} else {{
        if (tEl) tEl.textContent = String(sec);
      }}
    }}, 1000);
  }})();
</script>
</body>
</html>"##, eyebrow = eyebrow, title = title);

    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
        body.len(),
        body,
    )
}

/// Stable identifier for this desktop install. Pulled from the OS
/// hostname; used to tag outbound /sync/push events so we can
/// recognise our own changes when they come back on /sync/pull and
/// avoid applying them again.
pub fn device_id() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Generate a fresh cross-device identifier (32 hex chars, ~128 bits
/// of randomness). Used as `cloud_id` for memory entries and
/// conversations so we can match events across devices on apply.
///
/// We pull from SQLite's randomblob() at insert time rather than from
/// a Rust crate to avoid adding a dep for a single primitive. This
/// function exists for the rare cases (engine apply, future tests)
/// where Rust needs the same primitive without a transaction.
pub fn cloud_id_hex() -> String {
    let bytes: [u8; 16] = std::array::from_fn(|_| {
        // Mix two os-random sources so we don't depend on a single
        // crate's PRNG. std::process::id is stable; SystemTime gives
        // variance per call. XOR with rand fallback once we add it.
        use std::collections::hash_map::DefaultHasher;
        use std::hash::Hasher;
        let mut h = DefaultHasher::new();
        h.write_u128(std::time::SystemTime::now().elapsed().unwrap_or_default().as_nanos());
        h.write_u32(std::process::id());
        (h.finish() & 0xff) as u8
    });
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Production base URL. The desktop currently hardcodes this. A future
/// build flag can swap to a staging URL if we ever need one.
pub const CLOUD_BASE: &str = "https://api.usetravis.com";

const KEYCHAIN_SERVICE: &str = "Travis";
const KEYCHAIN_JWT_ENTRY: &str = "cloud_jwt";

/// v0.22.5 — JWT moved out of OS keychain into a file in app data dir.
///
/// Why: every CloudClient::current() call read the JWT, which on
/// macOS hit Keychain. Since the keychain ACL is bound to the signed
/// app binary, every minor version bump invalidated "Always Allow"
/// and macOS prompted for the keychain password again. With sync
/// running every 60s in the background plus periodic status checks,
/// users were seeing the password prompt constantly. Reported by
/// Taylor 2026-06-23.
///
/// Threat model: the JWT is a 24h bearer token. It already lives in
/// process memory the entire session (used as the Authorization
/// header on every API call). Persisting it to a 600-mode file in
/// the app's data directory doesn't meaningfully change attacker
/// reach vs keychain on a single-user macOS host. The token is
/// short-lived and revocable from /app/settings if anything goes
/// wrong.
///
/// BYOK API keys stay in keychain — those are long-lived, often
/// belong to a paid third-party service, and warrant the prompt.

static JWT_CACHE: OnceLock<Mutex<Option<String>>> = OnceLock::new();

fn jwt_cache() -> &'static Mutex<Option<String>> {
    JWT_CACHE.get_or_init(|| Mutex::new(None))
}

fn jwt_file_path() -> anyhow::Result<PathBuf> {
    let dir = app_data_dir()
        .ok_or_else(|| anyhow::anyhow!("app data dir not initialized"))?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("cloud_jwt"))
}

// `app_data_dir` is defined above (line ~50) and is `pub` for the
// sync engine + other modules. We reuse it here.

fn legacy_jwt_entry() -> Result<Entry, keyring::Error> {
    Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_JWT_ENTRY)
}

/// Store the session JWT in the app data directory.
pub fn store_jwt(jwt: &str) -> anyhow::Result<()> {
    let path = jwt_file_path()?;
    // Write to a temp + rename for atomicity.
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, jwt)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        let _ = std::fs::set_permissions(&tmp, perms);
    }
    std::fs::rename(&tmp, &path)?;
    if let Ok(mut g) = jwt_cache().lock() {
        *g = Some(jwt.to_string());
    }
    Ok(())
}

/// Read the current session JWT. Returns `None` if there is no
/// stored token (user has never signed in or has signed out).
///
/// First checks the in-process cache, then the file, then (one-time)
/// migrates from the old keychain location. Once migrated, the
/// keychain entry is deleted so we never prompt again.
pub fn read_jwt() -> Option<String> {
    if let Ok(g) = jwt_cache().lock() {
        if let Some(jwt) = g.as_ref() {
            return Some(jwt.clone());
        }
    }
    let path = match jwt_file_path() {
        Ok(p) => p,
        Err(_) => return None,
    };
    if let Ok(s) = std::fs::read_to_string(&path) {
        let s = s.trim().to_string();
        if !s.is_empty() {
            if let Ok(mut g) = jwt_cache().lock() {
                *g = Some(s.clone());
            }
            return Some(s);
        }
    }
    // Cold cache + no file = check the legacy keychain location ONCE
    // and migrate. Will prompt the user once on this launch, but only
    // once across the lifetime of this install.
    if let Some(legacy) = read_legacy_jwt_from_keychain() {
        tracing::info!("cloud: migrating JWT from keychain → file");
        if let Err(e) = store_jwt(&legacy) {
            tracing::warn!("cloud: jwt file write failed during migration: {e}");
        } else if let Err(e) = clear_legacy_jwt_from_keychain() {
            tracing::warn!("cloud: legacy keychain JWT cleanup failed: {e}");
        }
        return Some(legacy);
    }
    None
}

fn read_legacy_jwt_from_keychain() -> Option<String> {
    match legacy_jwt_entry().and_then(|e| e.get_password()) {
        Ok(s) if !s.is_empty() => Some(s),
        _ => None,
    }
}

fn clear_legacy_jwt_from_keychain() -> Result<(), keyring::Error> {
    match legacy_jwt_entry()?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e),
    }
}

/// Delete the stored JWT. Used on sign-out.
pub fn clear_jwt() -> anyhow::Result<()> {
    if let Ok(mut g) = jwt_cache().lock() {
        *g = None;
    }
    if let Ok(path) = jwt_file_path() {
        let _ = std::fs::remove_file(&path);
    }
    // Belt-and-suspenders: also clear any stale keychain entry from
    // pre-v0.22.5 installs. Should already have been migrated by
    // read_jwt() but a manual sign-out from a fresh install needs to
    // hit both locations.
    let _ = clear_legacy_jwt_from_keychain();
    Ok(())
}

/// Lightweight client wrapping reqwest + the session JWT.
///
/// Construct with [`CloudClient::current`] which reads the JWT from the
/// keychain; if none is stored, returns `None` and the caller should
/// route the user to the sign-in flow.
pub struct CloudClient {
    http: reqwest::Client,
    jwt: String,
}

impl CloudClient {
    /// Returns a client bound to the currently-stored JWT, or `None` if
    /// the user is signed out.
    pub fn current(http: reqwest::Client) -> Option<Self> {
        read_jwt().map(|jwt| Self { http, jwt })
    }

    fn auth(&self) -> String {
        format!("Bearer {}", self.jwt)
    }

    /// `GET /auth/me` — returns the current user profile if the JWT is
    /// still valid. Used at startup to confirm the stored token works;
    /// on 401 we treat the user as signed out and prompt re-auth.
    pub async fn me(&self) -> anyhow::Result<CloudUser> {
        let resp = self
            .http
            .get(format!("{CLOUD_BASE}/auth/me"))
            .header("authorization", self.auth())
            .send()
            .await?;
        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            anyhow::bail!("unauthorized");
        }
        let body: CloudUser = resp.error_for_status()?.json().await?;
        Ok(body)
    }

    /// `GET /llm/policy` — returns the tier policy + today's usage. The
    /// desktop uses this to choose models and surface usage indicators.
    pub async fn policy(&self) -> anyhow::Result<CloudPolicy> {
        let resp = self
            .http
            .get(format!("{CLOUD_BASE}/llm/policy"))
            .header("authorization", self.auth())
            .send()
            .await?;
        let body: CloudPolicy = resp.error_for_status()?.json().await?;
        Ok(body)
    }

    /// `POST /usage/byok-event` — desktop reports a BYOK call so the
    /// backend has identity-tagged usage data even though we didn't see
    /// the request. Best-effort — failures are logged but never block.
    pub async fn record_byok_event(&self, event: ByokEvent) -> anyhow::Result<()> {
        let resp = self
            .http
            .post(format!("{CLOUD_BASE}/usage/byok-event"))
            .header("authorization", self.auth())
            .json(&event)
            .send()
            .await?;
        resp.error_for_status()?;
        Ok(())
    }

    /// v0.28.24 — POST a sanitized desktop health incident so orbit's
    /// /admin/incidents page reflects real client-side state. Payload
    /// is what the caller built ({kind, message, lane, ...}); server
    /// prefixes the kind with `desktop_` and writes to cloud_incident.
    pub async fn post_client_incident(&self, payload: serde_json::Value) -> anyhow::Result<()> {
        let resp = self
            .http
            .post(format!("{CLOUD_BASE}/me/incidents/client"))
            .header("authorization", self.auth())
            .json(&payload)
            .send()
            .await?;
        resp.error_for_status()?;
        Ok(())
    }

    /// `POST /auth/refresh` — get a fresh JWT before the current one
    /// expires. We refresh proactively when < 1h remaining.
    pub async fn refresh(&self) -> anyhow::Result<RefreshResponse> {
        let resp = self
            .http
            .post(format!("{CLOUD_BASE}/auth/refresh"))
            .header("authorization", self.auth())
            .send()
            .await?;
        let body: RefreshResponse = resp.error_for_status()?.json().await?;
        Ok(body)
    }

    /// v2 Phase 4 — list this user's workflow schedules.
    pub async fn list_schedules(&self) -> anyhow::Result<Vec<WorkflowSchedule>> {
        let resp = self
            .http
            .get(format!("{CLOUD_BASE}/workflows/schedules"))
            .header("authorization", self.auth())
            .send()
            .await?
            .error_for_status()?;
        let body: serde_json::Value = resp.json().await?;
        let arr = body
            .get("schedules")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        Ok(arr
            .into_iter()
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect())
    }

    /// v2 Phase 4 — create a new schedule.
    pub async fn create_schedule(&self, input: CreateScheduleInput) -> anyhow::Result<String> {
        let resp = self
            .http
            .post(format!("{CLOUD_BASE}/workflows/schedules"))
            .header("authorization", self.auth())
            .json(&input)
            .send()
            .await?
            .error_for_status()?;
        let body: serde_json::Value = resp.json().await?;
        Ok(body
            .get("id")
            .and_then(|v| v.as_str())
            .map(String::from)
            .unwrap_or_default())
    }

    /// v2 Phase 4 — delete a schedule.
    pub async fn delete_schedule(&self, id: &str) -> anyhow::Result<()> {
        self.http
            .delete(format!("{CLOUD_BASE}/workflows/schedules/{id}"))
            .header("authorization", self.auth())
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    /// v2 Phase 4 — trigger an immediate run.
    pub async fn run_workflow_now(&self, input: RunNowInput) -> anyhow::Result<String> {
        let resp = self
            .http
            .post(format!("{CLOUD_BASE}/workflows/run-now"))
            .header("authorization", self.auth())
            .json(&input)
            .send()
            .await?
            .error_for_status()?;
        let body: serde_json::Value = resp.json().await?;
        Ok(body
            .get("runId")
            .and_then(|v| v.as_str())
            .map(String::from)
            .unwrap_or_default())
    }

    /// v2 Phase 5 — which packs has the cloud authorized for this user?
    /// Free / Pro users always get an empty list (packs are Org-only).
    /// Org users get whatever the org admin has enabled for them.
    pub async fn authorized_packs(&self) -> anyhow::Result<Vec<String>> {
        let resp = self
            .http
            .get(format!("{CLOUD_BASE}/auth/me/packs"))
            .header("authorization", self.auth())
            .send()
            .await?
            .error_for_status()?;
        let body: serde_json::Value = resp.json().await?;
        Ok(body
            .get("packs")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default())
    }

    /// v2 Phase 4 — paginated run history. Optional `since` ISO
    /// timestamp filters to newer items only (for incremental polling).
    pub async fn list_runs(&self, since: Option<&str>) -> anyhow::Result<Vec<WorkflowRun>> {
        let mut url = format!("{CLOUD_BASE}/workflows/runs");
        if let Some(s) = since {
            url.push_str(&format!("?since={}", urlencoding::encode(s)));
        }
        let resp = self
            .http
            .get(&url)
            .header("authorization", self.auth())
            .send()
            .await?
            .error_for_status()?;
        let body: serde_json::Value = resp.json().await?;
        let arr = body
            .get("runs")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        Ok(arr
            .into_iter()
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect())
    }

    /// `POST /auth/signout` — tells the backend to revoke this token.
    /// The local copy is cleared via `clear_jwt()` separately.
    pub async fn signout(&self) -> anyhow::Result<()> {
        let _ = self
            .http
            .post(format!("{CLOUD_BASE}/auth/signout"))
            .header("authorization", self.auth())
            .send()
            .await?;
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudUser {
    pub id: String,
    pub email: String,
    pub name: Option<String>,
    pub org_id: Option<String>,
    pub tier: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudPolicy {
    pub tier: String,
    pub allowed_models: Vec<String>,
    pub daily_call_cap: u64,
    pub daily_cost_cap_cents: u64,
    pub used_today: PolicyUsage,
    pub remaining_today: PolicyUsage,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyUsage {
    pub calls: u64,
    pub cost_cents: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ByokEvent {
    pub model: String,
    pub provider: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_write_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshResponse {
    pub token: String,
    pub expires_in: u64,
}

// --- v2 Phase 4 — workflow loop types ---------------------------------

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct WorkflowSchedule {
    pub id: String,
    pub name: String,
    pub trigger_kind: String,
    pub trigger_spec: String,
    pub prompt: String,
    pub is_active: i32,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct WorkflowRun {
    pub id: String,
    pub user_id: String,
    #[serde(default)]
    pub schedule_id: Option<String>,
    #[serde(default)]
    pub schedule_name: Option<String>,
    pub source: String,
    pub status: String,
    pub started_at: String,
    #[serde(default)]
    pub finished_at: Option<String>,
    #[serde(default)]
    pub result_text: Option<String>,
    #[serde(default)]
    pub input_tokens: u32,
    #[serde(default)]
    pub output_tokens: u32,
    #[serde(default)]
    pub cost_usd_cents: u32,
    #[serde(default)]
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateScheduleInput {
    pub name: String,
    pub trigger_kind: String,
    pub trigger_spec: serde_json::Value,
    pub prompt: String,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunNowInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schedule_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
}

// --- OAuth loopback flow -------------------------------------------------

#[derive(Debug, Deserialize)]
struct InitResponse {
    #[serde(rename = "authUrl")]
    auth_url: String,
    #[allow(dead_code)]
    state: String,
}

/// Signal a pending sign-in attempt to give up early. Set by the
/// `cloud_sign_in_cancel` command when the user clicks Cancel on the
/// SignIn screen; the in-flight `sign_in_with_google` task polls for
/// it via `tokio::select!` against the loopback listener.
pub static SIGN_IN_CANCEL: tokio::sync::Notify = tokio::sync::Notify::const_new();

/// Tier 2 — one row per (user, provider) connected account.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectedAccount {
    pub provider: String,
    pub scopes_granted: String,
    pub provider_account_id: Option<String>,
    pub is_active: i64,
    pub created_at: String,
    pub last_used_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ConnectedAccountsResp {
    accounts: Vec<ConnectedAccount>,
}

/// Tier 2 — fetch the list of connected_account rows.
pub async fn connected_accounts(http: &reqwest::Client) -> anyhow::Result<Vec<ConnectedAccount>> {
    let jwt = read_jwt().ok_or_else(|| anyhow::anyhow!("not signed in"))?;
    let resp: ConnectedAccountsResp = http
        .get(format!("{CLOUD_BASE}/workflows/connected-accounts"))
        .bearer_auth(&jwt)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(resp.accounts)
}

/// Tier 2 — revoke a connected account by provider.
pub async fn disconnect_account(http: &reqwest::Client, provider: &str) -> anyhow::Result<()> {
    let jwt = read_jwt().ok_or_else(|| anyhow::anyhow!("not signed in"))?;
    http.delete(format!(
        "{CLOUD_BASE}/workflows/connected-accounts/{provider}"
    ))
    .bearer_auth(&jwt)
    .send()
    .await?
    .error_for_status()?;
    Ok(())
}

/// Tier 3 — patch a proposed action's status on a workflow run.
pub async fn update_action_status(
    http: &reqwest::Client,
    run_id: &str,
    action_id: &str,
    status: &str,
    payload: Option<serde_json::Value>,
) -> anyhow::Result<()> {
    let jwt = read_jwt().ok_or_else(|| anyhow::anyhow!("not signed in"))?;
    let body = serde_json::json!({ "status": status, "payload": payload });
    http.patch(format!(
        "{CLOUD_BASE}/workflows/runs/{run_id}/actions/{action_id}"
    ))
    .bearer_auth(&jwt)
    .json(&body)
    .send()
    .await?
    .error_for_status()?;
    Ok(())
}

/// Tier 2 — extend the signed-in user's Google grant to include
/// inbox + calendar read scopes. Opens the browser to
/// /auth/oauth/google/extend, waits for the loopback bounce, returns
/// the comma-separated list of providers Google actually enrolled.
pub async fn extend_google_grant(
    http: &reqwest::Client,
    want_scopes: &[&str],
) -> anyhow::Result<String> {
    let jwt = read_jwt().ok_or_else(|| anyhow::anyhow!("not signed in"))?;

    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let redirect = format!("http://127.0.0.1:{port}/cb");

    let init_url = format!(
        "{CLOUD_BASE}/auth/oauth/google/extend?redirect={}&scopes={}",
        urlencoding::encode(&redirect),
        urlencoding::encode(&want_scopes.join(","))
    );
    let init: InitResponse = http
        .get(&init_url)
        .header("accept", "application/json")
        .bearer_auth(&jwt)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    if let Err(e) = open_in_browser(&init.auth_url) {
        tracing::warn!("could not open browser for extend: {e}");
    }

    // Wait for the loopback bounce. Extend callback returns
    // ?extended=<providers>, no token in the URL.
    let (stream, _addr) = tokio::select! {
        biased;
        _ = SIGN_IN_CANCEL.notified() => anyhow::bail!("canceled"),
        r = tokio::time::timeout(Duration::from_secs(2 * 60), listener.accept()) => {
            r.map_err(|_| anyhow::anyhow!("extend timed out"))??
        }
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut buf = vec![0u8; 8192];
    let mut stream = stream;
    let n = stream.read(&mut buf).await?;
    let req = String::from_utf8_lossy(&buf[..n]).to_string();
    let _ = stream
        .write_all(branded_loopback_response("connected", "Travis is now connected.").as_bytes())
        .await;
    let _ = stream.shutdown().await;

    let first_line = req.lines().next().unwrap_or_default();
    let providers = first_line
        .split_whitespace()
        .nth(1)
        .and_then(|path| {
            let q = path.split_once('?').map(|(_, q)| q).unwrap_or("");
            q.split('&')
                .filter_map(|kv| kv.split_once('='))
                .find(|(k, _)| *k == "extended")
                .map(|(_, v)| v.to_string())
        })
        .unwrap_or_default();
    if providers.is_empty() {
        anyhow::bail!("extend completed without provider list");
    }
    Ok(providers)
}

/// v3 Slice 4 — desktop handoff claim via web-based session.
///
/// Flow:
///   1. Bind a loopback listener
///   2. Open browser to https://usetravis.com/app/handoff?device=<label>&redirect=http://127.0.0.1:<port>/cb
///   3. Web: if signed_in, shows Approve UI → POSTs /auth/oauth/handoff/start →
///      cloud writes {code → user_id} to KV with 5-min TTL → redirects to
///      `${redirect}?code=<code>`
///      If signed_out: web kicks the user through Google OAuth first,
///      then resumes here.
///   4. Loopback catches the request, extracts ?code=
///   5. POST /auth/oauth/handoff/claim with code → cloud returns fresh JWT +
///      user profile, code is single-use and deleted from KV
///   6. Store JWT in keychain, return CloudUser
///
/// This is the v3 primary sign-in path. The Google-direct flow stays as
/// a fallback for users who don't want a browser hop or are signing in
/// for the first time and would rather do it from the desktop.
pub async fn claim_handoff_from_web(http: &reqwest::Client) -> anyhow::Result<CloudUser> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    // 1. Bind loopback.
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let redirect = format!("http://127.0.0.1:{port}/cb");

    // 2. Device label — hostname is informative without revealing too much.
    let device_label = hostname_fallback();

    // 3. Open browser.
    let url = format!(
        "https://usetravis.com/app/handoff?device={}&redirect={}",
        urlencoding::encode(&device_label),
        urlencoding::encode(&redirect),
    );
    if let Err(e) = open_in_browser(&url) {
        tracing::warn!("could not open browser for handoff: {e}");
    }

    // 4. Wait for the loopback callback that carries the code. We
    //    accept connections IN A LOOP and keep listening until we get
    //    a real GET with `?code=` — Windows browsers (especially
    //    Chrome/Edge) tend to open a TCP preconnect probe to the
    //    loopback URL before issuing the actual GET. The probe is
    //    just a TCP handshake with no body; if we treat the first
    //    accept as the real request we end up with an empty buffer
    //    and bail out, leaving the real GET hanging.
    //
    //    Hard cap of 5 minutes across the whole loop; cancel and
    //    individual reads are bounded so a never-completing probe
    //    can't wedge us.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5 * 60);
    let code = loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            anyhow::bail!("handoff timed out — close the browser tab and try again");
        }
        let (stream, _addr) = tokio::select! {
            biased;
            _ = SIGN_IN_CANCEL.notified() => anyhow::bail!("handoff canceled"),
            r = tokio::time::timeout(remaining, listener.accept()) => {
                r.map_err(|_| anyhow::anyhow!("handoff timed out — close the browser tab and try again"))??
            }
        };
        let mut stream = stream;
        let mut buf = vec![0u8; 8192];

        // Read up to the end-of-headers (\r\n\r\n) so we always see the
        // full request line. Bounded by a 5s per-connection deadline so
        // a slow / hung probe can't stall us. read() returns 0 on FIN —
        // that's the "preconnect probe with no body" case.
        let read_deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        let mut total = 0usize;
        let mut got_request = false;
        while total < buf.len() {
            let read_remaining = read_deadline.saturating_duration_since(tokio::time::Instant::now());
            if read_remaining.is_zero() {
                break;
            }
            let n = match tokio::time::timeout(read_remaining, stream.read(&mut buf[total..])).await {
                Ok(Ok(n)) => n,
                Ok(Err(_)) | Err(_) => 0,
            };
            if n == 0 {
                break;
            }
            total += n;
            got_request = true;
            // Found end-of-headers — we have everything we need.
            if buf[..total].windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
        }

        if !got_request {
            // Empty connection (preconnect probe). Move on and accept
            // the next one — that's the real GET.
            tracing::debug!("handoff: empty loopback connection, waiting for the real GET");
            let _ = stream.shutdown().await;
            continue;
        }

        let req = String::from_utf8_lossy(&buf[..total]).to_string();
        let first_line = req.lines().next().unwrap_or_default();
        let extracted = first_line
            .split_whitespace()
            .nth(1)
            .and_then(|path| {
                let q = path.split_once('?').map(|(_, q)| q).unwrap_or("");
                q.split('&')
                    .filter_map(|kv| kv.split_once('='))
                    .find(|(k, _)| *k == "code")
                    .map(|(_, v)| urlencoding::decode(v).unwrap_or_default().into_owned())
            })
            .unwrap_or_default();

        if !extracted.is_empty() {
            // Send the success page on the SAME connection that carried
            // the code so the browser actually paints something.
            let _ = stream
                .write_all(branded_loopback_response("signed in", "You're signed in.").as_bytes())
                .await;
            let _ = stream.shutdown().await;
            break extracted;
        }

        // Some other GET (favicon, etc.) — close and keep listening.
        let _ = stream
            .write_all(b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n")
            .await;
        let _ = stream.shutdown().await;
        tracing::debug!(
            "handoff: loopback hit without a code, continuing to wait. Path: {}",
            first_line
        );
    };

    // 6. Exchange code → JWT.
    let claim_resp: ClaimResponse = http
        .post(format!("{CLOUD_BASE}/auth/oauth/handoff/claim"))
        .json(&serde_json::json!({ "code": code }))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    // 7. Store + return.
    store_jwt(&claim_resp.token)?;
    Ok(claim_resp.user)
}

#[derive(Debug, Deserialize)]
struct ClaimResponse {
    token: String,
    #[allow(dead_code)]
    expires_in: u64,
    user: CloudUser,
}

fn hostname_fallback() -> String {
    // Best-effort host label. We never use this as an identifier;
    // it's a human-facing string shown on the web approval screen so
    // the user can tell which device is asking.
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "your computer".to_string())
}

/// Drive the full Google sign-in flow end to end.
///
/// Returns the new JWT (already stored in the keychain) and the user
/// profile so the caller can update the UI immediately.
///
/// v3 Slice 4 (final) — no longer exposed as a Tauri command. The
/// web-handoff flow (claim_handoff_from_web) replaced it as the
/// only IPC-reachable sign-in path. Kept here as a module-private
/// helper for dev tooling and the rare smoke-test scenario.
#[allow(dead_code)]
pub async fn sign_in_with_google(http: &reqwest::Client) -> anyhow::Result<CloudUser> {
    // 1. Bind the loopback listener first so we know which port to ask
    //    the backend to redirect to.
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let redirect = format!("http://127.0.0.1:{port}/cb");

    // 2. Ask the backend for the Google auth URL.
    let init_url = format!(
        "{CLOUD_BASE}/auth/oauth/google/init?redirect={}",
        urlencoding::encode(&redirect)
    );
    let init: InitResponse = http
        .get(&init_url)
        .header("accept", "application/json")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    // 3. Open the browser. Best-effort — if the user has no default
    //    browser configured we still log the URL so they can paste it.
    if let Err(e) = open_in_browser(&init.auth_url) {
        tracing::warn!("could not open browser: {e}. Auth URL: {}", init.auth_url);
    }

    // 4. Wait for the loopback callback OR a cancel signal OR a 2-minute
    //    timeout. The cancel path lets the UI abort cleanly when the user
    //    closes the browser tab or hits an error on Google's side; the
    //    timeout is the hard backstop. Both paths release the port so
    //    a quick retry doesn't fail.
    let callback = tokio::select! {
        biased;
        _ = SIGN_IN_CANCEL.notified() => {
            anyhow::bail!("sign-in canceled");
        }
        r = tokio::time::timeout(Duration::from_secs(2 * 60), accept_callback(listener)) => {
            r.map_err(|_| anyhow::anyhow!("sign-in timed out — the browser tab was closed before completing"))??
        }
    };

    // 5. Store the JWT, fetch the user profile, return it.
    store_jwt(&callback.token)?;
    let client = CloudClient { http: http.clone(), jwt: callback.token.clone() };
    let user = client.me().await?;
    Ok(user)
}

struct CallbackResult {
    token: String,
}

async fn accept_callback(listener: TcpListener) -> anyhow::Result<CallbackResult> {
    let (mut stream, _) = listener.accept().await?;

    // Read just enough of the request to get the path + query.
    let mut buf = vec![0u8; 4096];
    let n = stream.read(&mut buf).await?;
    let request = String::from_utf8_lossy(&buf[..n]).to_string();

    let first_line = request.lines().next().unwrap_or("").to_string();
    let path = first_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .to_string();

    let query = path.split_once('?').map(|(_, q)| q).unwrap_or("");
    let mut token: Option<String> = None;
    for pair in query.split('&') {
        let Some((k, v)) = pair.split_once('=') else {
            continue;
        };
        if k == "token" {
            token = Some(
                urlencoding::decode(v)
                    .map(|s| s.into_owned())
                    .unwrap_or_else(|_| v.to_string()),
            );
        }
    }

    let body = if token.is_some() {
        "<!doctype html><html><body style=\"font-family:-apple-system,sans-serif;max-width:480px;margin:64px auto;padding:0 24px;text-align:center;color:#1a1a1a;\"><h2>Signed in to Travis</h2><p>You can close this tab and return to the app.</p></body></html>"
    } else {
        "<!doctype html><html><body style=\"font-family:-apple-system,sans-serif;max-width:480px;margin:64px auto;padding:0 24px;text-align:center;color:#1a1a1a;\"><h2>Something went wrong</h2><p>The sign-in didn't complete. Please return to Travis and try again.</p></body></html>"
    };

    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(response.as_bytes()).await.ok();
    stream.shutdown().await.ok();

    let token = token.ok_or_else(|| anyhow::anyhow!("no token in callback URL"))?;
    Ok(CallbackResult { token })
}

#[cfg(target_os = "windows")]
fn open_in_browser(url: &str) -> std::io::Result<()> {
    // Don't use `cmd /c start "" <url>` — cmd.exe parses `&` in the URL
    // as a command separator BEFORE passing to start, so OAuth URLs get
    // truncated at the first `&`. Symptoms: Google replies with
    // "Required parameter is missing: response_type" because everything
    // past `?client_id=…&` was chopped off.
    //
    // rundll32 hands the URL straight to the ShellExecute API as a
    // single literal argument — no shell parsing, no truncation. This
    // is the documented Win32 way to launch the system browser from a
    // command line.
    std::process::Command::new("rundll32")
        .args(["url.dll,FileProtocolHandler", url])
        .spawn()
        .map(|_| ())
}

#[cfg(target_os = "macos")]
fn open_in_browser(url: &str) -> std::io::Result<()> {
    std::process::Command::new("open").arg(url).spawn().map(|_| ())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn open_in_browser(url: &str) -> std::io::Result<()> {
    std::process::Command::new("xdg-open").arg(url).spawn().map(|_| ())
}
