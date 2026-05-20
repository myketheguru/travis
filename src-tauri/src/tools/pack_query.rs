//! `pack_query` — generic typed-table query tool for the LLM.
//!
//! Travis's bridge to read arbitrary rows from any enabled pack table
//! without a per-table tool. Workspace-clamped automatically.
//! Filter values are bound as parameters (no SQL injection). Filter
//! keys are validated against the table's FieldDef list — unknown
//! fields are rejected. Supports a small operator vocabulary: eq, ne,
//! lt, lte, gt, gte, like, ilike, in, isNull, isNotNull.
//!
//! Read-only. Never writes; never executes raw SQL from the LLM.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use tauri::Manager;

use crate::llm::ToolDef;
use crate::tools::{Tool, ToolContext};
use crate::AppState;

pub struct PackQueryTool;

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 500;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    pack_slug: String,
    table_slug: String,
    /// `{ field: value }` OR `{ field: { op, value } }`. Unknown fields
    /// are rejected. `value` can be a string, number, bool, or null.
    #[serde(default)]
    filters: serde_json::Map<String, Value>,
    /// Column to sort by (must be in the table's fields). Defaults to
    /// the table's `default_sort` or `id`.
    #[serde(default)]
    sort: Option<String>,
    /// "asc" or "desc". Defaults to the table's `default_sort_dir`.
    #[serde(default)]
    sort_dir: Option<String>,
    #[serde(default)]
    limit: Option<i64>,
}

#[async_trait]
impl Tool for PackQueryTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "pack_query".into(),
            description: "Read rows from any enabled pack's typed table. \
                Call pack_introspect first to discover what's queryable. \
                Filters: pass a map of field -> value for equality, or \
                field -> { op: 'lt'|'lte'|'gt'|'gte'|'ne'|'like'|'ilike'|\
                'in'|'isNull'|'isNotNull', value }. 'like' uses SQL % \
                wildcards literally; 'ilike' is case-insensitive (lowercased \
                comparison). 'in' takes a value array. Workspace-clamped \
                automatically — no need to pass workspaceId. Default limit \
                50, max 500. Returns { rows: [...], rowCount, table, \
                pack }."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "packSlug": { "type": "string", "description": "e.g. 'lead-to-empower'." },
                    "tableSlug": { "type": "string", "description": "e.g. 'contract', 'engagement', 'invoice'." },
                    "filters": {
                        "type": "object",
                        "description": "Map of field -> value (eq) OR field -> { op, value }. Unknown fields are rejected."
                    },
                    "sort": { "type": "string", "description": "Column to sort by." },
                    "sortDir": { "type": "string", "enum": ["asc", "desc"] },
                    "limit": { "type": "integer", "description": "Max rows to return. Default 50, max 500." }
                },
                "required": ["packSlug", "tableSlug"]
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: Input = serde_json::from_value(input)?;
        let state = ctx.app.state::<AppState>();

        // Resolve pack + table.
        let pack = state
            .enabled_packs
            .iter()
            .find(|p2| p2.slug() == p.pack_slug)
            .ok_or_else(|| {
                let names: Vec<&str> = state.enabled_packs.iter().map(|p2| p2.slug()).collect();
                anyhow::anyhow!(
                    "unknown pack \"{}\". Enabled packs: {}",
                    p.pack_slug,
                    names.join(", ")
                )
            })?;
        let table = pack
            .tables()
            .iter()
            .find(|t| t.slug == p.table_slug)
            .ok_or_else(|| {
                let names: Vec<&str> = pack.tables().iter().map(|t| t.slug).collect();
                anyhow::anyhow!(
                    "unknown table \"{}\" in pack \"{}\". Available: {}",
                    p.table_slug,
                    p.pack_slug,
                    names.join(", ")
                )
            })?;

        // Build SELECT clause from declared fields. Bracketing the SQL
        // identifier in quotes is safe because the field slug is from
        // the pack's declared TableDef, not user input.
        let columns = table
            .fields
            .iter()
            .map(|f| format!("\"{}\"", f.slug))
            .collect::<Vec<_>>()
            .join(", ");

        // Workspace clamp. Read the active workspace; for tables that
        // don't carry workspace_id, skip the clause.
        let workspace_id = state.workspace.read().await.active_id;
        let has_workspace = table.fields.iter().any(|f| f.slug == "workspace_id");

        let mut sql = format!("SELECT {columns} FROM \"{}\"", table.slug);
        let mut where_clauses: Vec<String> = Vec::new();
        let mut binds: Vec<BindValue> = Vec::new();

        if has_workspace {
            where_clauses.push(format!("workspace_id = ?{}", binds.len() + 1));
            binds.push(BindValue::Int(workspace_id));
        }

        // Validate + bind filters.
        for (raw_key, raw_val) in &p.filters {
            let key = raw_key.as_str();
            if !table.fields.iter().any(|f| f.slug == key) {
                anyhow::bail!(
                    "unknown field \"{}\" on table \"{}\". Valid fields: {}",
                    key,
                    p.table_slug,
                    table.fields.iter().map(|f| f.slug).collect::<Vec<_>>().join(", ")
                );
            }
            // Detect op-shape vs plain-value-shape.
            let (op, value) = if let Some(obj) = raw_val.as_object() {
                if let (Some(op_v), Some(val_v)) = (obj.get("op"), obj.get("value")) {
                    let op_str = op_v
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("filter op must be a string"))?;
                    (op_str.to_string(), val_v.clone())
                } else {
                    ("eq".to_string(), Value::Object(obj.clone()))
                }
            } else {
                ("eq".to_string(), raw_val.clone())
            };

            let clause = build_clause(key, &op, &value, &mut binds)?;
            where_clauses.push(clause);
        }

        if !where_clauses.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&where_clauses.join(" AND "));
        }

        // Sort.
        let sort_col = p
            .sort
            .as_deref()
            .or(table.list_view.default_sort)
            .unwrap_or("id");
        if !table.fields.iter().any(|f| f.slug == sort_col) {
            anyhow::bail!(
                "cannot sort by unknown field \"{}\"",
                sort_col
            );
        }
        let dir = match p.sort_dir.as_deref() {
            Some("asc") => "ASC",
            Some("desc") => "DESC",
            _ => match table.list_view.default_sort_dir {
                crate::packs::SortDir::Asc => "ASC",
                crate::packs::SortDir::Desc => "DESC",
            },
        };
        sql.push_str(&format!(" ORDER BY \"{sort_col}\" {dir}"));

        // Limit.
        let limit = p.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
        sql.push_str(&format!(" LIMIT {limit}"));

        // Execute.
        let mut q = sqlx::query(&sql);
        for b in &binds {
            q = match b {
                BindValue::Int(v) => q.bind(*v),
                BindValue::Float(v) => q.bind(*v),
                BindValue::Text(v) => q.bind(v.clone()),
                BindValue::Bool(v) => q.bind(*v),
            };
        }
        let rows = q.fetch_all(&ctx.db.pool).await?;

        // Serialize via the same row_to_json that auto-CRUD uses, so
        // shapes are consistent with the rest of the UI.
        let serialized: Vec<Value> = rows
            .iter()
            .map(|r| crate::packs_cmd::row_to_json(r, table.fields))
            .collect();

        Ok(json!({
            "rows": serialized,
            "rowCount": serialized.len(),
            "pack": p.pack_slug,
            "table": p.table_slug,
        })
        .to_string())
    }
}

