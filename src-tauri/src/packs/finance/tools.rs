//! Finance pack LLM tools.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

use crate::llm::ToolDef;
use crate::tools::{Tool, ToolContext};

pub struct LogBillTool;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LogBillInput {
    name: String,
    #[serde(default)]
    amount_cents: Option<i64>,
    #[serde(default)]
    cadence: Option<String>,
    #[serde(default)]
    next_due_at: Option<String>,
    #[serde(default)]
    autopay: Option<bool>,
    #[serde(default)]
    notes: Option<String>,
}

#[async_trait]
impl Tool for LogBillTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "log_bill".into(),
            description: "Add or update a recurring bill (electric, phone, rent, insurance). Amounts in cents.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string" },
                    "amountCents": { "type": "integer", "description": "Amount in cents. $150 = 15000." },
                    "cadence": { "type": "string", "enum": ["monthly", "quarterly", "yearly", "one-time"] },
                    "nextDueAt": { "type": "string", "description": "ISO date." },
                    "autopay": { "type": "boolean" },
                    "notes": { "type": "string" }
                },
                "required": ["name"]
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: LogBillInput = serde_json::from_value(input)?;
        let existing = sqlx::query("SELECT id FROM bill WHERE name = ?1 LIMIT 1")
            .bind(&p.name).fetch_optional(&ctx.db.pool).await?;
        let autopay = p.autopay.unwrap_or(false) as i64;
        let id: i64 = if let Some(row) = existing {
            let id: i64 = row.try_get(0)?;
            sqlx::query(
                "UPDATE bill SET amount_cents = COALESCE(?2, amount_cents),
                    cadence = COALESCE(?3, cadence),
                    next_due_at = COALESCE(?4, next_due_at),
                    autopay = ?5,
                    notes = COALESCE(?6, notes)
                 WHERE id = ?1",
            )
            .bind(id).bind(p.amount_cents).bind(&p.cadence).bind(&p.next_due_at).bind(autopay).bind(&p.notes)
            .execute(&ctx.db.pool).await?;
            id
        } else {
            let row = sqlx::query(
                "INSERT INTO bill (name, amount_cents, cadence, next_due_at, autopay, notes)
                 VALUES (?1, ?2, COALESCE(?3, 'monthly'), ?4, ?5, ?6) RETURNING id",
            )
            .bind(&p.name).bind(p.amount_cents).bind(&p.cadence).bind(&p.next_due_at).bind(autopay).bind(&p.notes)
            .fetch_one(&ctx.db.pool).await?;
            row.try_get(0)?
        };
        Ok(format!("Logged bill '{}' (id={id}).", p.name))
    }
}

pub struct ListBillsTool;

#[async_trait]
impl Tool for ListBillsTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "list_bills".into(),
            description: "List recurring bills, sorted by next due date. Powers 'what bills are coming up?' / 'am I over-paying?'.".into(),
            input_schema: json!({ "type": "object", "properties": {} }),
        }
    }
    async fn execute(&self, ctx: &ToolContext, _input: Value) -> anyhow::Result<String> {
        let rows = sqlx::query(
            "SELECT id, name, amount_cents, cadence, next_due_at, autopay FROM bill
             ORDER BY next_due_at ASC NULLS LAST LIMIT 50",
        ).fetch_all(&ctx.db.pool).await?;
        if rows.is_empty() { return Ok("No bills logged yet.".into()); }
        let mut out = String::from("Bills:\n");
        for r in rows {
            let id: i64 = r.try_get("id").unwrap_or(0);
            let name: String = r.try_get("name").unwrap_or_default();
            let amount: Option<i64> = r.try_get("amount_cents").ok();
            let cadence: String = r.try_get("cadence").unwrap_or_default();
            let due: Option<String> = r.try_get("next_due_at").ok();
            let autopay: i64 = r.try_get("autopay").unwrap_or(0);
            out.push_str(&format!("· [{id}] {name}"));
            if let Some(v) = amount { out.push_str(&format!(" · ${:.2}", v as f64 / 100.0)); }
            out.push_str(&format!(" · {cadence}"));
            if let Some(v) = due { out.push_str(&format!(" · due {v}")); }
            if autopay == 1 { out.push_str(" · autopay"); }
            out.push('\n');
        }
        Ok(out)
    }
}

