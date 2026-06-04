//! Workflow recipe definitions — declarative shape of each multi-step
//! output Travis can drive (invoice generation, sign-in sheet curation,
//! contract drafting, etc.).
//!
//! Recipes are `&'static` to keep the registry in the binary's read-only
//! data section, matching the [`PackHandle`] pattern.

/// One workflow recipe — what inputs it needs, how to ask for them,
/// and what action to dispatch when complete.
#[derive(Debug, Clone, Copy)]
pub struct WorkflowDef {
    /// Stable identifier — used in the workflow_state table and in
    /// LLM workflowOps emissions. Lowercase, snake_case.
    pub name: &'static str,

    /// Human-facing name shown in confirmation cards and logs.
    pub display_name: &'static str,

    /// One-line description for the LLM — what this workflow produces.
    pub description: &'static str,

    /// Required slots, in the order Travis should prefer to ask for
    /// them when multiple are unfilled. The first required + missing
    /// slot is what Travis asks about next.
    pub slots: &'static [Slot],

    /// Action kind the dialogue manager dispatches when all required
    /// slots are filled. The action handler receives the slot values
    /// as its params.
    pub finalize_action: &'static str,
}

/// One slot in a workflow — a piece of information the recipe needs
/// before it can finalise.
#[derive(Debug, Clone, Copy)]
pub struct Slot {
    /// Stable identifier — used as the key in workflow_state.slots_json
    /// and in LLM workflowOps emissions.
    pub name: &'static str,

    /// Human-facing label rendered in the prompt block and confirmation
    /// cards. "Signed sign-in sheet", not "signed_sheet".
    pub label: &'static str,

    /// Required slots block finalisation; optional slots are nice-to-have.
    pub required: bool,

    /// Type — drives how Travis asks for it, how to validate the value,
    /// and how to render it back.
    pub kind: SlotKind,

    /// Phrasing hint for the LLM — used in the prompt to guide how
    /// Travis should ask for this slot when it's the next missing one.
    /// One short sentence; Travis adapts to context.
    pub ask_hint: &'static str,
}

/// Slot value types. Drives both the asking strategy ("which engagement?"
/// suggests selection chips of existing engagements; "drop the PO"
/// suggests file-attach) and the persisted shape.
#[derive(Debug, Clone, Copy)]
pub enum SlotKind {
    /// Free text — name, note, etc.
    Text,

    /// ISO 8601 date (YYYY-MM-DD).
    Date,

    /// Two dates — "Jan-Feb" or "2026-01-29 to 2026-02-24". LLM normalises.
    DateRange,

    /// Reference to an existing entity in the graph. `kind` matches an
    /// entity kind declared by a pack (e.g. "school", "engagement",
    /// "coach", "contract").
    Entity { kind: &'static str },

    /// A document — Travis can either accept a freshly dropped PDF for
    /// extraction, or surface existing documents linked to the relevant
    /// entity. `kind` is the document kind ("po", "wo", "signed_sheet",
    /// "contract", "invoice"). Slice 2+ wires the actual file attach.
    Document { kind: &'static str },

    /// Monetary amount in cents.
    Money,

    /// Floating-point number — hours, multipliers, etc.
    Number,
}

impl SlotKind {
    /// Short label for the LLM prompt block — readable, machine-parseable.
    pub fn label(&self) -> String {
        match self {
            SlotKind::Text => "text".into(),
            SlotKind::Date => "date".into(),
            SlotKind::DateRange => "date-range".into(),
            SlotKind::Entity { kind } => format!("entity:{kind}"),
            SlotKind::Document { kind } => format!("document:{kind}"),
            SlotKind::Money => "money".into(),
            SlotKind::Number => "number".into(),
        }
    }
}
