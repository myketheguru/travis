//! Cloud sync — Phase 2.1 of the v2 cloud-first architecture.
//!
//! Wraps the cloud's /sync/* endpoints with a typed Rust client and
//! exposes the first useful operation on top: **migrating an existing
//! local install** to the cloud.
//!
//! Continuous bidirectional sync (write-through cache, pull-on-launch,
//! conflict resolution) lands in Phase 2.2.
//!
//! Migration model:
//!   - User signs in (Phase 1)
//!   - Onboarding finishes (Phase 1.5)
//!   - We check `meta.cloud_migration_status`:
//!       "complete"     → skip prompt, cloud is source of truth
//!       "skipped"      → skip prompt for this session (will offer again later)
//!       "fresh"        → local stays untouched, cloud started empty
//!       missing/empty  → show the migration prompt
//!   - User picks Upload / Fresh / Skip
//!   - We record the choice + a per-kind count for what was pushed.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::Row;

use super::{read_jwt, CLOUD_BASE};
use crate::db::Db;

const META_MIGRATION_STATUS: &str = "cloud_migration_status";
const META_MIGRATION_DETAILS: &str = "cloud_migration_details";
const PUSH_BATCH_SIZE: usize = 200;

/// A single change as the cloud expects it in /sync/push.
#[derive(Debug, Clone, Serialize)]
struct OutboundChange {
    kind: String,
    payload: Value,
    #[serde(skip_serializing_if = "Option::is_none", rename = "sourceDevice")]
    source_device: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PushResponse {
    #[allow(dead_code)]
    ok: bool,
    accepted: Vec<Value>,
    skipped: Vec<Value>,
    #[allow(dead_code)]
    cursor: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationStatus {
    pub status: String,
    pub local_counts: LocalCounts,
    pub details: Option<MigrationDetails>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LocalCounts {
    pub profile: u32,
    pub memories: u32,
    pub conversations: u32,
    pub conversation_messages: u32,
    pub settings: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MigrationDetails {
    pub pushed: u32,
    pub skipped: u32,
    pub at: String,
    pub decision: String,
}

/// Light client around the cloud /sync endpoints. Each call uses the
/// session JWT from the keychain.
struct SyncClient {
    http: reqwest::Client,
    jwt: String,
}

impl SyncClient {
    fn current(http: reqwest::Client) -> anyhow::Result<Self> {
        let jwt = read_jwt().ok_or_else(|| anyhow::anyhow!("not signed in"))?;
        Ok(Self { http, jwt })
    }

    fn auth(&self) -> String {
        format!("Bearer {}", self.jwt)
    }

    async fn push(&self, changes: Vec<OutboundChange>) -> anyhow::Result<PushResponse> {
        let resp = self
            .http
            .post(format!("{CLOUD_BASE}/sync/push"))
            .header("authorization", self.auth())
            .timeout(Duration::from_secs(45))
            .json(&json!({ "changes": changes }))
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("/sync/push {}: {body}", status.as_u16());
        }
        Ok(resp.json::<PushResponse>().await?)
    }

    /// Push `batch` if it has reached PUSH_BATCH_SIZE; do nothing otherwise.
    /// Updates the running counters in place. Caller flushes the
    /// remainder at the end with `flush()`.
    async fn maybe_flush(
        &self,
        batch: &mut Vec<OutboundChange>,
        pushed: &mut u32,
        skipped: &mut u32,
    ) -> anyhow::Result<()> {
        if batch.len() >= PUSH_BATCH_SIZE {
            self.flush(batch, pushed, skipped).await?;
        }
        Ok(())
    }

    async fn flush(
        &self,
        batch: &mut Vec<OutboundChange>,
        pushed: &mut u32,
        skipped: &mut u32,
    ) -> anyhow::Result<()> {
        if batch.is_empty() {
            return Ok(());
        }
        let take = std::mem::take(batch);
        let resp = self.push(take).await?;
        *pushed += resp.accepted.len() as u32;
        *skipped += resp.skipped.len() as u32;
        Ok(())
    }
}

// --- Status -------------------------------------------------------------

pub async fn migration_status(db: &Db) -> anyhow::Result<MigrationStatus> {
    let status = db
        .meta(META_MIGRATION_STATUS)
        .await?
        .unwrap_or_default();
    let details = match db.meta(META_MIGRATION_DETAILS).await? {
        Some(s) => serde_json::from_str::<MigrationDetails>(&s).ok(),
        None => None,
    };
    let local_counts = count_locally(db).await?;
    Ok(MigrationStatus {
        status,
        local_counts,
        details,
    })
}

async fn count_locally(db: &Db) -> anyhow::Result<LocalCounts> {
    let pool = &db.pool;
    let profile: i64 = sqlx::query("SELECT COUNT(*) FROM user_profile")
        .fetch_one(pool)
        .await
        .map(|r| r.get(0))
        .unwrap_or(0);
    let memories: i64 = sqlx::query("SELECT COUNT(*) FROM embedding")
        .fetch_one(pool)
        .await
        .map(|r| r.get(0))
        .unwrap_or(0);
    let conversations: i64 = sqlx::query("SELECT COUNT(*) FROM conversation")
        .fetch_one(pool)
        .await
        .map(|r| r.get(0))
        .unwrap_or(0);
    let conversation_messages: i64 = sqlx::query("SELECT COUNT(*) FROM conversation_message")
        .fetch_one(pool)
        .await
        .map(|r| r.get(0))
        .unwrap_or(0);
    let settings: i64 = sqlx::query("SELECT COUNT(*) FROM meta")
        .fetch_one(pool)
        .await
        .map(|r| r.get(0))
        .unwrap_or(0);
    Ok(LocalCounts {
        profile: profile.max(0) as u32,
        memories: memories.max(0) as u32,
        conversations: conversations.max(0) as u32,
        conversation_messages: conversation_messages.max(0) as u32,
        settings: settings.max(0) as u32,
    })
}

// --- Decision recording -------------------------------------------------

/// Mark the user's choice without uploading anything. Used by the
/// "Start fresh" and "Skip for now" paths.
pub async fn record_decision(db: &Db, decision: &str, status: &str) -> anyhow::Result<()> {
    db.set_meta(META_MIGRATION_STATUS, status).await?;
    let details = MigrationDetails {
        pushed: 0,
        skipped: 0,
        at: chrono::Utc::now().to_rfc3339(),
        decision: decision.to_string(),
    };
    db.set_meta(
        META_MIGRATION_DETAILS,
        &serde_json::to_string(&details).unwrap_or_else(|_| "{}".to_string()),
    )
    .await?;
    Ok(())
}

// --- Upload pipeline ----------------------------------------------------

/// Stream the local DB into /sync/push as a series of change events.
/// Returns the final counts so the UI can confirm what happened.
///
/// Each kind family is collected into outbound batches of PUSH_BATCH_SIZE
/// to keep individual requests under the cloud's per-call ceiling. The
/// cloud DO will skip any kinds it doesn't recognise; we track those
/// counts and surface them so the user can tell if anything was dropped.
pub async fn upload_local(
    http: reqwest::Client,
    db: &Db,
    source_device: Option<String>,
) -> anyhow::Result<MigrationDetails> {
    let client = SyncClient::current(http)?;

    let mut pushed = 0u32;
    let mut skipped = 0u32;
    let mut batch: Vec<OutboundChange> = Vec::with_capacity(PUSH_BATCH_SIZE);

    // --- profile -------------------------------------------------------
    if let Some(profile) = db.user_profile().await.ok().flatten() {
        batch.push(OutboundChange {
            kind: "profile.set".to_string(),
            payload: json!({
                "name": profile.name,
                "role": profile.role,
                "org": profile.org,
                "contextBlurb": profile.context_blurb,
                "communicationStyle": profile.communication_style,
            }),
            source_device: source_device.clone(),
        });
    }

    // --- meta (settings) -----------------------------------------------
    let settings_rows = sqlx::query("SELECT key, value FROM meta")
        .fetch_all(&db.pool)
        .await
        .unwrap_or_default();
    for row in settings_rows {
        let key: String = row.get(0);
        let value: String = row.get(1);
        // Migration status itself stays local; we don't want to ship it
        // around or have it appear as a setting in the cloud.
        if key.starts_with("cloud_migration_") {
            continue;
        }
        batch.push(OutboundChange {
            kind: "settings.set".to_string(),
            payload: json!({ "key": key, "value": value }),
            source_device: source_device.clone(),
        });
        client
            .maybe_flush(&mut batch, &mut pushed, &mut skipped)
            .await?;
    }
    client
        .flush(&mut batch, &mut pushed, &mut skipped)
        .await?;

    // --- memory entries (embedding.text only — vectors regenerate on cloud)
    let memory_rows = sqlx::query(
        "SELECT id, source_kind, source_id, text, created_at FROM embedding ORDER BY id",
    )
    .fetch_all(&db.pool)
    .await
    .unwrap_or_default();
    for row in memory_rows {
        let id: i64 = row.get(0);
        let source_kind: String = row.get(1);
        let source_id: i64 = row.get(2);
        let text: String = row.get(3);
        let created_at: String = row.get(4);
        batch.push(OutboundChange {
            kind: "memory.add".to_string(),
            payload: json!({
                "localId": id,
                "sourceKind": source_kind,
                "sourceId": source_id,
                "text": text,
                "createdAt": created_at,
            }),
            source_device: source_device.clone(),
        });
        client
            .maybe_flush(&mut batch, &mut pushed, &mut skipped)
            .await?;
    }
    client
        .flush(&mut batch, &mut pushed, &mut skipped)
        .await?;

    // --- conversations (with messages embedded as one event per convo)
    let conv_rows = sqlx::query(
        "SELECT id, kind, title, status, created_at, updated_at FROM conversation ORDER BY id",
    )
    .fetch_all(&db.pool)
    .await
    .unwrap_or_default();
    for row in conv_rows {
        let id: i64 = row.get(0);
        let kind: String = row.get(1);
        let title: Option<String> = row.try_get(2).ok();
        let status: String = row.get(3);
        let created_at: String = row.get(4);
        let updated_at: String = row.get(5);

        let msg_rows = sqlx::query(
            "SELECT role, content, payload_json, created_at \
             FROM conversation_message WHERE conversation_id = ?1 ORDER BY id",
        )
        .bind(id)
        .fetch_all(&db.pool)
        .await
        .unwrap_or_default();
        let messages: Vec<Value> = msg_rows
            .into_iter()
            .map(|m| {
                let role: String = m.get(0);
                let content: String = m.get(1);
                let payload: Option<String> = m.try_get(2).ok();
                let created_at: String = m.get(3);
                json!({
                    "role": role,
                    "content": content,
                    "payload": payload.and_then(|s| serde_json::from_str::<Value>(&s).ok()),
                    "createdAt": created_at,
                })
            })
            .collect();

        batch.push(OutboundChange {
            kind: "conversation.upsert".to_string(),
            payload: json!({
                "localId": id,
                "kind": kind,
                "title": title,
                "status": status,
                "createdAt": created_at,
                "updatedAt": updated_at,
                "messages": messages,
            }),
            source_device: source_device.clone(),
        });
        client
            .maybe_flush(&mut batch, &mut pushed, &mut skipped)
            .await?;
    }
    client
        .flush(&mut batch, &mut pushed, &mut skipped)
        .await?;

    // Record the result locally so we don't prompt again.
    let details = MigrationDetails {
        pushed,
        skipped,
        at: chrono::Utc::now().to_rfc3339(),
        decision: "upload".to_string(),
    };
    db.set_meta(META_MIGRATION_STATUS, "complete").await?;
    db.set_meta(
        META_MIGRATION_DETAILS,
        &serde_json::to_string(&details).unwrap_or_else(|_| "{}".to_string()),
    )
    .await?;

    Ok(details)
}
