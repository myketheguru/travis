use std::time::Duration;

use oauth2::basic::BasicClient;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, EndpointNotSet, EndpointSet,
    PkceCodeChallenge, RedirectUrl, RefreshToken, Scope, TokenResponse, TokenUrl,
};
use sqlx::SqlitePool;
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;
use tokio::net::TcpListener;
use tokio::time::timeout;

use crate::secrets;

use super::oauth_util::{is_expired, iso_utc_in, wait_for_callback};
use super::{fetch_account, upsert_account};

const SUCCESS_HTML: &str = r#"<!doctype html><html><body style="font-family:system-ui;background:#0a0a18;color:#ececf1;padding:40px"><h2>You can close this tab</h2><p>Travis is connected to your Google account.</p></body></html>"#;

pub const PROVIDER: &str = "google_calendar";
pub const REFRESH_SECRET_KEY: &str = "google_calendar_refresh";

const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const USERINFO_URL: &str = "https://openidconnect.googleapis.com/v1/userinfo";
// Scopes cover the unified Google connection (calendar read + gmail send).
// PROVIDER stays "google_calendar" for backward compat with the OAuth account
// row even though scopes now include gmail.send.
const DEFAULT_SCOPES: &[&str] = &[
    "https://www.googleapis.com/auth/calendar.events.readonly",
    "https://www.googleapis.com/auth/gmail.send",
    "openid",
    "email",
];

const CLIENT_ID: Option<&str> = option_env!("TRAVIS_GOOGLE_CLIENT_ID");
const CLIENT_SECRET: Option<&str> = option_env!("TRAVIS_GOOGLE_CLIENT_SECRET");

pub fn is_configured() -> bool {
    CLIENT_ID.is_some() && CLIENT_SECRET.is_some()
}

/// Concrete BasicClient type after we've configured all the endpoints.
type ConfiguredClient = BasicClient<EndpointSet, EndpointNotSet, EndpointNotSet, EndpointNotSet, EndpointSet>;

fn build_client(redirect_url: RedirectUrl) -> anyhow::Result<ConfiguredClient> {
    let cid = CLIENT_ID.ok_or_else(|| {
        anyhow::anyhow!("Google client id missing — set TRAVIS_GOOGLE_CLIENT_ID at build time.")
    })?;
    let secret = CLIENT_SECRET.ok_or_else(|| {
        anyhow::anyhow!("Google client secret missing — set TRAVIS_GOOGLE_CLIENT_SECRET at build time.")
    })?;
    Ok(BasicClient::new(ClientId::new(cid.into()))
        .set_client_secret(ClientSecret::new(secret.into()))
        .set_auth_uri(AuthUrl::new(AUTH_URL.into())?)
        .set_token_uri(TokenUrl::new(TOKEN_URL.into())?)
        .set_redirect_uri(redirect_url))
}

