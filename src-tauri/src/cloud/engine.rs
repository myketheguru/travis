//! Continuous sync engine — Phase 2.2 of the v2 cloud-first
//! architecture.
//!
//! Runs an idempotent push+pull cycle:
//!
//!   1. Drain the `sync_outbox` table → POST /sync/push in batches.
//!      Successful pushes delete their outbox rows. Failures bump
//!      attempts and stash last_error.
//!
//!   2. GET /sync/pull?since=<cursor> → apply incoming events that
//!      *weren't* originated by us (source-device suppression), then
//!      advance the local cursor.
//!
//! Conflict resolution: last-write-wins. For settings.set today, that
//! means "remote event applied means remote value overwrites local
//! value." We rely on the cloud's monotonic cursor to order events.
//!
//! Apply support is intentionally narrow this session — settings.set
//! only. Profile / memory / conversation apply lands in Phase 2.3.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row;
use tokio::time::sleep;

use super::{read_jwt, CLOUD_BASE};
use crate::db::Db;

const META_CURSOR: &str = "cloud_sync_cursor";
const META_LAST_SYNC: &str = "cloud_sync_last_at";
const META_LAST_ERROR: &str = "cloud_sync_last_error";
const PUSH_BATCH_SIZE: usize = 200;
const PULL_PAGE_SIZE: u32 = 500;
const MAX_OUTBOX_ATTEMPTS: i64 = 6;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub cursor: String,
    pub pending_outbox: u32,
    pub failing_outbox: u32,
    pub last_sync_at: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncRunResult {
    pub pushed: u32,
    pub pulled_applied: u32,
    pub pulled_skipped: u32,
    pub cursor: String,
}

#[derive(Debug, Clone, Serialize)]
struct OutboundChange {
    kind: String,
    payload: Value,
    #[serde(skip_serializing_if = "Option::is_none", rename = "sourceDevice")]
    source_device: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PushResponse {
    accepted: Vec<Value>,
    #[allow(dead_code)]
    skipped: Vec<Value>,
    #[allow(dead_code)]
    cursor: String,
}

#[derive(Debug, Deserialize)]
struct PullResponse {
    head: String,
    changes: Vec<PullChange>,
    #[serde(default, rename = "hasMore")]
    has_more: bool,
}

#[derive(Debug, Deserialize)]
struct PullChange {
    cursor: String,
    kind: String,
    payload: Value,
    #[serde(default, rename = "sourceDevice")]
    source_device: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SyncEngine {
    http: reqwest::Client,
    device_id: String,
}

impl SyncEngine {
    pub fn new(http: reqwest::Client, device_id: String) -> Self {
        Self { http, device_id }
    }

    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    fn auth(&self) -> anyhow::Result<String> {
        let jwt =
            read_jwt().ok_or_else(|| anyhow::anyhow!("sync: not signed in"))?;
        Ok(format!("Bearer {jwt}"))
    }

    pub async fn status(&self, db: &Db) -> anyhow::Result<SyncStatus> {
        let cursor = db.meta(META_CURSOR).await?.unwrap_or_else(|| "0".to_string());
        let last_sync_at = db.meta(META_LAST_SYNC).await?;
        let last_error = db.meta(META_LAST_ERROR).await?;

        let pending: i64 = sqlx::query("SELECT COUNT(*) FROM sync_outbox")
            .fetch_one(&db.pool)
            .await
            .map(|r| r.get(0))
            .unwrap_or(0);
        let failing: i64 = sqlx::query("SELECT COUNT(*) FROM sync_outbox WHERE attempts > 0")
            .fetch_one(&db.pool)
            .await
            .map(|r| r.get(0))
            .unwrap_or(0);

        Ok(SyncStatus {
            cursor,
            pending_outbox: pending.max(0) as u32,
            failing_outbox: failing.max(0) as u32,
            last_sync_at,
            last_error,
        })
    }

    /// Run one full push + pull cycle. Returns counts so the UI can
    /// surface a small confirmation toast on manual triggers.
    pub async fn run_once(&self, db: &Db) -> anyhow::Result<SyncRunResult> {
        let pushed = self.drain_outbox(db).await?;
        let (pulled_applied, pulled_skipped, cursor) = self.pull_and_apply(db).await?;

        db.set_meta_from_remote(META_LAST_SYNC, &chrono::Utc::now().to_rfc3339())
            .await
            .ok();
        // Clear any prior error string on a successful pass.
        db.set_meta_from_remote(META_LAST_ERROR, "").await.ok();

        Ok(SyncRunResult {
            pushed,
            pulled_applied,
            pulled_skipped,
            cursor,
        })
    }

    // --- push --------------------------------------------------------

