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
pub mod pricing;
mod tables;
mod tools;

use crate::packs::{AlertDef, AlertSeverity, PackHandle, PackMigration, TableDef};

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
        // 0.4.0 — adds the invoicing document layer (work_order,
        // purchase_order, invoice_line, company_profile) + multi-line
        // invoice columns. LTE_INVOICING_SPEC.md.
        "0.4.0"
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

    fn migrations(&self) -> &'static [PackMigration] {
        MIGRATIONS
    }

    fn prompt_fragment(&self) -> Option<&'static str> {
        Some(PROMPT_FRAGMENT)
    }

    fn entity_kinds(&self) -> &'static [&'static str] {
        &["coach", "school", "dept", "module", "engagement"]
    }

    fn action_kinds(&self) -> &'static [&'static str] {
        &["propose_invoice_draft", "propose_program_invoice_draft"]
    }

    fn register_actions(&self, registry: &mut crate::actions::ActionRegistry) {
        registry.register(Box::new(actions::ProposeInvoiceDraftHandler));
        registry.register(Box::new(actions::ProposeProgramInvoiceDraftHandler));
    }

    fn register_tools(&self, registry: &mut crate::tools::ToolRegistry) {
        registry.register(Box::new(tools::quote_margin::QuoteMarginTool));
        registry.register(Box::new(tools::validate_invoice::ValidateInvoiceTool));
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
    // --- Program delivery: the 3 A's "what's stuck" set --------------
    AlertDef {
        slug: "unsigned_metrics_agreement",
        label: "Engagements delivering without a signed metrics agreement",
        severity: AlertSeverity::Action,
        // Scope built / delivery underway but the metrics agreement —
        // the gate between Action Planning and Accountable — isn't
        // signed. Accountability debt and a contract-risk gap.
        sql: "SELECT COUNT(*) AS count, \
                     (SELECT name FROM engagement \
                       WHERE stage IN ('action_planning','accountable') \
                         AND metrics_agreement_signed = 0 \
                       ORDER BY updated_at DESC LIMIT 1) AS sample_label, \
                     (SELECT id FROM engagement \
                       WHERE stage IN ('action_planning','accountable') \
                         AND metrics_agreement_signed = 0 \
                       ORDER BY updated_at DESC LIMIT 1) AS sample_id \
              FROM engagement \
              WHERE stage IN ('action_planning','accountable') \
                AND metrics_agreement_signed = 0",
    },
    AlertDef {
        slug: "overdue_accountability_review",
        label: "Active engagements with no accountability review on record",
        severity: AlertSeverity::Money,
        // An engagement in delivery with zero metrics reviews recorded.
        // Unreviewed metrics is what loses the renewal — the money
        // alert for the program side.
        sql: "SELECT COUNT(*) AS count, \
                     (SELECT name FROM engagement e \
                       WHERE e.stage = 'accountable' \
                         AND NOT EXISTS (SELECT 1 FROM accountability_review r \
                                          WHERE r.engagement_id = e.id) \
                       ORDER BY e.updated_at DESC LIMIT 1) AS sample_label, \
                     (SELECT id FROM engagement e \
                       WHERE e.stage = 'accountable' \
                         AND NOT EXISTS (SELECT 1 FROM accountability_review r \
                                          WHERE r.engagement_id = e.id) \
                       ORDER BY e.updated_at DESC LIMIT 1) AS sample_id \
              FROM engagement e \
              WHERE e.stage = 'accountable' \
                AND NOT EXISTS (SELECT 1 FROM accountability_review r \
                                 WHERE r.engagement_id = e.id)",
    },
    AlertDef {
        slug: "stalled_assessment",
        label: "Engagements stuck in Assessment with no diagnostic recorded",
        severity: AlertSeverity::Action,
        // Opened > 21 days ago, still in Assessment, no assessment row.
        // The diagnostic stalled — the 3 A's can't advance.
        sql: "SELECT COUNT(*) AS count, \
                     (SELECT name FROM engagement e \
                       WHERE e.stage = 'assessment' \
                         AND e.created_at <= datetime('now','-21 day') \
                         AND NOT EXISTS (SELECT 1 FROM assessment a \
                                          WHERE a.engagement_id = e.id) \
                       ORDER BY e.created_at ASC LIMIT 1) AS sample_label, \
                     (SELECT id FROM engagement e \
                       WHERE e.stage = 'assessment' \
                         AND e.created_at <= datetime('now','-21 day') \
                         AND NOT EXISTS (SELECT 1 FROM assessment a \
                                          WHERE a.engagement_id = e.id) \
                       ORDER BY e.created_at ASC LIMIT 1) AS sample_id \
              FROM engagement e \
              WHERE e.stage = 'assessment' \
                AND e.created_at <= datetime('now','-21 day') \
                AND NOT EXISTS (SELECT 1 FROM assessment a \
                                 WHERE a.engagement_id = e.id)",
    },
    // --- Invoicing: the "is the billing tidy?" set (LTE_INVOICING_SPEC §7)
    AlertDef {
        slug: "overlapping_invoice_period",
        label: "Invoices with overlapping periods or outside their PO window",
        severity: AlertSeverity::Money,
        // Solves Jacob-goes-from-memory: two non-void invoices for the
        // same engagement cover overlapping date ranges, OR an invoice
        // period falls outside its linked PO's activity window. Scope is
        // engagement_id (not school_id) — a school can host multiple
        // engagements in parallel (math + science + ELA), so two POs in
        // the same week are normal as long as they're different engagements.
        sql: "WITH problems AS ( \
                SELECT i1.id AS invoice_id, i1.number AS sample_label \
                FROM invoice i1 \
                JOIN invoice i2 \
                  ON i1.engagement_id IS NOT NULL \
                 AND i1.engagement_id = i2.engagement_id \
                 AND i1.id < i2.id \
                 AND i1.status != 'void' AND i2.status != 'void' \
                 AND i1.period_end >= i2.period_start \
                 AND i1.period_start <= i2.period_end \
                UNION \
                SELECT i.id AS invoice_id, i.number AS sample_label \
                FROM invoice i \
                JOIN purchase_order po ON po.id = i.purchase_order_id \
                WHERE i.status != 'void' \
                  AND (i.period_start < po.activity_start \
                       OR i.period_end > po.activity_end) \
              ) \
              SELECT COUNT(*) AS count, \
                     (SELECT sample_label FROM problems LIMIT 1) AS sample_label, \
                     (SELECT invoice_id FROM problems LIMIT 1) AS sample_id \
              FROM problems",
    },
    AlertDef {
        slug: "wo_date_outside_school_year",
        label: "Work orders with a date outside the engagement's school year",
        severity: AlertSeverity::Action,
        // Catches the PS 498-style 02/15/2025-vs-2026 typo. We parse
        // engagement.school_year as the first four chars (\"2026-2027\"
        // -> \"2026\") and check that the WO date's year is within
        // [start_year, start_year+1]. Schools with malformed school_year
        // values (NULL, empty, non-numeric) are skipped — the alert
        // is for fixing typos, not for hassling about unset fields.
        sql: "SELECT COUNT(*) AS count, \
                     (SELECT contract_ref FROM work_order wo \
                       JOIN engagement e ON e.id = wo.engagement_id \
                       WHERE wo.date_issued IS NOT NULL \
                         AND e.school_year IS NOT NULL \
                         AND LENGTH(e.school_year) >= 4 \
                         AND CAST(substr(e.school_year, 1, 4) AS INTEGER) > 0 \
                         AND ( \
                            CAST(substr(wo.date_issued, 1, 4) AS INTEGER) \
                              < CAST(substr(e.school_year, 1, 4) AS INTEGER) \
                            OR CAST(substr(wo.date_issued, 1, 4) AS INTEGER) \
                              > CAST(substr(e.school_year, 1, 4) AS INTEGER) + 1 \
                         ) \
                       ORDER BY wo.id ASC LIMIT 1) AS sample_label, \
                     (SELECT wo.id FROM work_order wo \
                       JOIN engagement e ON e.id = wo.engagement_id \
                       WHERE wo.date_issued IS NOT NULL \
                         AND e.school_year IS NOT NULL \
                         AND LENGTH(e.school_year) >= 4 \
                         AND CAST(substr(e.school_year, 1, 4) AS INTEGER) > 0 \
                         AND ( \
                            CAST(substr(wo.date_issued, 1, 4) AS INTEGER) \
                              < CAST(substr(e.school_year, 1, 4) AS INTEGER) \
                            OR CAST(substr(wo.date_issued, 1, 4) AS INTEGER) \
                              > CAST(substr(e.school_year, 1, 4) AS INTEGER) + 1 \
                         ) \
                       ORDER BY wo.id ASC LIMIT 1) AS sample_id \
              FROM work_order wo \
              JOIN engagement e ON e.id = wo.engagement_id \
              WHERE wo.date_issued IS NOT NULL \
                AND e.school_year IS NOT NULL \
                AND LENGTH(e.school_year) >= 4 \
                AND CAST(substr(e.school_year, 1, 4) AS INTEGER) > 0 \
                AND ( \
                   CAST(substr(wo.date_issued, 1, 4) AS INTEGER) \
                     < CAST(substr(e.school_year, 1, 4) AS INTEGER) \
                   OR CAST(substr(wo.date_issued, 1, 4) AS INTEGER) \
                     > CAST(substr(e.school_year, 1, 4) AS INTEGER) + 1 \
                )",
    },
];

