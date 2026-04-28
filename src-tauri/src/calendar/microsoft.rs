//! Microsoft (Outlook / Microsoft Graph) OAuth2 flow. Mirrors the Google
//! flow shape: localhost listener for the redirect, PKCE + state, persists
//! refresh token in the keychain and access_token + expiry in the DB. The
//! same `oauth_account` table holds the row, keyed by `provider = "microsoft"`.
//!
//! Scopes cover both calendar read and Mail.Send; `offline_access` is what
//! gets us a refresh token in the Microsoft world.

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

pub const PROVIDER: &str = "microsoft";
pub const REFRESH_SECRET_KEY: &str = "microsoft_refresh";

const AUTH_URL: &str = "https://login.microsoftonline.com/common/oauth2/v2.0/authorize";
const TOKEN_URL: &str = "https://login.microsoftonline.com/common/oauth2/v2.0/token";
const USERINFO_URL: &str = "https://graph.microsoft.com/v1.0/me";

const DEFAULT_SCOPES: &[&str] = &[
    "Calendars.Read",
    "Mail.Send",
    "User.Read",
    "offline_access",
];

const CLIENT_ID: Option<&str> = option_env!("TRAVIS_MICROSOFT_CLIENT_ID");
const CLIENT_SECRET: Option<&str> = option_env!("TRAVIS_MICROSOFT_CLIENT_SECRET");

const SUCCESS_HTML: &str = r#"<!doctype html><html><body style="font-family:system-ui;background:#0a0a18;color:#ececf1;padding:40px"><h2>You can close this tab</h2><p>Travis is connected to your Microsoft account.</p></body></html>"#;

pub fn is_configured() -> bool {
    CLIENT_ID.is_some() && CLIENT_SECRET.is_some()
}

type ConfiguredClient =
    BasicClient<EndpointSet, EndpointNotSet, EndpointNotSet, EndpointNotSet, EndpointSet>;

fn build_client(redirect_url: RedirectUrl) -> anyhow::Result<ConfiguredClient> {
    let cid = CLIENT_ID.ok_or_else(|| {
        anyhow::anyhow!(
            "Microsoft client id missing — set TRAVIS_MICROSOFT_CLIENT_ID at build time."
        )
    })?;
    let secret = CLIENT_SECRET.ok_or_else(|| {
        anyhow::anyhow!(
            "Microsoft client secret missing — set TRAVIS_MICROSOFT_CLIENT_SECRET at build time."
        )
    })?;
    Ok(BasicClient::new(ClientId::new(cid.into()))
        .set_client_secret(ClientSecret::new(secret.into()))
        .set_auth_uri(AuthUrl::new(AUTH_URL.into())?)
        .set_token_uri(TokenUrl::new(TOKEN_URL.into())?)
        .set_redirect_uri(redirect_url))
}

pub async fn connect(
    app: AppHandle,
    pool: &SqlitePool,
    http: reqwest::Client,
) -> anyhow::Result<String> {
    // Microsoft requires the redirect URI to match an entry registered in
    // Azure Portal. The recommended setup is to register `http://localhost`
    // for a "Mobile and desktop applications" platform — Azure then matches
    // any port and path on localhost. We bind to localhost (resolves to
    // 127.0.0.1) and use a no-path URI so registration stays simple.
    let listener = TcpListener::bind("localhost:0")
        .await
        .map_err(|e| anyhow::anyhow!("bind localhost: {e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| anyhow::anyhow!("local_addr: {e}"))?
        .port();
    let redirect_uri = format!("http://localhost:{port}");

    let client = build_client(RedirectUrl::new(redirect_uri.clone())?)?;

    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

    let mut auth_builder = client
        .authorize_url(CsrfToken::new_random)
        .set_pkce_challenge(pkce_challenge);
    for scope in DEFAULT_SCOPES {
        auth_builder = auth_builder.add_scope(Scope::new((*scope).to_string()));
    }
    let (auth_url, csrf_token) = auth_builder.url();

    app.opener()
        .open_url(auth_url.as_str(), None::<&str>)
        .map_err(|e| anyhow::anyhow!("open auth url: {e}"))?;

    let (code, returned_state) = timeout(
        Duration::from_secs(300),
        wait_for_callback(listener, SUCCESS_HTML),
    )
    .await
    .map_err(|_| {
        anyhow::anyhow!("timed out waiting for Microsoft to redirect — sign in cancelled")
    })??;

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
    let expires_at = token.expires_in().map(iso_utc_in);
    let refresh_token = token
        .refresh_token()
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Microsoft didn't return a refresh token — make sure the offline_access scope is included."
            )
        })?
        .secret()
        .to_string();
    let scopes: Vec<String> = token
        .scopes()
        .map(|sc| sc.iter().map(|s| s.to_string()).collect())
        .unwrap_or_else(|| DEFAULT_SCOPES.iter().map(|s| s.to_string()).collect());

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

pub async fn disconnect(pool: &SqlitePool) -> anyhow::Result<()> {
    let _ = secrets::delete_api_key(REFRESH_SECRET_KEY);
    super::delete_account(pool, PROVIDER)
        .await
        .map_err(|e| anyhow::anyhow!("delete account: {e}"))?;
    Ok(())
}

pub async fn access_token(pool: &SqlitePool, http: &reqwest::Client) -> anyhow::Result<String> {
    let account = fetch_account(pool, PROVIDER)
        .await
        .map_err(|e| anyhow::anyhow!("read account: {e}"))?
        .ok_or_else(|| anyhow::anyhow!("Microsoft account isn't connected."))?;

    if let (Some(token), Some(exp)) = (&account.access_token, &account.expires_at) {
        if !is_expired(exp) {
            return Ok(token.clone());
        }
    }

    let refresh = secrets::get_api_key(REFRESH_SECRET_KEY)
        .ok_or_else(|| anyhow::anyhow!("refresh token missing — reconnect Microsoft account"))?;
    let client = build_client(RedirectUrl::new("http://127.0.0.1:0/unused".into())?)?;
    let token = client
        .exchange_refresh_token(&RefreshToken::new(refresh))
        .request_async(http)
        .await
        .map_err(|e| anyhow::anyhow!("refresh token: {e}"))?;
    let access = token.access_token().secret().to_string();
    let new_expires = token.expires_in().map(iso_utc_in);
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
    // Graph returns mail or userPrincipalName depending on account type.
    let email = v
        .get("mail")
        .and_then(|s| s.as_str())
        .or_else(|| v.get("userPrincipalName").and_then(|s| s.as_str()))
        .map(|s| s.to_string());
    Ok(email)
}
