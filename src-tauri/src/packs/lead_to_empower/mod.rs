//! Lead to Empower pack — after-school enrichment program operations.
//!
//! Vertical: contractors-at-sites, billable hours, signed timesheets,
//! NYC Department of Finance invoicing. Travis's first vertical and
//! the validation case for the pack abstraction (PACKS.md, MARKET.md
//! tier-A item #1).
//!
//! For step 8 of the pack refactor (PACKS_AUDIT.md), this module is
//! the new home for L2E-specific code that previously lived in core.
//! Moves are landing incrementally: the action handler lands first
//! since it's the cleanest piece. The `domain/{coach,school,...}`
//! modules, `pdf/`, and the L2E commands move next.

mod actions;
pub mod domain;

use crate::packs::PackHandle;

const SLUG: &str = "lead-to-empower";

pub struct LeadToEmpowerPack;

impl PackHandle for LeadToEmpowerPack {
    fn slug(&self) -> &'static str {
        SLUG
    }

    fn name(&self) -> &'static str {
        "Lead to Empower"
    }

    fn version(&self) -> &'static str {
        "0.1.0"
    }

    fn prompt_fragment(&self) -> Option<&'static str> {
        Some(PROMPT_FRAGMENT)
    }

    fn entity_kinds(&self) -> &'static [&'static str] {
        &["coach", "school", "dept"]
    }

    fn action_kinds(&self) -> &'static [&'static str] {
        &["propose_invoice_draft"]
    }

    fn register_actions(&self, registry: &mut crate::actions::ActionRegistry) {
        registry.register(Box::new(actions::ProposeInvoiceDraftHandler));
    }
}

/// System-prompt fragment contributed by the L2E pack. Currently unused
/// — step 10 of the pack refactor (PACKS_AUDIT.md) wires the system-
/// prompt assembly call sites to ask the pack registry for fragments.
/// Until then this fragment stays dead-coded as documentation of what
/// the pack will surface.
const PROMPT_FRAGMENT: &str = "\
You also help with after-school enrichment program ops:\n\
- Track coaches placed at schools, their hourly rates, and hours worked.\n\
- Maintain signed timesheets (signing_sheets) — these are how the\n\
  Department of Finance authorizes payment.\n\
- Draft NYC DoF-shaped invoices when hours have been signed off.\n\
\n\
When the user mentions a coach by name, prefer recording the mention\n\
even if no specific action is requested.\
";