/// Run the full OAuth dance: spin up a localhost listener on a random port,
/// open the browser, wait for the redirect, exchange the code for tokens,
/// fetch user email, persist tokens. Returns the email of the connected
/// account.
pub async fn connect(
    app: AppHandle,
    pool: &SqlitePool,
    http: reqwest::Client,
) -> anyhow::Result<String> {
    // Bind a random local port and use it as our redirect URI.
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| anyhow::anyhow!("bind localhost: {e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| anyhow::anyhow!("local_addr: {e}"))?
        .port();
    let redirect_uri = format!("http://127.0.0.1:{port}/callback");

    let client = build_client(RedirectUrl::new(redirect_uri.clone())?)?;

    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

    let mut auth_builder = client
        .authorize_url(CsrfToken::new_random)
        .set_pkce_challenge(pkce_challenge)
        .add_extra_param("access_type", "offline")
        .add_extra_param("prompt", "consent");
    for scope in DEFAULT_SCOPES {
        auth_builder = auth_builder.add_scope(Scope::new((*scope).to_string()));
    }
    let (auth_url, csrf_token) = auth_builder.url();

    // Open browser to auth URL.
    app.opener()
        .open_url(auth_url.as_str(), None::<&str>)
        .map_err(|e| anyhow::anyhow!("open auth url: {e}"))?;

    // Wait up to 5 minutes for the user to complete the flow.
    let (code, returned_state) = timeout(
        Duration::from_secs(300),
        wait_for_callback(listener, SUCCESS_HTML),
    )
    .await
    .map_err(|_| anyhow::anyhow!("timed out waiting for Google to redirect — sign in cancelled"))??;

    if returned_state != *csrf_token.secret() {
        anyhow::bail!("OAuth state mismatch — possible CSRF, refusing to continue");
    }

    let token = client
        .exchange_code(AuthorizationCode::new(code))
        .set_pkce_verifier(pkce_verifier)
        .request_async(&http)
        .await
        .map_err(|e| anyhow::anyhow!("token exchange: {e}"))?;

    let access_token = token.access_token().secret().to_string();
    let expires_at = token.expires_in().map(|d| iso_utc_in(d));
    let refresh_token = token
        .refresh_token()
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Google didn't return a refresh token — try disconnecting any prior session at https://myaccount.google.com/permissions and connect again."
            )
        })?
        .secret()
        .to_string();
    let scopes: Vec<String> = token
        .scopes()
        .map(|sc| sc.iter().map(|s| s.to_string()).collect())
        .unwrap_or_else(|| DEFAULT_SCOPES.iter().map(|s| s.to_string()).collect());

    // Fetch the user's email so we can show "connected as ..." in the UI.
    let email = fetch_email(&http, &access_token).await.unwrap_or(None);

    secrets::store_api_key(REFRESH_SECRET_KEY, &refresh_token)
        .map_err(|e| anyhow::anyhow!("store refresh token: {e}"))?;
    upsert_account(
        pool,
        PROVIDER,
        email.as_deref(),
        &scopes,
        Some(&access_token),
        expires_at.as_deref(),
    )
    .await
    .map_err(|e| anyhow::anyhow!("persist account: {e}"))?;

    Ok(email.unwrap_or_else(|| "connected".into()))
}

/// Disconnect: delete local tokens. We don't bother revoking on Google's side
/// since the user can do that from myaccount.google.com if they want.
pub async fn disconnect(pool: &SqlitePool) -> anyhow::Result<()> {
    let _ = secrets::delete_api_key(REFRESH_SECRET_KEY);
    super::delete_account(pool, PROVIDER)
        .await
        .map_err(|e| anyhow::anyhow!("delete account: {e}"))?;
    Ok(())
}

/// Get a fresh access token, refreshing if needed. Returns the token string.
pub async fn access_token(
    pool: &SqlitePool,
    http: &reqwest::Client,
) -> anyhow::Result<String> {
    let account = fetch_account(pool, PROVIDER)
        .await
        .map_err(|e| anyhow::anyhow!("read account: {e}"))?
        .ok_or_else(|| anyhow::anyhow!("Google Calendar isn't connected."))?;

    if let (Some(token), Some(exp)) = (&account.access_token, &account.expires_at) {
        if !is_expired(exp) {
            return Ok(token.clone());
        }
    }

    let refresh = secrets::get_api_key(REFRESH_SECRET_KEY)
        .ok_or_else(|| anyhow::anyhow!("refresh token missing — reconnect Google Calendar"))?;
    let client = build_client(RedirectUrl::new("http://127.0.0.1:0/unused".into())?)?;
    let token = client
        .exchange_refresh_token(&RefreshToken::new(refresh))
        .request_async(http)
        .await
        .map_err(|e| anyhow::anyhow!("refresh token: {e}"))?;
    let access = token.access_token().secret().to_string();
    let new_expires = token.expires_in().map(|d| iso_utc_in(d));
    let scopes: Vec<String> = account
        .scopes
        .split_whitespace()
        .map(|s| s.to_string())
        .collect();
    upsert_account(
        pool,
        PROVIDER,
        account.account_id.as_deref(),
        &scopes,
        Some(&access),
        new_expires.as_deref(),
    )
    .await
    .map_err(|e| anyhow::anyhow!("persist refreshed token: {e}"))?;
    Ok(access)
}

// ---------- helpers ----------

async fn fetch_email(http: &reqwest::Client, access_token: &str) -> anyhow::Result<Option<String>> {
    let resp = http
        .get(USERINFO_URL)
        .bearer_auth(access_token)
        .send()
        .await?;
    if !resp.status().is_success() {
        return Ok(None);
    }
    let v: serde_json::Value = resp.json().await?;
    Ok(v.get("email")
        .and_then(|s| s.as_str())
        .map(|s| s.to_string()))
}