/// Internal bind-value helper so we can heterogeneous-bind without
/// erasing types through serde_json::Value at execute time.
enum BindValue {
    Int(i64),
    Float(f64),
    Text(String),
    Bool(bool),
}

fn build_clause(field: &str, op: &str, value: &Value, binds: &mut Vec<BindValue>) -> anyhow::Result<String> {
    match op {
        "isNull" => Ok(format!("\"{field}\" IS NULL")),
        "isNotNull" => Ok(format!("\"{field}\" IS NOT NULL")),
        "in" => {
            let arr = value
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("'in' op requires array value"))?;
            if arr.is_empty() {
                anyhow::bail!("'in' op requires non-empty array");
            }
            let mut placeholders: Vec<String> = Vec::new();
            for v in arr {
                push_bind(v, binds)?;
                placeholders.push(format!("?{}", binds.len()));
            }
            Ok(format!("\"{field}\" IN ({})", placeholders.join(", ")))
        }
        "eq" | "ne" | "lt" | "lte" | "gt" | "gte" | "like" => {
            push_bind(value, binds)?;
            let idx = binds.len();
            let sql_op = match op {
                "eq" => "=",
                "ne" => "!=",
                "lt" => "<",
                "lte" => "<=",
                "gt" => ">",
                "gte" => ">=",
                "like" => "LIKE",
                _ => unreachable!(),
            };
            Ok(format!("\"{field}\" {sql_op} ?{}", idx))
        }
        "ilike" => {
            // Case-insensitive: compare LOWER on both sides.
            let s = value
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("'ilike' op requires string value"))?;
            binds.push(BindValue::Text(s.to_lowercase()));
            Ok(format!("LOWER(\"{field}\") LIKE ?{}", binds.len()))
        }
        other => anyhow::bail!(
            "unknown op \"{}\" — valid: eq, ne, lt, lte, gt, gte, like, ilike, in, isNull, isNotNull",
            other
        ),
    }
}

fn push_bind(value: &Value, binds: &mut Vec<BindValue>) -> anyhow::Result<()> {
    match value {
        Value::Null => binds.push(BindValue::Text(String::new())),
        Value::Bool(b) => binds.push(BindValue::Bool(*b)),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                binds.push(BindValue::Int(i));
            } else if let Some(f) = n.as_f64() {
                binds.push(BindValue::Float(f));
            } else {
                anyhow::bail!("unsupported numeric value");
            }
        }
        Value::String(s) => binds.push(BindValue::Text(s.clone())),
        other => anyhow::bail!("unsupported filter value: {other}"),
    }
    Ok(())
}

// Required to make sqlx::Row::try_get accessible — the row_to_json
// path uses it indirectly. Keeps trait imports localized.
#[allow(dead_code)]
fn _row_trait_assert<R: Row>() {}
