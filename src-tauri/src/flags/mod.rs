use anyhow::{Context, Result};
use serde_json::{json, Value};
use sqlx::SqlitePool;

pub type Flags = Value;

pub const DEFAULT_FLAGS_URL: &str = "https://leadtoempower.github.io/travis/config.json";

fn empty_flags() -> Flags {
    json!({ "features": {} })
}

/// Read the flags URL from `meta`, defaulting to the placeholder URL if unset.
pub async fn get_flags_url(pool: &SqlitePool) -> Result<String> {
    let row: Option<(String,)> = sqlx::query_as("SELECT value FROM meta WHERE key = ?1")
        .bind("flags_url")
        .fetch_optional(pool)
        .await
        .context("failed to read flags_url from meta")?;
    Ok(row.map(|r| r.0).unwrap_or_else(|| DEFAULT_FLAGS_URL.to_string()))
}

/// Update the configured flags URL in meta.
pub async fn set_flags_url(pool: &SqlitePool, url: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO meta(key, value, updated_at) VALUES ('flags_url', ?1, CURRENT_TIMESTAMP)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = CURRENT_TIMESTAMP",
    )
    .bind(url)
    .execute(pool)
    .await
    .context("failed to upsert flags_url")?;
    Ok(())
}

/// Read the latest cached flags. Returns the empty default `{ "features": {} }`
/// if no row has been written yet.
pub async fn cached(pool: &SqlitePool) -> Result<Flags> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT flags_json FROM feature_flag_cache WHERE id = 1",
    )
    .fetch_optional(pool)
    .await
    .context("failed to read feature_flag_cache")?;

    match row {
        Some((json_str,)) => {
            let v: Value = serde_json::from_str(&json_str).unwrap_or_else(|_| empty_flags());
            Ok(v)
        }
        None => Ok(empty_flags()),
    }
}

/// Convenience: read a single flag from `cached().features[name]`.
pub async fn get_flag(pool: &SqlitePool, name: &str) -> Result<Option<Value>> {
    let cached = cached(pool).await?;
    Ok(cached
        .get("features")
        .and_then(|f| f.get(name))
        .cloned())
}

/// Fetch fresh flags from the configured URL and upsert into the cache.
/// On failure, returns the existing cached flags (or empty defaults).
pub async fn refresh(pool: &SqlitePool, http: &reqwest::Client) -> Result<Flags> {
    let url = get_flags_url(pool).await?;

    let resp = match http.get(&url).send().await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("flags refresh: GET {} failed: {}", url, e);
            return cached(pool).await;
        }
    };

    if !resp.status().is_success() {
        tracing::warn!("flags refresh: {} returned status {}", url, resp.status());
        return cached(pool).await;
    }

    let etag = resp
        .headers()
        .get(reqwest::header::ETAG)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let body = match resp.text().await {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!("flags refresh: failed to read body: {}", e);
            return cached(pool).await;
        }
    };

    let parsed: Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("flags refresh: invalid JSON from {}: {}", url, e);
            return cached(pool).await;
        }
    };

    let canonical = serde_json::to_string(&parsed).unwrap_or(body);

    sqlx::query(
        "INSERT INTO feature_flag_cache (id, flags_json, fetched_at, etag, source_url)
         VALUES (1, ?1, CURRENT_TIMESTAMP, ?2, ?3)
         ON CONFLICT(id) DO UPDATE SET
            flags_json = excluded.flags_json,
            fetched_at = CURRENT_TIMESTAMP,
            etag       = excluded.etag,
            source_url = excluded.source_url",
    )
    .bind(&canonical)
    .bind(&etag)
    .bind(&url)
    .execute(pool)
    .await
    .context("failed to upsert feature_flag_cache")?;

    Ok(parsed)
}