pub struct LogSubscriptionTool;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LogSubscriptionInput {
    name: String,
    #[serde(default)]
    amount_cents: Option<i64>,
    #[serde(default)]
    cadence: Option<String>,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    next_renewal_at: Option<String>,
}

#[async_trait]
impl Tool for LogSubscriptionTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "log_subscription".into(),
            description: "Add or update a subscription (Netflix, Adobe, gym). Amounts in cents.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string" },
                    "amountCents": { "type": "integer" },
                    "cadence": { "type": "string", "enum": ["monthly", "quarterly", "yearly"] },
                    "category": { "type": "string" },
                    "status": { "type": "string", "enum": ["active", "cancelled", "paused"] },
                    "nextRenewalAt": { "type": "string" }
                },
                "required": ["name"]
            }),
        }
    }
    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: LogSubscriptionInput = serde_json::from_value(input)?;
        let existing = sqlx::query("SELECT id FROM subscription WHERE name = ?1 LIMIT 1")
            .bind(&p.name).fetch_optional(&ctx.db.pool).await?;
        let id: i64 = if let Some(row) = existing {
            let id: i64 = row.try_get(0)?;
            sqlx::query(
                "UPDATE subscription SET
                    amount_cents = COALESCE(?2, amount_cents),
                    cadence = COALESCE(?3, cadence),
                    category = COALESCE(?4, category),
                    status = COALESCE(?5, status),
                    next_renewal_at = COALESCE(?6, next_renewal_at)
                 WHERE id = ?1",
            )
            .bind(id).bind(p.amount_cents).bind(&p.cadence).bind(&p.category).bind(&p.status).bind(&p.next_renewal_at)
            .execute(&ctx.db.pool).await?;
            id
        } else {
            let row = sqlx::query(
                "INSERT INTO subscription (name, amount_cents, cadence, category, status, next_renewal_at)
                 VALUES (?1, ?2, COALESCE(?3, 'monthly'), ?4, COALESCE(?5, 'active'), ?6) RETURNING id",
            )
            .bind(&p.name).bind(p.amount_cents).bind(&p.cadence).bind(&p.category).bind(&p.status).bind(&p.next_renewal_at)
            .fetch_one(&ctx.db.pool).await?;
            row.try_get(0)?
        };
        Ok(format!("Logged subscription '{}' (id={id}).", p.name))
    }
}

pub struct ListSubscriptionsTool;

#[async_trait]
impl Tool for ListSubscriptionsTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "list_subscriptions".into(),
            description: "List active subscriptions with monthly cost. Powers 'what am I paying for?' and 'am I doubling up?'.".into(),
            input_schema: json!({ "type": "object", "properties": {} }),
        }
    }
    async fn execute(&self, ctx: &ToolContext, _input: Value) -> anyhow::Result<String> {
        let rows = sqlx::query(
            "SELECT id, name, amount_cents, cadence, category, next_renewal_at FROM subscription
             WHERE status = 'active' ORDER BY amount_cents DESC NULLS LAST LIMIT 100",
        ).fetch_all(&ctx.db.pool).await?;
        if rows.is_empty() { return Ok("No active subscriptions logged.".into()); }
        let mut out = String::from("Active subscriptions:\n");
        let mut monthly_total: i64 = 0;
        for r in rows {
            let id: i64 = r.try_get("id").unwrap_or(0);
            let name: String = r.try_get("name").unwrap_or_default();
            let amount: Option<i64> = r.try_get("amount_cents").ok();
            let cadence: String = r.try_get("cadence").unwrap_or_default();
            let cat: Option<String> = r.try_get("category").ok();
            let renews: Option<String> = r.try_get("next_renewal_at").ok();
            out.push_str(&format!("· [{id}] {name}"));
            if let Some(v) = amount {
                out.push_str(&format!(" · ${:.2}/{cadence}", v as f64 / 100.0));
                let monthly = match cadence.as_str() {
                    "yearly" => v / 12,
                    "quarterly" => v / 3,
                    _ => v,
                };
                monthly_total += monthly;
            }
            if let Some(v) = cat { out.push_str(&format!(" · {v}")); }
            if let Some(v) = renews { out.push_str(&format!(" · renews {v}")); }
            out.push('\n');
        }
        out.push_str(&format!("\nTotal ~${:.2}/mo", monthly_total as f64 / 100.0));
        Ok(out)
    }
}
