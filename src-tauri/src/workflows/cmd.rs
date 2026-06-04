//! Tauri commands for the workflow surface.
//!
//! The frontend needs to render an "active workflow" indicator that
//! survives between user turns — Taylor wants to feel like Travis is
//! still working on the thing she asked for, not waiting silently
//! between messages. `get_active_workflow` returns the full state
//! (recipe definition + slot states) so the UI can show the progress
//! pill, expandable slot list, and "next ask" hint.

use serde::Serialize;
use sqlx::SqlitePool;
use tauri::State;

use super::recipe::{Slot, SlotKind};
use super::registry::find_recipe;
use super::state::{self, SlotSource};
use crate::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowSurface {
    pub id: i64,
    pub conversation_id: i64,
    pub recipe_name: String,
    pub display_name: String,
    pub description: String,
    pub status: String,
    pub started_intent: Option<String>,
    pub started_at: String,
    pub last_activity_at: String,
    pub finalize_action: String,
    pub slots: Vec<SlotSurface>,
    pub filled_count: usize,
    pub required_total: usize,
    pub next_ask: Option<NextAsk>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlotSurface {
    pub name: String,
    pub label: String,
    pub kind: String,
    pub required: bool,
    pub filled: bool,
    pub value_preview: Option<String>,
    pub source: Option<SlotSource>,
    pub resolved_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NextAsk {
    pub slot_name: String,
    pub label: String,
    pub kind: String,
    pub ask_hint: String,
}

/// Resolve the active workflow for a conversation. Returns `None`
/// when no workflow is in flight. Used by the AskTab / overlay to
/// render the progress pill.
#[tauri::command]
pub async fn get_active_workflow(
    state: State<'_, AppState>,
    conversation_id: i64,
) -> Result<Option<WorkflowSurface>, String> {
    Ok(build_surface(&state.db.pool, conversation_id).await)
}

async fn build_surface(
    pool: &SqlitePool,
    conversation_id: i64,
) -> Option<WorkflowSurface> {
    let st = state::get_active(pool, conversation_id).await?;
    let recipe = find_recipe(&st.recipe_name)?;

    let mut slots: Vec<SlotSurface> = Vec::new();
    let mut filled_count = 0usize;
    let mut required_total = 0usize;
    for slot in recipe.slots {
        if slot.required {
            required_total += 1;
        }
        let filled_value = st.slots.get(slot.name);
        let filled = filled_value.is_some();
        if filled && slot.required {
            filled_count += 1;
        }
        slots.push(SlotSurface {
            name: slot.name.to_string(),
            label: slot.label.to_string(),
            kind: slot.kind.label(),
            required: slot.required,
            filled,
            value_preview: filled_value.map(|v| preview_value(&v.value)),
            source: filled_value.map(|v| v.source),
            resolved_at: filled_value.map(|v| v.resolved_at.clone()),
        });
    }

    let next_ask = recipe
        .slots
        .iter()
        .find(|s: &&Slot| s.required && !st.slots.contains_key(s.name))
        .map(|s| NextAsk {
            slot_name: s.name.to_string(),
            label: s.label.to_string(),
            kind: s.kind.label(),
            ask_hint: s.ask_hint.to_string(),
        });

    Some(WorkflowSurface {
        id: st.id,
        conversation_id: st.conversation_id,
        recipe_name: st.recipe_name,
        display_name: recipe.display_name.to_string(),
        description: recipe.description.to_string(),
        status: st.status,
        started_intent: st.started_intent,
        started_at: st.started_at,
        last_activity_at: st.last_activity_at,
        finalize_action: recipe.finalize_action.to_string(),
        slots,
        filled_count,
        required_total,
        next_ask,
    })
}

fn preview_value(v: &serde_json::Value) -> String {
    use serde_json::Value;
    match v {
        Value::String(s) => {
            if s.len() > 60 {
                format!("{}…", &s[..60])
            } else {
                s.clone()
            }
        }
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "—".into(),
        Value::Object(map) => {
            for key in ["name", "label", "title", "display_name"] {
                if let Some(Value::String(s)) = map.get(key) {
                    return s.clone();
                }
            }
            if let Some(Value::Number(n)) = map.get("id") {
                return format!("#{n}");
            }
            let s = serde_json::to_string(map).unwrap_or_default();
            if s.len() > 60 { format!("{}…", &s[..60]) } else { s }
        }
        Value::Array(arr) => format!("[{} item{}]", arr.len(), if arr.len() == 1 { "" } else { "s" }),
    }
}

// SlotKind is also surfaced via the trait's `label()` method, but
// keep this re-export so external callers can import the type from
// the cmd surface without reaching into recipe.
#[allow(dead_code)]
pub use super::recipe::SlotKind as _SlotKindForCmd;
