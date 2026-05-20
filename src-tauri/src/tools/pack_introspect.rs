//! `pack_introspect` — generic schema-discovery tool for the LLM.
//!
//! Returns every enabled pack's metadata: pack slug + name + version,
//! then for each table: slug, display name, fields with type + required.
//! Pairs with `pack_query` to let Travis answer arbitrary "look up X"
//! questions without per-table tools (e.g. the LTE-specific find_school
//! / find_contract are still useful for ranked search, but if a future
//! pack adds a `vendor` table, `pack_introspect` discovers it
//! automatically and `pack_query` queries it).
//!
//! Read-only. No workspace clamp needed — schema is global.

use async_trait::async_trait;
use serde_json::{json, Value};
use tauri::Manager;

use crate::llm::ToolDef;
use crate::tools::{Tool, ToolContext};
use crate::AppState;

pub struct PackIntrospectTool;

#[async_trait]
impl Tool for PackIntrospectTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "pack_introspect".into(),
            description: "List every enabled pack's tables and field schemas. \
                Use to discover what's queryable when the user asks a \
                question whose answer might live in a pack table you don't \
                already know about. Returns: { packs: [{ slug, name, \
                version, tables: [{ slug, displayName, primary, fields: \
                [{ slug, label, type, required, help }] }] }] }. Pair \
                with pack_query to actually read rows."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, _input: Value) -> anyhow::Result<String> {
        let state = ctx.app.state::<AppState>();
        let mut packs_out = Vec::new();
        for pack in &state.enabled_packs {
            let mut tables_out = Vec::new();
            for t in pack.tables() {
                let fields_out: Vec<Value> = t
                    .fields
                    .iter()
                    .map(|f| {
                        json!({
                            "slug": f.slug,
                            "label": f.label,
                            "type": field_type_label(&f.field_type),
                            "required": f.required,
                            "help": f.help,
                        })
                    })
                    .collect();
                tables_out.push(json!({
                    "slug": t.slug,
                    "displayName": t.display_name,
                    "primary": t.primary,
                    "displayField": t.display_field,
                    "fields": fields_out,
                }));
            }
            packs_out.push(json!({
                "slug": pack.slug(),
                "name": pack.name(),
                "version": pack.version(),
                "tables": tables_out,
            }));
        }
        Ok(json!({ "packs": packs_out }).to_string())
    }
}

fn field_type_label(t: &crate::packs::FieldType) -> String {
    use crate::packs::FieldType::*;
    match t {
        Text => "text".into(),
        LongText => "longText".into(),
        Email => "email".into(),
        Phone => "phone".into(),
        Integer => "integer".into(),
        Number => "number".into(),
        Currency => "currency".into(),
        Date => "date".into(),
        DateTime => "dateTime".into(),
        Bool => "bool".into(),
        Enum { options } => format!("enum({})", options.join("|")),
        Ref { table } => format!("ref({table})"),
        Json => "json".into(),
        Timestamp => "timestamp".into(),
    }
}
