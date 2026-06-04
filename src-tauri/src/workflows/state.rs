//! Per-conversation workflow state — what recipe is active, which
//! slots are filled, when each was filled, where the value came from.
//!
//! One active workflow per conversation for v1. If real usage shows
//! Taylor stacking workflows ("invoice PS498… actually first finish
//! the contract draft I started earlier"), we can lift that constraint.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::collections::HashMap;

/// Where a slot's value came from. Used by Travis to decide when to
/// double-check ("I extracted 29.5 hours from your sheet — confirm?")
/// vs trust silently ("you said math team").
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SlotSource {
    /// User typed it directly in the conversation.
    UserTyped,
    /// User dropped a document; Travis extracted the field.
    Extracted,
    /// User dropped a document; the document itself is the slot value.
    UserDropped,
    /// Travis found it in the graph (existing engagement, linked PO, etc.)
    /// and is using it as a confident default.
    GraphResolved,
}

/// One slot's filled state. The value is stored as raw JSON so the
/// shape can vary by SlotKind (Entity → {id, name}, Document → {id,
/// path, kind}, Money → cents, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowSlotValue {
    pub value: serde_json::Value,
    pub source: SlotSource,
    pub resolved_at: String,
}

/// One persisted workflow row. Mirrors the workflow_state SQL table.
#[derive(Debug, Clone)]
pub struct WorkflowState {
    pub id: i64,
    pub conversation_id: i64,
    pub recipe_name: String,
    pub status: String,
    pub slots: HashMap<String, WorkflowSlotValue>,
    pub started_intent: Option<String>,
    pub started_at: String,
    pub last_activity_at: String,
    pub completed_at: Option<String>,
}

#[derive(sqlx::FromRow)]
struct WorkflowStateRow {
    id: i64,
    conversation_id: i64,
    recipe_name: String,
    status: String,
    slots_json: String,
    started_intent: Option<String>,
    started_at: String,
    last_activity_at: String,
    completed_at: Option<String>,
}

impl From<WorkflowStateRow> for WorkflowState {
    fn from(r: WorkflowStateRow) -> Self {
        let slots: HashMap<String, WorkflowSlotValue> =
            serde_json::from_str(&r.slots_json).unwrap_or_default();
        WorkflowState {
            id: r.id,
            conversation_id: r.conversation_id,
            recipe_name: r.recipe_name,
            status: r.status,
            slots,
            started_intent: r.started_intent,
            started_at: r.started_at,
            last_activity_at: r.last_activity_at,
            completed_at: r.completed_at,
        }
    }
}

/// Get the active workflow for a conversation, if any.
pub async fn get_active(
    pool: &SqlitePool,
    conversation_id: i64,
) -> Option<WorkflowState> {
    sqlx::query_as::<_, WorkflowStateRow>(
        "SELECT id, conversation_id, recipe_name, status, slots_json,
                started_intent, started_at, last_activity_at, completed_at
         FROM workflow_state
         WHERE conversation_id = ?1 AND status = 'active'
         ORDER BY id DESC LIMIT 1",
    )
    .bind(conversation_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .map(Into::into)
}

/// Start a new workflow on a conversation. If one is already active,
/// abandon it first — Taylor's intent supersedes whatever was in flight.
pub async fn start(
    pool: &SqlitePool,
    conversation_id: i64,
    recipe_name: &str,
    started_intent: Option<&str>,
) -> anyhow::Result<WorkflowState> {
    // Abandon any active workflow on this conversation first.
    sqlx::query(
        "UPDATE workflow_state
         SET status = 'abandoned',
             last_activity_at = CURRENT_TIMESTAMP
         WHERE conversation_id = ?1 AND status = 'active'",
    )
    .bind(conversation_id)
    .execute(pool)
    .await?;

    let id: i64 = sqlx::query_scalar(
        "INSERT INTO workflow_state
            (conversation_id, recipe_name, slots_json, started_intent)
         VALUES (?1, ?2, '{}', ?3)
         RETURNING id",
    )
    .bind(conversation_id)
    .bind(recipe_name)
    .bind(started_intent)
    .fetch_one(pool)
    .await?;

    get_by_id(pool, id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("workflow_state {id} disappeared after insert"))
}

/// Fill a single slot. Overwrites any previous value for the same slot
/// — the LLM is responsible for re-asking if it needs to revise.
pub async fn fill_slot(
    pool: &SqlitePool,
    workflow_id: i64,
    slot_name: &str,
    value: serde_json::Value,
    source: SlotSource,
) -> anyhow::Result<WorkflowState> {
    let row = get_by_id(pool, workflow_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("workflow {workflow_id} not found"))?;

    let mut slots = row.slots;
    let now = chrono::Utc::now().to_rfc3339();
    slots.insert(
        slot_name.to_string(),
        WorkflowSlotValue {
            value,
            source,
            resolved_at: now,
        },
    );

    let slots_json = serde_json::to_string(&slots)?;
    sqlx::query(
        "UPDATE workflow_state
         SET slots_json = ?1, last_activity_at = CURRENT_TIMESTAMP
         WHERE id = ?2",
    )
    .bind(&slots_json)
    .bind(workflow_id)
    .execute(pool)
    .await?;

    get_by_id(pool, workflow_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("workflow {workflow_id} disappeared"))
}

/// Mark a workflow completed — called once the finalize action has
/// been proposed (or applied, depending on UX). The workflow stops
/// surfacing in the prompt block from this point.
pub async fn mark_completed(
    pool: &SqlitePool,
    workflow_id: i64,
) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE workflow_state
         SET status = 'completed',
             completed_at = CURRENT_TIMESTAMP,
             last_activity_at = CURRENT_TIMESTAMP
         WHERE id = ?1",
    )
    .bind(workflow_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Abandon — the user changed subject or explicitly cancelled.
pub async fn mark_abandoned(
    pool: &SqlitePool,
    workflow_id: i64,
) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE workflow_state
         SET status = 'abandoned',
             last_activity_at = CURRENT_TIMESTAMP
         WHERE id = ?1",
    )
    .bind(workflow_id)
    .execute(pool)
    .await?;
    Ok(())
}

async fn get_by_id(
    pool: &SqlitePool,
    id: i64,
) -> anyhow::Result<Option<WorkflowState>> {
    let row = sqlx::query_as::<_, WorkflowStateRow>(
        "SELECT id, conversation_id, recipe_name, status, slots_json,
                started_intent, started_at, last_activity_at, completed_at
         FROM workflow_state
         WHERE id = ?1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(Into::into))
}
