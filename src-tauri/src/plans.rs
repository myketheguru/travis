//! v0.20.12 — first-class plan + step substrate.
//!
//! The agent loop's "do everything from scratch every turn" pattern
//! turned the IS 217 invoice into a 15-minute flow with 50+ run_python
//! calls because every regeneration re-read the spreadsheet, re-filtered
//! the dates, re-rendered the PDF. Plans give the LLM a place to
//! record "I read the sign-in log, here's the parsed result" and the
//! NEXT turn (or even the next attempt in the same turn) can return
//! that cached output without rerunning.
//!
//! Shape:
//!
//! ```text
//! plan (id, conversation_id, goal, status)
//!   └── plan_step (plan_id, key, purpose, status, result_json, document_ids)
//! ```
//!
//! Lifecycle:
//!
//! 1. `plan_create(conversationId, goal, steps)` — LLM declares a plan
//!    at the start of a complex turn. Returns `planId`.
//! 2. `plan_step_record(planId, key, result, documentIds?)` — LLM
//!    records the result of running a step. Idempotent on (plan_id, key).
//! 3. `plan_step_get(planId, key)` — fast cache lookup. Returns the
//!    cached result_json if status='done', or None if not yet done.
//! 4. `plan_status(planId)` — returns full state for a plan.
//!
//! The substrate is intentionally simple. The LLM decides what each
//! step DOES (calling other tools to actually do the work) — plans
//! just remember what happened.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tauri::State;