// Pack-owned migrations. Numbering is independent of core's
// `_sqlx_migrations`; tracked in `meta.pack.lead-to-empower.
// schema_version`. The billing-spine tables predate pack migrations
// and stay in core's 0003_domain.sql — see domain/mod.rs.
const PROGRAM_DELIVERY_SQL: &str = include_str!("migrations/0001_program_delivery.sql");
const QUOTE_SQL: &str = include_str!("migrations/0002_quote.sql");
const INVOICING_SQL: &str = include_str!("migrations/0003_invoicing.sql");

static MIGRATIONS: &[PackMigration] = &[
    PackMigration {
        name: "0001_program_delivery",
        sql: PROGRAM_DELIVERY_SQL,
    },
    PackMigration {
        name: "0002_quote",
        sql: QUOTE_SQL,
    },
    PackMigration {
        name: "0003_invoicing",
        sql: INVOICING_SQL,
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
even if no specific action is requested.\n\
\n\
LTE delivery runs the \"3 A's\": every school engagement moves\n\
Assessment -> Action Planning -> Accountable -> closed.\n\
- Assessment: surveys, walkthroughs, observations, data analysis\n\
  against the leadership rubric. Record each as an assessment on the\n\
  engagement.\n\
- Action Planning: the scope of work — which catalog modules, for\n\
  whom, when. The signed metrics agreement gates the move into\n\
  delivery.\n\
- Accountable: delivering modules + ~3 metrics reviews/year (Sept\n\
  baseline, Jan mid, May/June reflection).\n\
The catalog is 21 priced modules across two pillars (Leadership\n\
Development; Data-Driven Decision-Making & Teacher Effectiveness).\n\
When the user mentions a school, walkthrough, module, or metrics\n\
review, record it against the right engagement even if no action is\n\
asked. If a mention implies the engagement changed stage, note it\n\
and confirm the transition in conversation rather than asking\n\
permission to track.\
";
