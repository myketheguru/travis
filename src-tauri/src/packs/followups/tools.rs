//! Follow-ups pack LLM tools.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

use crate::llm::ToolDef;
use crate::tools::{Tool, ToolContext};

pub struct LogFollowupTool;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LogFollowupInput {
    title: String,
    #[serde(default)]
    person: Option<String>,
    #[serde(default)]
    due_by: Option<String>,
    #[serde(default)]
    notes: Option<String>,
    #[serde(default)]
    source: Option<String>,
}

#[async_trait]
impl Tool for LogFollowupTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "log_followup".into(),
            description: "Record a commitment the user made — 'I'll send X', 'let me get back to you', 'I'll follow up next week'. Auto-capture whenever the user makes a promise; don't wait for confirmation.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "title": { "type": "string", "description": "The commitment in imperative form — 'send Sarah the Q3 deck'." },
                    "person": { "type": "string", "description": "Who they owe this to." },
                    "dueBy": { "type": "string", "description": "Optional ISO date the user mentioned." },
                    "notes": { "type": "string" },
                    "source": { "type": "string", "description": "'user' when directly stated, 'ambient' when caught in a meeting, 'inbox' from email." }
                },
                "required": ["title"]
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: LogFollowupInput = serde_json::from_value(input)?;
        let row = sqlx::query(
            "INSERT INTO followup (title, person, due_by, notes, source)
             VALUES (?1, ?2, ?3, ?4, ?5) RETURNING id",
        )
        .bind(&p.title)
        .bind(&p.person)
        .bind(&p.due_by)
        .bind(&p.notes)
        .bind(&p.source)
        .fetch_one(&ctx.db.pool)
        .await?;
        let id: i64 = row.try_get(0)?;
        Ok(format!("Logged follow-up '{}' (id={id}).", p.title))
    }
}

pub struct ListFollowupsTool;

#[derive(Deserialize)]
struct ListFollowupsInput {
    #[serde(default)]
    person: Option<String>,
    #[serde(default = "default_open")]
    only_open: bool,
}
fn default_open() -> bool { true }

#[async_trait]
impl Tool for ListFollowupsTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "list_followups".into(),
            description: "List open follow-ups. Optionally filter by person. Powers 'who did I promise to email this week?' and 'anything open with Sarah?'.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "person": { "type": "string", "description": "Optional filter by person name." },
                    "onlyOpen": { "type": "boolean", "description": "Default true — hide done/dropped." }
                }
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: ListFollowupsInput = serde_json::from_value(input).unwrap_or(ListFollowupsInput { person: None, only_open: true });
        let mut q = String::from("SELECT id, title, person, due_by, status, created_at FROM followup WHERE 1=1");
        if p.only_open { q.push_str(" AND status = 'open'"); }
        if let Some(person) = &p.person {
            q.push_str(&format!(" AND person LIKE '%{}%' COLLATE NOCASE", person.replace('\'', "''")));
        }
        q.push_str(" ORDER BY due_by ASC NULLS LAST, created_at DESC LIMIT 20");
        let rows = sqlx::query(&q).fetch_all(&ctx.db.pool).await?;
        if rows.is_empty() {
            return Ok("No open follow-ups.".into());
        }
        let mut out = String::from("Open follow-ups:\n");
        for r in rows {
            let id: i64 = r.try_get("id").unwrap_or(0);
            let title: String = r.try_get("title").unwrap_or_default();
            let person: Option<String> = r.try_get("person").ok();
            let due: Option<String> = r.try_get("due_by").ok();
            out.push_str(&format!("· [{id}] {title}"));
            if let Some(v) = person { out.push_str(&format!(" · {v}")); }
            if let Some(v) = due { out.push_str(&format!(" · due {v}")); }
            out.push('\n');
        }
        Ok(out)
    }
}

pub struct CompleteFollowupTool;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompleteFollowupInput {
    id: i64,
    #[serde(default)]
    status: Option<String>,
}

#[async_trait]
impl Tool for CompleteFollowupTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "complete_followup".into(),
            description: "Mark a follow-up done or dropped. Use when the user says they finished / sent / took care of it.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "integer" },
                    "status": { "type": "string", "enum": ["done", "dropped"] }
                },
                "required": ["id"]
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: CompleteFollowupInput = serde_json::from_value(input)?;
        let status = p.status.unwrap_or_else(|| "done".to_string());
        sqlx::query(
            "UPDATE followup SET status = ?2, completed_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')
             WHERE id = ?1",
        )
        .bind(p.id)
        .bind(&status)
        .execute(&ctx.db.pool)
        .await?;
        Ok(format!("Follow-up {} marked {status}.", p.id))
    }
}
