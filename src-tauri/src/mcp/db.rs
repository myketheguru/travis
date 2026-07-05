//! MCP server config persistence.
//!
//! One row per configured MCP server. On startup, we iterate this
//! list + register each server's tools.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServer {
    pub id: i64,
    /// Short slug used as tool name prefix. Unique.
    pub slug: String,
    /// Human display label.
    pub label: String,
    /// Full HTTP endpoint. POST JSON-RPC calls go here.
    pub url: String,
    /// Optional bearer token, stored plain in the local DB — we're
    /// desktop-first and the DB lives on the user's machine. Never
    /// synced to cloud (see 0056 migration comment).
    pub auth_token: Option<String>,
    pub enabled: bool,
    pub created_at: String,
}

pub async fn list(pool: &SqlitePool) -> Result<Vec<McpServer>> {
    let rows = sqlx::query(
        "SELECT id, slug, label, url, auth_token, enabled, created_at
         FROM mcp_server
         ORDER BY created_at ASC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| McpServer {
            id: r.get("id"),
            slug: r.get("slug"),
            label: r.get("label"),
            url: r.get("url"),
            auth_token: r.get("auth_token"),
            enabled: r.get::<i64, _>("enabled") != 0,
            created_at: r.get("created_at"),
        })
        .collect())
}

pub async fn upsert(
    pool: &SqlitePool,
    slug: &str,
    label: &str,
    url: &str,
    auth_token: Option<&str>,
    enabled: bool,
) -> Result<i64> {
    let row = sqlx::query(
        "INSERT INTO mcp_server (slug, label, url, auth_token, enabled)
         VALUES (?, ?, ?, ?, ?)
         ON CONFLICT(slug) DO UPDATE SET
             label = excluded.label,
             url = excluded.url,
             auth_token = excluded.auth_token,
             enabled = excluded.enabled
         RETURNING id",
    )
    .bind(slug)
    .bind(label)
    .bind(url)
    .bind(auth_token)
    .bind(enabled as i64)
    .fetch_one(pool)
    .await?;
    Ok(row.get::<i64, _>(0))
}

pub async fn delete(pool: &SqlitePool, id: i64) -> Result<()> {
    sqlx::query("DELETE FROM mcp_server WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_enabled(pool: &SqlitePool, id: i64, enabled: bool) -> Result<()> {
    sqlx::query("UPDATE mcp_server SET enabled = ? WHERE id = ?")
        .bind(enabled as i64)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}
