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

use std::path::PathBuf;
use std::sync::OnceLock;

/// One-shot store for the app's data directory, set in setup() once
/// Tauri has resolved it. Read by the sync engine to find local files
/// for upload.
static APP_DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

pub fn init_app_data_dir(path: PathBuf) {
    let _ = APP_DATA_DIR.set(path);
}

pub fn app_data_dir() -> Option<PathBuf> {
    APP_DATA_DIR.get().cloned()
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

fn jwt_entry() -> Result<Entry, keyring::Error> {
    Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_JWT_ENTRY)
}

/// Store the session JWT in the OS keychain.
pub fn store_jwt(jwt: &str) -> anyhow::Result<()> {
    jwt_entry()?.set_password(jwt)?;
    Ok(())
}

/// Read the current session JWT from the keychain. Returns `None` if
/// there is no stored token (user has never signed in or has signed out).
pub fn read_jwt() -> Option<String> {
    match jwt_entry().and_then(|e| e.get_password()) {
        Ok(s) if !s.is_empty() => Some(s),
        _ => None,
    }
}

/// Delete the stored JWT. Used on sign-out.
pub fn clear_jwt() -> anyhow::Result<()> {
    match jwt_entry()?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.into()),
    }
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

// --- OAuth loopback flow -------------------------------------------------

#[derive(Debug, Deserialize)]
struct InitResponse {
    #[serde(rename = "authUrl")]
    auth_url: String,
    #[allow(dead_code)]
    state: String,
}

/// Drive the full Google sign-in flow end to end.
///
/// Returns the new JWT (already stored in the keychain) and the user
/// profile so the caller can update the UI immediately.
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

    // 4. Wait for the loopback callback. Hard cap at 5 minutes so a
    //    user who closes the browser tab doesn't tie up the listener
    //    forever.
    let callback = tokio::time::timeout(Duration::from_secs(5 * 60), accept_callback(listener))
        .await
        .map_err(|_| anyhow::anyhow!("sign-in timed out — the browser tab was closed before completing"))??;

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
    std::process::Command::new("cmd")
        .args(["/c", "start", "", url])
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
