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
pub mod domain_cmd;
pub mod pdf;
pub mod pdf_cmd;
mod tables;

use crate::packs::{AlertDef, AlertSeverity, PackHandle, TableDef};

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

    fn description(&self) -> &'static str {
        "After-school enrichment program operations — coaches placed at \
         schools, billable hours, signed timesheets, NYC DoF invoicing."
    }

    fn default_enabled(&self) -> bool {
        // Existing v0.2.0 builds shipped with L2E enabled by default;
        // returning true preserves that behaviour for users upgrading.
        true
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

    fn tables(&self) -> &'static [TableDef] {
        tables::TABLES
    }

    fn alerts(&self) -> &'static [AlertDef] {
        ALERTS
    }
}

// Operational alerts — the layer-2 metric L2E sells on. Without these,
// the Splash screen shows "you have N invoices"; with them, it shows
// "you have $X in hours waiting to be invoiced" — actionable.
static ALERTS: &[AlertDef] = &[
    AlertDef {
        slug: "uninvoiced_hours",
        label: "Hours not yet invoiced",
        severity: AlertSeverity::Money,
        // Counts coach_hours rows with no covering non-void invoice for
        // the same coach in the same period. Sample fields are NULL for
        // v1; the alert page can drill in once we wire ref-resolution.
        sql: "SELECT COUNT(*) AS count, \
                     NULL AS sample_label, \
                     NULL AS sample_id \
              FROM coach_hours h \
              WHERE NOT EXISTS ( \
                SELECT 1 FROM invoice i \
                WHERE i.coach_id = h.coach_id \
                  AND h.session_date BETWEEN i.period_start AND i.period_end \
                  AND i.status != 'void' \
              )",
    },
    AlertDef {
        slug: "unsigned_sheets",
        label: "Signing sheets awaiting signature",
        severity: AlertSeverity::Action,
        sql: "SELECT COUNT(*) AS count, \
                     NULL AS sample_label, \
                     NULL AS sample_id \
              FROM signing_sheet \
              WHERE signed_at IS NULL",
    },
];

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
