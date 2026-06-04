//! `lte_find_contract` — chat-first contract resolution.
//!
//! Read-only. Returns ranked contract matches with ceiling burn,
//! last activity. The LLM uses this BEFORE proposing `create_contract`
//! (an action that needs confirmation — contracts commit to a
//! relationship).
//!
//! As of pack v0.7.0 (contract+engagement collapse), this queries the
//! `engagement` table directly — engagement IS the contract record
//! now. Falls back to also include rows in the legacy `contract`
//! table that don't yet have a corresponding engagement (those get
//! synthesised by migration 0005, but defensive in case any slipped).
//!
//! Ranking: active > draft > expired > terminated; within each status,
//! by recency of activity DESC then by ceiling-remaining DESC.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::Manager;

use crate::llm::ToolDef;
use crate::tools::{Tool, ToolContext};
use crate::AppState;

pub struct FindContractTool;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    /// Free-form query — contract ref, counterparty, or solicitation
    /// substring. If omitted, returns active contracts in default order.
    #[serde(default)]
    query: Option<String>,
    /// Optional status filter. Default: any non-archived.
    #[serde(default)]
    status: Option<String>,
}

#[async_trait]
impl Tool for FindContractTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "lte_find_contract".into(),
            description: "Find an L2E master contract by ref, counterparty, \
                or parent solicitation substring. Returns up to 5 ranked \
                matches with status, term_end, ceiling, invoiced total, \
                engagement count. Use BEFORE proposing a new contract — \
                the LLM should check for an existing one first, present the \
                top match if confident, or list 2-3 if ambiguous. Status \
                priority: active > draft > expired > terminated > archived. \
                Query omitted = all active contracts."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Ref, counterparty, or solicitation fragment. Optional." },
                    "status": { "type": "string", "enum": ["draft","active","expired","terminated","archived"], "description": "Optional status filter." }
                }
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: Input = serde_json::from_value(input)?;
        let state = ctx.app.state::<AppState>();
        let workspace_id = state.workspace.read().await.active_id;

        let q = p.query.as_deref().unwrap_or("").trim().to_lowercase();
        let like = format!("%{q}%");

        #[derive(sqlx::FromRow)]
        struct Row {
            id: i64,
            ref_: Option<String>,
            name: String,
            school_name: Option<String>,
            counterparty: Option<String>,
            parent_solicitation: Option<String>,
            status: String,
            term_end: Option<String>,
            ceiling_cents: i64,
            invoiced_cents: i64,
            last_activity: Option<String>,
        }

        let rows: Vec<Row> = sqlx::query_as(
            "SELECT e.id, e.ref AS ref_, e.name,
                    (SELECT s.name FROM school s WHERE s.id = e.school_id) AS school_name,
                    e.counterparty, e.parent_solicitation,
                    e.contract_status AS status, e.term_end, e.ceiling_cents,
                    COALESCE((SELECT SUM(i.amount_cents) FROM invoice i
                                WHERE i.engagement_id = e.id AND i.status != 'void'), 0)
                        AS invoiced_cents,
                    e.updated_at AS last_activity
             FROM engagement e
             WHERE e.workspace_id = ?1
               AND (?2 = ''
                    OR LOWER(COALESCE(e.ref,'')) LIKE ?3
                    OR LOWER(COALESCE(e.counterparty,'')) LIKE ?3
                    OR LOWER(COALESCE(e.parent_solicitation,'')) LIKE ?3
                    OR LOWER(e.name) LIKE ?3
                    OR LOWER(COALESCE(
                        (SELECT s.name FROM school s WHERE s.id = e.school_id), '')) LIKE ?3)
               AND (?4 IS NULL OR e.contract_status = ?4)
             ORDER BY
               CASE e.contract_status
                 WHEN 'active' THEN 0
                 WHEN 'draft' THEN 1
                 WHEN 'expired' THEN 2
                 WHEN 'terminated' THEN 3
                 ELSE 4
               END,
               e.updated_at DESC,
               (e.ceiling_cents - COALESCE((SELECT SUM(i.amount_cents) FROM invoice i
                                              WHERE i.engagement_id = e.id AND i.status != 'void'), 0)) DESC,
               e.name ASC
             LIMIT 5",
        )
        .bind(workspace_id)
        .bind(&q)
        .bind(&like)
        .bind(p.status.as_deref())
        .fetch_all(&ctx.db.pool)
        .await?;

        let candidates: Vec<Value> = rows
            .iter()
            .map(|r| {
                let remaining = (r.ceiling_cents - r.invoiced_cents).max(0);
                let burn_pct = if r.ceiling_cents > 0 {
                    (r.invoiced_cents as f64 / r.ceiling_cents as f64 * 100.0).round() as i64
                } else {
                    0
                };
                json!({
                    "id": r.id,
                    "ref": r.ref_,
                    "name": r.name,
                    "schoolName": r.school_name,
                    "counterparty": r.counterparty,
                    "parentSolicitation": r.parent_solicitation,
                    "status": r.status,
                    "termEnd": r.term_end,
                    "ceilingCents": r.ceiling_cents,
                    "invoicedCents": r.invoiced_cents,
                    "remainingCents": remaining,
                    "burnPct": burn_pct,
                    "lastActivity": r.last_activity,
                })
            })
            .collect();

        let result = if rows.is_empty() {
            "no_matches"
        } else if rows.len() == 1 {
            "single"
        } else {
            "multiple"
        };

        Ok(json!({
            "result": result,
            "candidates": candidates,
            "rationale": "ranked by status (active first), recency, remaining ceiling"
        })
        .to_string())
    }
}
