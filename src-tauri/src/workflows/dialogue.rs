//! Dialogue manager — renders the active workflow state into a
//! prompt block the LLM reads each turn.
//!
//! The block tells Travis: which recipe is running, which slots are
//! filled (and where each value came from), which slots are still
//! missing, and which one to ask about next. The LLM does the actual
//! conversational work; this module just exposes the state cleanly.

use super::recipe::WorkflowDef;
use super::registry::find_recipe;
use super::state::{SlotSource, WorkflowState};

/// Build the WORKFLOW prompt block for a conversation's active state.
/// Returns empty string when no workflow is active (caller can append
/// without separator handling).
pub fn format_for_prompt(state: Option<&WorkflowState>) -> String {
    let Some(state) = state else {
        return String::new();
    };
    let Some(recipe) = find_recipe(&state.recipe_name) else {
        // Stale recipe name — shouldn't happen in practice. Render
        // a minimal hint so we don't silently swallow the state.
        return format!(
            "ACTIVE WORKFLOW: {} (recipe definition missing — treat as abandoned)\n",
            state.recipe_name
        );
    };

    let mut s = String::from("ACTIVE WORKFLOW: ");
    s.push_str(recipe.display_name);
    s.push_str(" (");
    s.push_str(recipe.name);
    s.push_str(")\n");

    if let Some(intent) = state.started_intent.as_deref() {
        let intent = intent.trim();
        if !intent.is_empty() {
            s.push_str(&format!("Stated intent: \"{intent}\"\n"));
        }
    }

    // Filled slots — show what Travis already has and where it came from.
    let filled: Vec<_> = recipe
        .slots
        .iter()
        .filter(|slot| state.slots.contains_key(slot.name))
        .collect();
    if !filled.is_empty() {
        s.push_str("Filled:\n");
        for slot in &filled {
            let v = &state.slots[slot.name];
            let source_label = match v.source {
                SlotSource::UserTyped => "you typed it",
                SlotSource::Extracted => "extracted from a doc",
                SlotSource::UserDropped => "doc you dropped",
                SlotSource::GraphResolved => "from prior context",
            };
            let value_preview = preview_value(&v.value);
            s.push_str(&format!(
                "  - {} ({}): {} [{}]\n",
                slot.label,
                slot.kind.label(),
                value_preview,
                source_label,
            ));
        }
    }

    // Missing slots — required first, then optional.
    let mut missing_required: Vec<_> = recipe
        .slots
        .iter()
        .filter(|slot| slot.required && !state.slots.contains_key(slot.name))
        .collect();
    let mut missing_optional: Vec<_> = recipe
        .slots
        .iter()
        .filter(|slot| !slot.required && !state.slots.contains_key(slot.name))
        .collect();

    if !missing_required.is_empty() || !missing_optional.is_empty() {
        s.push_str("Missing:\n");
        for slot in missing_required.drain(..) {
            s.push_str(&format!(
                "  - {} ({}, required) — {}\n",
                slot.label,
                slot.kind.label(),
                slot.ask_hint,
            ));
        }
        for slot in missing_optional.drain(..) {
            s.push_str(&format!(
                "  - {} ({}, optional) — {}\n",
                slot.label,
                slot.kind.label(),
                slot.ask_hint,
            ));
        }
    }

    // Guidance for the LLM on what to do next.
    s.push_str(&next_move_hint(recipe, state));

    s
}

/// Tell the LLM what to do next given the current state. Three branches:
/// - any required slot missing → ask for one (the highest-priority one)
/// - all required filled → call finalize action
/// - everything filled → either finalize or offer to refine optional slots
fn next_move_hint(recipe: &WorkflowDef, state: &WorkflowState) -> String {
    let required_missing: Vec<_> = recipe
        .slots
        .iter()
        .filter(|slot| slot.required && !state.slots.contains_key(slot.name))
        .collect();

    if let Some(next) = required_missing.first() {
        format!(
            "Next move: ask the user for \"{}\" — only this one, conversationally. \
             {} Once they answer, emit a workflowOp to fill the slot, then re-evaluate \
             on the next turn.\n",
            next.label, next.ask_hint
        )
    } else {
        format!(
            "Next move: all required slots filled. Propose the {} action with the \
             collected slot values, then emit a workflowOp with kind:\"completed\" so \
             this workflow stops surfacing.\n",
            recipe.finalize_action
        )
    }
}

/// Render a slot value for the prompt block — compact, human-readable.
/// Long blobs are truncated; structured values flattened to a short
/// label.
fn preview_value(v: &serde_json::Value) -> String {
    use serde_json::Value;
    match v {
        Value::String(s) => {
            if s.len() > 80 {
                format!("\"{}…\"", &s[..80])
            } else {
                format!("\"{s}\"")
            }
        }
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".into(),
        Value::Object(map) => {
            // Prefer name/label/id keys for entity-shaped values.
            for key in ["name", "label", "title", "display_name"] {
                if let Some(Value::String(s)) = map.get(key) {
                    return format!("\"{s}\"");
                }
            }
            if let Some(Value::Number(n)) = map.get("id") {
                return format!("{{id: {n}}}");
            }
            let s = serde_json::to_string(map).unwrap_or_default();
            if s.len() > 80 {
                format!("{}…", &s[..80])
            } else {
                s
            }
        }
        Value::Array(_) => {
            let s = serde_json::to_string(v).unwrap_or_default();
            if s.len() > 80 {
                format!("{}…", &s[..80])
            } else {
                s
            }
        }
    }
}
