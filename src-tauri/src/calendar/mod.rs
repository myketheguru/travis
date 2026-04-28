pub mod google;
pub mod microsoft;
pub(crate) mod oauth_util;

use serde::Serialize;
use sqlx::SqlitePool;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct OAuthAccount {
    pub provider: String,
    pub account_id: Option<String>,
    pub scopes: String,
    pub access_token: Option<String>,
    pub expires_at: Option<String>,
    pub connected_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionStatus {
    pub provider: String,
    pub connected: bool,
    pub account_id: Option<String>,
    pub scopes: Vec<String>,
    pub connected_at: Option<String>,
    pub expires_at: Option<String>,
    /// True if compile-time client credentials are present so the user can
    /// actually connect. False on dev builds without the env vars set.
    pub configured: bool,
}

pub async fn fetch_account(
    pool: &SqlitePool,
    provider: &str,
) -> Result<Option<OAuthAccount>, sqlx::Error> {
    sqlx::query_as::<_, OAuthAccount>(
        "SELECT provider, account_id, scopes, access_token, expires_at, connected_at, updated_at
         FROM oauth_account WHERE provider = ?1",
    )
    .bind(provider)
    .fetch_optional(pool)
    .await
}

pub async fn upsert_account(
    pool: &SqlitePool,
    provider: &str,
    account_id: Option<&str>,
    scopes: &[String],
    access_token: Option<&str>,
    expires_at: Option<&str>,
) -> Result<(), sqlx::Error> {
    let scopes_joined = scopes.join(" ");
    sqlx::query(
        "INSERT INTO oauth_account (provider, account_id, scopes, access_token, expires_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(provider) DO UPDATE SET
            account_id = excluded.account_id,
            scopes = excluded.scopes,
            access_token = excluded.access_token,
            expires_at = excluded.expires_at,
            updated_at = CURRENT_TIMESTAMP",
    )
    .bind(provider)
    .bind(account_id)
    .bind(&scopes_joined)
    .bind(access_token)
    .bind(expires_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete_account(pool: &SqlitePool, provider: &str) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM oauth_account WHERE provider = ?1")
        .bind(provider)
        .execute(pool)
        .await?;
    Ok(())
}

pub fn status_for(account: Option<&OAuthAccount>, provider: &str, configured: bool) -> ConnectionStatus {
    match account {
        Some(a) => ConnectionStatus {
            provider: a.provider.clone(),
            connected: true,
            account_id: a.account_id.clone(),
            scopes: a
                .scopes
                .split_whitespace()
                .map(|s| s.to_string())
                .collect(),
            connected_at: Some(a.connected_at.clone()),
            expires_at: a.expires_at.clone(),
            configured,
        },
        None => ConnectionStatus {
            provider: provider.to_string(),
            connected: false,
            account_id: None,
            scopes: Vec::new(),
            connected_at: None,
            expires_at: None,
            configured,
        },
    }
}