use crate::AppState;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub id: i64,
    pub conversation_id: i64,
    pub goal: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct PlanStep {
    pub id: i64,
    pub plan_id: i64,
    pub key: String,
    pub purpose: String,
    pub status: String,
    pub depends_on: Option<String>,
    pub result_json: Option<String>,
    pub document_ids: Option<String>,
    pub result_hash: Option<String>,
    pub error: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanStepInput {
    pub key: String,
    pub purpose: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
}

pub async fn create_plan(
    pool: &SqlitePool,
    conversation_id: i64,
    goal: &str,
    steps: &[PlanStepInput],
) -> anyhow::Result<i64> {
    let plan_id: i64 = sqlx::query_scalar(
        "INSERT INTO plan (conversation_id, goal) VALUES (?1, ?2) RETURNING id",
    )
    .bind(conversation_id)
    .bind(goal.trim())
    .fetch_one(pool)
    .await?;
    for step in steps {
        let depends_on = if step.depends_on.is_empty() {
            None
        } else {
            Some(step.depends_on.join(","))
        };
        sqlx::query(
            "INSERT INTO plan_step (plan_id, key, purpose, depends_on)
             VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(plan_id)
        .bind(step.key.trim())
        .bind(step.purpose.trim())
        .bind(depends_on)
        .execute(pool)
        .await?;
    }
    Ok(plan_id)
}

pub async fn record_step(
    pool: &SqlitePool,
    plan_id: i64,
    key: &str,
    status: &str,
    result_json: Option<&str>,
    document_ids: Option<&[i64]>,
    error: Option<&str>,
) -> anyhow::Result<()> {
    let docs_str = document_ids.map(|ids| {
        ids.iter()
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(",")
    });
    let hash = result_json.map(|r| {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(r.as_bytes());
        let digest = h.finalize();
        let mut s = String::with_capacity(digest.len() * 2);
        for b in digest.iter() {
            s.push_str(&format!("{b:02x}"));
        }
        s
    });
    sqlx::query(
        "UPDATE plan_step SET
             status = ?3,
             result_json = COALESCE(?4, result_json),
             document_ids = COALESCE(?5, document_ids),
             result_hash = COALESCE(?6, result_hash),
             error = ?7,
             started_at = COALESCE(started_at, datetime('now')),
             completed_at = CASE WHEN ?3 IN ('done', 'failed', 'skipped')
                                 THEN datetime('now') ELSE completed_at END,
             updated_at = datetime('now')
         WHERE plan_id = ?1 AND key = ?2",
    )
    .bind(plan_id)
    .bind(key)
    .bind(status)
    .bind(result_json)
    .bind(docs_str)
    .bind(hash)
    .bind(error)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_step(
    pool: &SqlitePool,
    plan_id: i64,
    key: &str,
) -> anyhow::Result<Option<PlanStep>> {
    Ok(sqlx::query_as::<_, PlanStep>(
        "SELECT id, plan_id, key, purpose, status, depends_on, result_json,
                document_ids, result_hash, error, started_at, completed_at,
                created_at, updated_at
         FROM plan_step WHERE plan_id = ?1 AND key = ?2",
    )
    .bind(plan_id)
    .bind(key)
    .fetch_optional(pool)
    .await?)
}

pub async fn list_steps(pool: &SqlitePool, plan_id: i64) -> anyhow::Result<Vec<PlanStep>> {
    Ok(sqlx::query_as::<_, PlanStep>(
        "SELECT id, plan_id, key, purpose, status, depends_on, result_json,
                document_ids, result_hash, error, started_at, completed_at,
                created_at, updated_at
         FROM plan_step WHERE plan_id = ?1 ORDER BY id ASC",
    )
    .bind(plan_id)
    .fetch_all(pool)
    .await?)
}

pub async fn active_plan_for_conversation(
    pool: &SqlitePool,
    conversation_id: i64,
) -> anyhow::Result<Option<Plan>> {
    Ok(sqlx::query_as::<_, Plan>(
        "SELECT id, conversation_id, goal, status, created_at, updated_at
         FROM plan
         WHERE conversation_id = ?1 AND status = 'active'
         ORDER BY id DESC LIMIT 1",
    )
    .bind(conversation_id)
    .fetch_optional(pool)
    .await?)
}

pub async fn close_plan(
    pool: &SqlitePool,
    plan_id: i64,
    status: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE plan SET status = ?2, updated_at = datetime('now') WHERE id = ?1",
    )
    .bind(plan_id)
    .bind(status)
    .execute(pool)
    .await?;
    Ok(())
}

/// v0.20.13 — compute a stable hash of step inputs so the cache can
/// auto-invalidate when ANY input changes.
///
/// Hashes (in order):
/// 1. script source code (or any blob the LLM passed)
/// 2. sorted document ids paired with each doc's `content_hash` from
///    the `document` table — this is the bit that matters: when the
///    user uploads a new sign-in log, the content hash changes,
///    the input hash changes, the cache invalidates.
/// 3. libraries list (sorted) — a fresh dep changes behavior.
///
/// Missing docs are tolerated (hashed as `"missing:{id}"`) so a
/// deleted doc doesn't crash the planner.
pub async fn input_hash(
    pool: &SqlitePool,
    code: &str,
    document_ids: &[i64],
    libraries: &[String],
) -> anyhow::Result<String> {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(b"code:");
    h.update(code.as_bytes());
    h.update(b"\n");

    let mut sorted_ids: Vec<i64> = document_ids.to_vec();
    sorted_ids.sort_unstable();
    for id in sorted_ids {
        h.update(b"doc:");
        h.update(id.to_string().as_bytes());
        h.update(b":");
        let row: Option<(String,)> =
            sqlx::query_as("SELECT content_hash FROM document WHERE id = ?1")
                .bind(id)
                .fetch_optional(pool)
                .await
                .ok()
                .flatten();
        match row {
            Some((hash,)) => h.update(hash.as_bytes()),
            None => h.update(format!("missing:{id}").as_bytes()),
        }
        h.update(b"\n");
    }

    let mut sorted_libs: Vec<&String> = libraries.iter().collect();
    sorted_libs.sort();
    for lib in sorted_libs {
        h.update(b"lib:");
        h.update(lib.as_bytes());
        h.update(b"\n");
    }

    let digest = h.finalize();
    let mut s = String::with_capacity(digest.len() * 2);
    for b in digest.iter() {
        s.push_str(&format!("{b:02x}"));
    }
    Ok(s)
}

/// v0.20.14 — fold prior-step references into a downstream step's
/// input hash so when an upstream cached result changes (different
/// `result_hash`), the downstream step's hash flips too. Upstream
/// invalidation cascades automatically.
pub fn extend_hash_with_step_inputs(
    base_hash: &str,
    refs: &[(String, String, Option<String>)],
) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(base_hash.as_bytes());
    let mut sorted = refs.to_vec();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    for (key, as_file, hash) in sorted {
        h.update(b"\nstep_input:");
        h.update(key.as_bytes());
        h.update(b"::");
        h.update(as_file.as_bytes());
        h.update(b"::");
        h.update(hash.as_deref().unwrap_or("none").as_bytes());
    }
    let digest = h.finalize();
    let mut s = String::with_capacity(digest.len() * 2);
    for b in digest.iter() {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// v0.20.13 — cache-aware lookup. Returns the cached payload only
/// when the step is `done` AND the recorded `result_hash` matches
/// `input_hash`. Anything else (missing step, running step, stale
/// hash, failed step) returns None so the caller knows to actually
/// do the work.
///
/// Returned shape: `{result, documentIds, stepStatus, hitCacheAt}`.
pub async fn cache_hit_payload(
    pool: &SqlitePool,
    plan_id: i64,
    key: &str,
    expected_hash: &str,
) -> anyhow::Result<Option<serde_json::Value>> {
    let step = match get_step(pool, plan_id, key).await? {
        Some(s) => s,
        None => return Ok(None),
    };
    if step.status != "done" {
        return Ok(None);
    }
    let stored_hash = match step.result_hash.as_deref() {
        Some(h) => h,
        None => return Ok(None),
    };
    if stored_hash != expected_hash {
        tracing::info!(
            "plan cache miss: hash mismatch for step '{}' (stored vs expected)",
            key
        );
        return Ok(None);
    }
    let result_value: serde_json::Value = step
        .result_json
        .as_deref()
        .and_then(|j| serde_json::from_str(j).ok())
        .unwrap_or(serde_json::Value::Null);
    let doc_ids: Vec<i64> = step
        .document_ids
        .as_deref()
        .map(|raw| {
            raw.split(',')
                .filter_map(|x| x.trim().parse::<i64>().ok())
                .collect()
        })
        .unwrap_or_default();
    Ok(Some(serde_json::json!({
        "result": result_value,
        "documentIds": doc_ids,
        "stepStatus": step.status,
        "completedAt": step.completed_at,
        "fromCache": true,
    })))
}

/// v0.20.13 — record a step result keyed by the same input hash
/// that `cache_hit_payload` will check against. Always overwrites,
/// so re-running a step with different inputs updates the cache.
pub async fn record_step_with_hash(
    pool: &SqlitePool,
    plan_id: i64,
    key: &str,
    status: &str,
    result_json: &str,
    document_ids: &[i64],
    input_hash_str: &str,
    error: Option<&str>,
) -> anyhow::Result<()> {
    let docs_str = if document_ids.is_empty() {
        None
    } else {
        Some(
            document_ids
                .iter()
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(","),
        )
    };
    // Insert-or-update so a step record always exists after run.
    // The plan might not have declared this key up front — accept it
    // anyway (LLM might run ad-hoc steps).
    sqlx::query(
        "INSERT INTO plan_step
             (plan_id, key, purpose, status, result_json, document_ids,
              result_hash, error, started_at, completed_at)
         VALUES (?1, ?2, ?2, ?3, ?4, ?5, ?6, ?7, datetime('now'),
                 CASE WHEN ?3 IN ('done','failed','skipped') THEN datetime('now')
                      ELSE NULL END)
         ON CONFLICT(plan_id, key) DO UPDATE SET
             status       = excluded.status,
             result_json  = excluded.result_json,
             document_ids = excluded.document_ids,
             result_hash  = excluded.result_hash,
             error        = excluded.error,
             completed_at = CASE WHEN excluded.status IN ('done','failed','skipped')
                                  THEN datetime('now') ELSE completed_at END,
             updated_at   = datetime('now')",
    )
    .bind(plan_id)
    .bind(key)
    .bind(status)
    .bind(result_json)
    .bind(docs_str)
    .bind(input_hash_str)
    .bind(error)
    .execute(pool)
    .await?;
    Ok(())
}

// ---------- Tauri commands ----------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatePlanRequest {
    pub conversation_id: i64,
    pub goal: String,
    pub steps: Vec<PlanStepInput>,
}

#[tauri::command]
pub async fn plan_create_cmd(
    state: State<'_, AppState>,
    req: CreatePlanRequest,
) -> Result<i64, String> {
    create_plan(&state.db.pool, req.conversation_id, &req.goal, &req.steps)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn plan_status_cmd(
    state: State<'_, AppState>,
    plan_id: i64,
) -> Result<Vec<PlanStep>, String> {
    list_steps(&state.db.pool, plan_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn plan_active_cmd(
    state: State<'_, AppState>,
    conversation_id: i64,
) -> Result<Option<Plan>, String> {
    active_plan_for_conversation(&state.db.pool, conversation_id)
        .await
        .map_err(|e| e.to_string())
}
