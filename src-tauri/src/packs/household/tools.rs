//! Household pack LLM tools.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

use crate::llm::ToolDef;
use crate::tools::{Tool, ToolContext};

pub struct AddToGroceryTool;

#[derive(Deserialize)]
struct AddToGroceryInput {
    items: Vec<GroceryEntry>,
}

#[derive(Deserialize)]
struct GroceryEntry {
    name: String,
    #[serde(default)]
    quantity: Option<String>,
    #[serde(default)]
    category: Option<String>,
}

#[async_trait]
impl Tool for AddToGroceryTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "add_to_grocery".into(),
            description: "Add items to the grocery list. Handles multiple at once — 'add milk, eggs, cardamom' should call this once with all three.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "items": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "name": { "type": "string" },
                                "quantity": { "type": "string" },
                                "category": { "type": "string", "description": "produce, dairy, pantry, household" }
                            },
                            "required": ["name"]
                        }
                    }
                },
                "required": ["items"]
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: AddToGroceryInput = serde_json::from_value(input)?;
        let mut added = Vec::new();
        for item in &p.items {
            sqlx::query("INSERT INTO grocery_item (name, quantity, category) VALUES (?1, ?2, ?3)")
                .bind(&item.name)
                .bind(&item.quantity)
                .bind(&item.category)
                .execute(&ctx.db.pool)
                .await?;
            added.push(item.name.clone());
        }
        Ok(format!("Added {} items to grocery: {}", added.len(), added.join(", ")))
    }
}

pub struct ListGroceryTool;

#[async_trait]
impl Tool for ListGroceryTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "list_grocery".into(),
            description: "List all items on the current grocery list (unpurchased).".into(),
            input_schema: json!({ "type": "object", "properties": {} }),
        }
    }
    async fn execute(&self, ctx: &ToolContext, _input: Value) -> anyhow::Result<String> {
        let rows = sqlx::query(
            "SELECT id, name, quantity, category FROM grocery_item
             WHERE purchased_at IS NULL ORDER BY category, created_at",
        )
        .fetch_all(&ctx.db.pool)
        .await?;
        if rows.is_empty() {
            return Ok("Grocery list is empty.".into());
        }
        let mut out = String::from("Grocery list:\n");
        for r in rows {
            let id: i64 = r.try_get("id").unwrap_or(0);
            let name: String = r.try_get("name").unwrap_or_default();
            let qty: Option<String> = r.try_get("quantity").ok();
            let cat: Option<String> = r.try_get("category").ok();
            out.push_str(&format!("· [{id}] {name}"));
            if let Some(v) = qty { out.push_str(&format!(" · {v}")); }
            if let Some(v) = cat { out.push_str(&format!(" · {v}")); }
            out.push('\n');
        }
        Ok(out)
    }
}

pub struct MarkGroceryPurchasedTool;

#[derive(Deserialize)]
struct MarkPurchasedInput {
    ids: Vec<i64>,
}

#[async_trait]
impl Tool for MarkGroceryPurchasedTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "mark_grocery_purchased".into(),
            description: "Mark grocery items as purchased by id. Use when the user says they bought them / done shopping.".into(),
            input_schema: json!({
                "type": "object",
                "properties": { "ids": { "type": "array", "items": { "type": "integer" } } },
                "required": ["ids"]
            }),
        }
    }
    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: MarkPurchasedInput = serde_json::from_value(input)?;
        for id in &p.ids {
            sqlx::query("UPDATE grocery_item SET purchased_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = ?1")
                .bind(id)
                .execute(&ctx.db.pool)
                .await?;
        }
        Ok(format!("Marked {} items purchased.", p.ids.len()))
    }
}

pub struct LogChoreTool;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LogChoreInput {
    name: String,
    #[serde(default)]
    cadence: Option<String>,
    #[serde(default)]
    assigned_to: Option<String>,
    #[serde(default)]
    done_now: bool,
}

#[async_trait]
impl Tool for LogChoreTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "log_chore".into(),
            description: "Add or update a household chore. Set doneNow=true to also stamp last_done_at.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string" },
                    "cadence": { "type": "string", "enum": ["daily", "weekly", "monthly", "as-needed"] },
                    "assignedTo": { "type": "string" },
                    "doneNow": { "type": "boolean" }
                },
                "required": ["name"]
            }),
        }
    }
    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: LogChoreInput = serde_json::from_value(input)?;
        let existing = sqlx::query("SELECT id FROM chore WHERE name = ?1 LIMIT 1")
            .bind(&p.name)
            .fetch_optional(&ctx.db.pool)
            .await?;
        let id: i64 = if let Some(row) = existing {
            let id: i64 = row.try_get(0)?;
            let last_done = if p.done_now { Some("strftime('%Y-%m-%dT%H:%M:%fZ','now')") } else { None };
            let mut q = String::from("UPDATE chore SET cadence = COALESCE(?2, cadence), assigned_to = COALESCE(?3, assigned_to)");
            if last_done.is_some() { q.push_str(", last_done_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')"); }
            q.push_str(" WHERE id = ?1");
            sqlx::query(&q)
                .bind(id).bind(&p.cadence).bind(&p.assigned_to)
                .execute(&ctx.db.pool).await?;
            id
        } else {
            let last_done_val = if p.done_now { Some("now".to_string()) } else { None };
            let row = sqlx::query(
                "INSERT INTO chore (name, cadence, assigned_to, last_done_at) VALUES (?1, ?2, ?3, CASE WHEN ?4 IS NULL THEN NULL ELSE strftime('%Y-%m-%dT%H:%M:%fZ','now') END) RETURNING id",
            )
            .bind(&p.name).bind(&p.cadence).bind(&p.assigned_to).bind(&last_done_val)
            .fetch_one(&ctx.db.pool).await?;
            row.try_get(0)?
        };
        Ok(format!("Logged chore '{}' (id={id}).", p.name))
    }
}
