//! People pack LLM tools.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

use crate::llm::ToolDef;
use crate::tools::{Tool, ToolContext};

pub struct AddContactTool;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddContactInput {
    display_name: String,
    #[serde(default)]
    relationship: Option<String>,
    #[serde(default)]
    organization: Option<String>,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    phone: Option<String>,
    #[serde(default)]
    birthday: Option<String>,
    #[serde(default)]
    notes: Option<String>,
}

#[async_trait]
impl Tool for AddContactTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "add_contact".into(),
            description: "Add a person to the user's contacts. Use when the user mentions someone new by name with any identifying detail (role, employer, relationship). If the name already exists, this UPDATES their record.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "displayName": { "type": "string" },
                    "relationship": { "type": "string", "description": "friend, family, coworker, client, partner, other" },
                    "organization": { "type": "string" },
                    "email": { "type": "string" },
                    "phone": { "type": "string" },
                    "birthday": { "type": "string", "description": "ISO date; year optional." },
                    "notes": { "type": "string" }
                },
                "required": ["displayName"]
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: AddContactInput = serde_json::from_value(input)?;
        // Upsert by display_name
        let existing = sqlx::query("SELECT id FROM contact WHERE display_name = ?1 LIMIT 1")
            .bind(&p.display_name)
            .fetch_optional(&ctx.db.pool)
            .await?;
        let id: i64 = if let Some(row) = existing {
            let id: i64 = row.try_get(0)?;
            sqlx::query(
                "UPDATE contact SET
                    relationship = COALESCE(?2, relationship),
                    organization = COALESCE(?3, organization),
                    email = COALESCE(?4, email),
                    phone = COALESCE(?5, phone),
                    birthday = COALESCE(?6, birthday),
                    notes = COALESCE(?7, notes),
                    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')
                 WHERE id = ?1",
            )
            .bind(id)
            .bind(&p.relationship)
            .bind(&p.organization)
            .bind(&p.email)
            .bind(&p.phone)
            .bind(&p.birthday)
            .bind(&p.notes)
            .execute(&ctx.db.pool)
            .await?;
            id
        } else {
            let row = sqlx::query(
                "INSERT INTO contact (display_name, relationship, organization, email, phone, birthday, notes)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) RETURNING id",
            )
            .bind(&p.display_name)
            .bind(&p.relationship)
            .bind(&p.organization)
            .bind(&p.email)
            .bind(&p.phone)
            .bind(&p.birthday)
            .bind(&p.notes)
            .fetch_one(&ctx.db.pool)
            .await?;
            row.try_get(0)?
        };
        Ok(format!("Saved {} (id={id}).", p.display_name))
    }
}

pub struct FindContactTool;

#[derive(Deserialize)]
struct FindContactInput {
    query: String,
}

#[async_trait]
impl Tool for FindContactTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "find_contact".into(),
            description: "Look up a person by name or partial match. Returns their record: relationship, organization, email, phone, birthday, notes, when you last contacted them.".into(),
            input_schema: json!({
                "type": "object",
                "properties": { "query": { "type": "string" } },
                "required": ["query"]
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: FindContactInput = serde_json::from_value(input)?;
        let like = format!("%{}%", p.query);
        let rows = sqlx::query(
            "SELECT display_name, relationship, organization, email, phone, birthday, notes, last_contact_at
             FROM contact
             WHERE display_name LIKE ?1 COLLATE NOCASE
             ORDER BY last_contact_at DESC NULLS LAST
             LIMIT 5",
        )
        .bind(&like)
        .fetch_all(&ctx.db.pool)
        .await?;
        if rows.is_empty() {
            return Ok(format!("No contact matched '{}'.", p.query));
        }
        let mut out = String::new();
        for r in rows {
            let name: String = r.try_get("display_name").unwrap_or_default();
            let rel: Option<String> = r.try_get("relationship").ok();
            let org: Option<String> = r.try_get("organization").ok();
            let email: Option<String> = r.try_get("email").ok();
            let phone: Option<String> = r.try_get("phone").ok();
            let bday: Option<String> = r.try_get("birthday").ok();
            let notes: Option<String> = r.try_get("notes").ok();
            out.push_str(&format!("· {name}"));
            if let Some(v) = rel { out.push_str(&format!(" · {v}")); }
            if let Some(v) = org { out.push_str(&format!(" · {v}")); }
            if let Some(v) = email { out.push_str(&format!(" · {v}")); }
            if let Some(v) = phone { out.push_str(&format!(" · {v}")); }
            if let Some(v) = bday { out.push_str(&format!(" · birthday {v}")); }
            if let Some(v) = notes {
                let snippet: String = v.chars().take(140).collect();
                out.push_str(&format!("\n  notes: {snippet}"));
            }
            out.push('\n');
        }
        Ok(out)
    }
}

pub struct LogContactTool;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LogContactInput {
    display_name: String,
}

#[async_trait]
impl Tool for LogContactTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "log_contact_touch".into(),
            description: "Record that the user just talked to / emailed / met a person. Bumps last_contact_at. Use when the user says they saw / called / emailed someone.".into(),
            input_schema: json!({
                "type": "object",
                "properties": { "displayName": { "type": "string" } },
                "required": ["displayName"]
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: LogContactInput = serde_json::from_value(input)?;
        let n = sqlx::query(
            "UPDATE contact SET last_contact_at = strftime('%Y-%m-%dT%H:%M:%fZ','now'),
                 updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')
             WHERE display_name = ?1",
        )
        .bind(&p.display_name)
        .execute(&ctx.db.pool)
        .await?
        .rows_affected();
        if n == 0 {
            Ok(format!("No contact named '{}' — use add_contact first.", p.display_name))
        } else {
            Ok(format!("Logged contact touch with {}.", p.display_name))
        }
    }
}
