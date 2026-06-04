//! Workflow registry — collects recipes contributed by every enabled
//! pack, plus any core-shipped defaults (currently none).
//!
//! The framework is generic; the recipes themselves are domain-specific
//! and live with their pack (`PackHandle::workflows()`). This mirrors
//! the action/tool/table registries: core defines the shape, packs fill
//! in the content.

use super::recipe::WorkflowDef;

/// Recipes core ships with — currently empty. Reserved for future
/// vertical-agnostic workflows (e.g. a generic "schedule reminder"
/// recipe that might live in core).
const CORE_RECIPES: &[WorkflowDef] = &[];

/// Walk core + every compiled-in pack and look up a recipe by name.
/// Returns the first match. Pack ordering is `compiled_in_packs()`
/// order; conflicts (two packs with the same recipe name) resolve to
/// the first found — we should avoid collisions by namespacing
/// recipes by their pack's domain (e.g. `lte_generate_invoice`).
pub fn find_recipe(name: &str) -> Option<&'static WorkflowDef> {
    if let Some(r) = CORE_RECIPES.iter().find(|r| r.name == name) {
        return Some(r);
    }
    for pack in crate::packs::compiled_in_packs() {
        if let Some(r) = pack.workflows().iter().find(|r| r.name == name) {
            return Some(r);
        }
    }
    None
}

/// Every recipe currently registered — used by the journal extraction
/// schema to declare which workflow names the LLM is allowed to emit.
/// Order: core first, then packs in `compiled_in_packs()` order.
pub fn all_recipes() -> Vec<&'static WorkflowDef> {
    let mut out: Vec<&'static WorkflowDef> = CORE_RECIPES.iter().collect();
    for pack in crate::packs::compiled_in_packs() {
        for r in pack.workflows() {
            out.push(r);
        }
    }
    out
}
