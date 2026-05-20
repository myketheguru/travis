//! `lte_find_contract` — chat-first contract resolution.
//!
//! Read-only. Returns ranked contract matches with ceiling burn,
//! engagement count, last activity. The LLM uses this BEFORE proposing
//! `create_contract` (an action that needs confirmation — contracts
//! commit to a relationship).
//!
//! Ranking: active > expiring soon > expired > terminated; within each
//! status, by recency of activity DESC then by ceiling-remaining DESC.

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
            ref_: String,
            name: Option<String>,
            counterparty: Option<String>,
            parent_solicitation: Option<String>,
            status: String,
            term_end: Option<String>,
            ceiling_cents: i64,
            invoiced_cents: i64,
            engagement_count: i64,
            last_activity: Option<String>,
        }

        let rows: Vec<Row> = sqlx::query_as(
            "SELECT c.id, c.ref AS ref_, c.name, c.counterparty, c.parent_solicitation,
                    c.status, c.term_end, c.ceiling_cents,
                    COALESCE((SELECT SUM(i.amount_cents) FROM invoice i
                                JOIN engagement e ON e.id = i.engagement_id
                                WHERE e.contract_id = c.id AND i.status != 'void'), 0) AS invoiced_cents,
                    (SELECT COUNT(*) FROM engagement e WHERE e.contract_id = c.id) AS engagement_count,
                    (SELECT MAX(updated_at) FROM engagement e WHERE e.contract_id = c.id) AS last_activity
             FROM contract c
             WHERE c.workspace_id = ?1
               AND (?2 = '' OR LOWER(c.ref) LIKE ?3 OR LOWER(COALESCE(c.counterparty,'')) LIKE ?3 OR LOWER(COALESCE(c.parent_solicitation,'')) LIKE ?3 OR LOWER(COALESCE(c.name,'')) LIKE ?3)
               AND (?4 IS NULL OR c.status = ?4)
             ORDER BY
               CASE c.status
                 WHEN 'active' THEN 0
                 WHEN 'draft' THEN 1
                 WHEN 'expired' THEN 2
                 WHEN 'terminated' THEN 3
                 ELSE 4
               END,
               last_activity DESC NULLS LAST,
               (c.ceiling_cents - COALESCE((SELECT SUM(i.amount_cents) FROM invoice i
                                              JOIN engagement e ON e.id = i.engagement_id
                                              WHERE e.contract_id = c.id AND i.status != 'void'), 0)) DESC,
               c.ref ASC
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
                    "counterparty": r.counterparty,
                    "parentSolicitation": r.parent_solicitation,
                    "status": r.status,
                    "termEnd": r.term_end,
                    "ceilingCents": r.ceiling_cents,
                    "invoicedCents": r.invoiced_cents,
                    "remainingCents": remaining,
                    "burnPct": burn_pct,
                    "engagementCount": r.engagement_count,
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