    async fn drain_outbox(&self, db: &Db) -> anyhow::Result<u32> {
        let mut pushed_total = 0u32;
        loop {
            let rows = sqlx::query(
                "SELECT id, kind, payload FROM sync_outbox \
                 WHERE attempts < ?1 \
                 ORDER BY id LIMIT ?2",
            )
            .bind(MAX_OUTBOX_ATTEMPTS)
            .bind(PUSH_BATCH_SIZE as i64)
            .fetch_all(&db.pool)
            .await?;
            if rows.is_empty() {
                break;
            }
            let mut ids: Vec<i64> = Vec::with_capacity(rows.len());
            let mut changes: Vec<OutboundChange> = Vec::with_capacity(rows.len());
            for row in &rows {
                let id: i64 = row.get(0);
                let kind: String = row.get(1);
                let raw: String = row.get(2);
                let payload = serde_json::from_str::<Value>(&raw)
                    .unwrap_or(Value::Null);
                ids.push(id);
                changes.push(OutboundChange {
                    kind,
                    payload,
                    source_device: Some(self.device_id.clone()),
                });
            }
            match self.push(changes).await {
                Ok(resp) => {
                    pushed_total += resp.accepted.len() as u32;
                    // Delete the just-pushed rows. Even partially-skipped
                    // batches drop their ids — skipped events are rejected
                    // for a kind/shape reason that retrying won't fix.
                    if !ids.is_empty() {
                        let placeholders =
                            std::iter::repeat("?").take(ids.len()).collect::<Vec<_>>().join(",");
                        let sql = format!("DELETE FROM sync_outbox WHERE id IN ({placeholders})");
                        let mut q = sqlx::query(&sql);
                        for id in &ids {
                            q = q.bind(*id);
                        }
                        q.execute(&db.pool).await.ok();
                    }
                }
                Err(e) => {
                    let err_str = e.to_string();
                    let placeholders =
                        std::iter::repeat("?").take(ids.len()).collect::<Vec<_>>().join(",");
                    let sql = format!(
                        "UPDATE sync_outbox SET attempts = attempts + 1, last_error = ?1 \
                         WHERE id IN ({placeholders})"
                    );
                    let mut q = sqlx::query(&sql).bind(&err_str);
                    for id in &ids {
                        q = q.bind(*id);
                    }
                    q.execute(&db.pool).await.ok();
                    db.set_meta_from_remote(META_LAST_ERROR, &err_str).await.ok();
                    return Err(e);
                }
            }
        }
        Ok(pushed_total)
    }

    async fn push(&self, changes: Vec<OutboundChange>) -> anyhow::Result<PushResponse> {
        let auth = self.auth()?;
        let resp = self
            .http
            .post(format!("{CLOUD_BASE}/sync/push"))
            .header("authorization", auth)
            .timeout(Duration::from_secs(45))
            .json(&serde_json::json!({ "changes": changes }))
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("/sync/push {}: {body}", status.as_u16());
        }
        Ok(resp.json::<PushResponse>().await?)
    }

    // --- pull --------------------------------------------------------

    async fn pull_and_apply(&self, db: &Db) -> anyhow::Result<(u32, u32, String)> {
        let mut cursor = db.meta(META_CURSOR).await?.unwrap_or_else(|| "0".to_string());
        let mut applied = 0u32;
        let mut skipped_self = 0u32;

        loop {
            let pulled = self.pull(&cursor).await?;
            let head = pulled.head.clone();
            let has_more = pulled.has_more;

            if pulled.changes.is_empty() {
                cursor = head;
                break;
            }
            for change in &pulled.changes {
                if change.source_device.as_deref() == Some(self.device_id.as_str()) {
                    skipped_self += 1;
                    continue;
                }
                match self.apply(db, change).await {
                    Ok(true) => applied += 1,
                    Ok(false) => {}
                    Err(e) => {
                        tracing::warn!(
                            "sync apply failed (kind={}, cursor={}): {e}",
                            change.kind,
                            change.cursor
                        );
                    }
                }
            }
            // Advance the cursor to the last item of this page even on
            // partial apply failures — those are recorded but shouldn't
            // pin us forever. Next pull picks up where we left off.
            cursor = pulled
                .changes
                .last()
                .map(|c| c.cursor.clone())
                .unwrap_or_else(|| head.clone());
            if !has_more {
                cursor = head;
                break;
            }
            // Yield between pages so we don't starve other tasks.
            sleep(Duration::from_millis(10)).await;
        }

        db.set_meta_from_remote(META_CURSOR, &cursor).await?;
        Ok((applied, skipped_self, cursor))
    }

    async fn pull(&self, since: &str) -> anyhow::Result<PullResponse> {
        let auth = self.auth()?;
        let url = format!(
            "{CLOUD_BASE}/sync/pull?since={}&limit={}",
            urlencoding::encode(since),
            PULL_PAGE_SIZE
        );
        let resp = self
            .http
            .get(&url)
            .header("authorization", auth)
            .timeout(Duration::from_secs(30))
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("/sync/pull {status}: {body}");
        }
        Ok(resp.json::<PullResponse>().await?)
    }

    /// Apply a single change locally without re-enqueueing. Returns
    /// `Ok(true)` if applied, `Ok(false)` if the event was a known
    /// kind we don't yet apply (e.g. memory.add in this slice).
    async fn apply(&self, db: &Db, change: &PullChange) -> anyhow::Result<bool> {
        match change.kind.as_str() {
            "settings.set" => {
                let key = change
                    .payload
                    .get("key")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("settings.set missing key"))?;
                let value = change
                    .payload
                    .get("value")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("settings.set missing value"))?;
                db.set_meta_from_remote(key, value).await?;
                Ok(true)
            }
            "profile.set" => {
                let name = change
                    .payload
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let role = change
                    .payload
                    .get("role")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let org = change
                    .payload
                    .get("org")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let context_blurb = change
                    .payload
                    .get("contextBlurb")
                    .and_then(|v| v.as_str());
                let communication_style = change
                    .payload
                    .get("communicationStyle")
                    .and_then(|v| v.as_str());
                db.upsert_user_profile_from_remote(
                    name,
                    role,
                    org,
                    context_blurb,
                    communication_style,
                )
                .await?;
                Ok(true)
            }
            _ => {
                // memory.add and conversation.upsert are pushed (so the
                // cloud accumulates the full graph) but local apply for
                // them is deferred. Need stable cloud_id columns first
                // so a re-pulled event doesn't double-insert / overwrite
                // the user's local edits. Phase 2.4.
                Ok(false)
            }
        }
    }
}
