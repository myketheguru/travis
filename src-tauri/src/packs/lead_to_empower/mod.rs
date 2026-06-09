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
mod workflows;

use crate::packs::{
    AlertDef, AlertSeverity, PackHandle, PackMigration, TableDef, ValveDef, ValveType,
    ValveValue,
};
use crate::workflows::recipe::WorkflowDef;

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
        // 0.7.0 — collapse engagement+contract into one unified
        // "Contract" record per Taylor's feedback. Migration 0005
        // extends engagement with the contract-shape fields; UI/prompt
        // language now says "contract" everywhere. Adds the PO/WO →
        // contract workflow and draw-down tracking.
        "0.7.0"
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
        &[
            "propose_invoice_draft",
            "propose_program_invoice_draft",
            "lte_create_contract",
            "lte_create_engagement",
            "lte_record_coach_hours",
            "lte_create_work_order",
            "lte_create_purchase_order",
            "lte_derive_sign_in_sheet",
            "lte_create_contract_from_doc",
        ]
    }

    fn register_actions(&self, registry: &mut crate::actions::ActionRegistry) {
        registry.register(Box::new(actions::ProposeInvoiceDraftHandler));
        registry.register(Box::new(actions::ProposeProgramInvoiceDraftHandler));
        registry.register(Box::new(actions::CreateContractHandler));
        registry.register(Box::new(actions::CreateEngagementHandler));
        registry.register(Box::new(actions::RecordCoachHoursHandler));
        registry.register(Box::new(actions::CreateWorkOrderHandler));
        registry.register(Box::new(actions::CreatePurchaseOrderHandler));
        registry.register(Box::new(actions::DeriveSignInSheetHandler));
        registry.register(Box::new(actions::CreateContractFromDocHandler));
    }

    fn register_tools(&self, registry: &mut crate::tools::ToolRegistry) {
        registry.register(Box::new(tools::quote_margin::QuoteMarginTool));
        registry.register(Box::new(tools::validate_invoice::ValidateInvoiceTool));
        registry.register(Box::new(tools::find_school::FindOrCreateSchoolTool));
        registry.register(Box::new(tools::find_contract::FindContractTool));
        registry.register(Box::new(tools::find_engagement::FindEngagementTool));
        registry.register(Box::new(tools::summarize_context::SummarizeContextTool));
    }

    fn tables(&self) -> &'static [TableDef] {
        tables::TABLES
    }

    fn alerts(&self) -> &'static [AlertDef] {
        ALERTS
    }

    fn workflows(&self) -> &'static [WorkflowDef] {
        workflows::WORKFLOWS
    }

    fn valves(&self) -> &'static [ValveDef] {
        VALVES
    }
}

// Pack-author-declared settings. Travis renders the form in Settings →
// Packs; user changes land in meta.pack.lead-to-empower.valve.<slug>.
// Pack code reads via `packs::get_valve_text(...)` etc.
static VALVES: &[ValveDef] = &[
    ValveDef {
        slug: "default_invoice_terms",
        label: "Default invoice payment terms",
        valve_type: ValveType::Enum {
            options: &["Net 15", "Net 30", "Net 45", "Net 60", "Due on receipt"],
        },
        default: ValveValue::Text("Net 30"),
        help: Some(
            "Used as the default Terms field when Travis drafts a new invoice. \
             You can still override per-invoice in the form.",
        ),
    },
    ValveDef {
        slug: "auto_lock_signed_sheets",
        label: "Auto-lock signed sign-in sheets",
        valve_type: ValveType::Bool,
        default: ValveValue::Bool(true),
        help: Some(
            "When on, once a sign-in sheet is marked signed, Travis blocks \
             further edits to its rows. Turn off if you frequently get \
             post-signature corrections.",
        ),
    },
    ValveDef {
        slug: "dof_route_default_program",
        label: "Default program for DoF-route invoices",
        valve_type: ValveType::Text,
        default: ValveValue::Text(""),
        help: Some(
            "When set, new DoF-route invoices pre-fill this program name. \
             Leave blank to require explicit selection each time.",
        ),
    },
];

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
    // --- Contracts: the burn/expiry pair (slice 6) ----------------------
    AlertDef {
        slug: "contract_near_ceiling",
        label: "Contracts near their billing ceiling",
        severity: AlertSeverity::Money,
        // Active contracts where the sum of non-void invoice.amount_cents
        // (rolled up via engagement.contract_id) is >= 90% of
        // contract.ceiling_cents. Ceiling 0 is treated as "unset" and
        // excluded — the alert is for tracked-ceiling contracts only.
        sql: "WITH burn AS ( \
                SELECT c.id AS contract_id, c.ref AS ref, c.ceiling_cents AS ceiling, \
                       COALESCE(SUM(i.amount_cents), 0) AS billed \
                FROM contract c \
                LEFT JOIN engagement e ON e.contract_id = c.id \
                LEFT JOIN invoice i ON i.engagement_id = e.id AND i.status != 'void' \
                WHERE c.status = 'active' AND c.ceiling_cents > 0 \
                GROUP BY c.id, c.ref, c.ceiling_cents \
              ) \
              SELECT COUNT(*) AS count, \
                     (SELECT ref FROM burn WHERE billed * 10 >= ceiling * 9 \
                       ORDER BY (billed * 1.0 / ceiling) DESC LIMIT 1) AS sample_label, \
                     (SELECT contract_id FROM burn WHERE billed * 10 >= ceiling * 9 \
                       ORDER BY (billed * 1.0 / ceiling) DESC LIMIT 1) AS sample_id \
              FROM burn WHERE billed * 10 >= ceiling * 9",
    },
    AlertDef {
        slug: "contract_expiring_soon",
        label: "Active contracts expiring within 60 days",
        severity: AlertSeverity::Action,
        // Active contracts whose term_end falls between today and 60 days
        // out. Excludes contracts with NULL term_end (term not set means
        // no expiry tracking, not urgent).
        sql: "SELECT COUNT(*) AS count, \
                     (SELECT ref FROM contract \
                       WHERE status = 'active' \
                         AND term_end IS NOT NULL \
                         AND term_end >= date('now') \
                         AND term_end <= date('now', '+60 day') \
                       ORDER BY term_end ASC LIMIT 1) AS sample_label, \
                     (SELECT id FROM contract \
                       WHERE status = 'active' \
                         AND term_end IS NOT NULL \
                         AND term_end >= date('now') \
                         AND term_end <= date('now', '+60 day') \
                       ORDER BY term_end ASC LIMIT 1) AS sample_id \
              FROM contract \
              WHERE status = 'active' \
                AND term_end IS NOT NULL \
                AND term_end >= date('now') \
                AND term_end <= date('now', '+60 day')",
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
const CONTRACTS_SQL: &str = include_str!("migrations/0004_contracts.sql");
const COLLAPSE_CONTRACT_SQL: &str =
    include_str!("migrations/0005_collapse_contract_engagement.sql");

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
    PackMigration {
        name: "0004_contracts",
        sql: CONTRACTS_SQL,
    },
    PackMigration {
        name: "0005_collapse_contract_engagement",
        sql: COLLAPSE_CONTRACT_SQL,
    },
];

/// System-prompt fragment contributed by the L2E pack. Currently unused
/// — step 10 of the pack refactor (PACKS_AUDIT.md) wires the system-
/// prompt assembly call sites to ask the pack registry for fragments.
/// Until then this fragment stays dead-coded as documentation of what
/// the pack will surface.
const PROMPT_FRAGMENT: &str = "\
You also help with after-school enrichment program ops:\n\
\n\
IMPORTANT vocabulary (as of pack v0.7.0):\n\
- \"Contract\" and \"engagement\" refer to the SAME record. Internally\n\
  the table is `engagement` (for code stability); externally you ALWAYS\n\
  call it a contract in conversation with Taylor. A contract = one\n\
  piece of work at one school (e.g. \"PS 498 math team coaching\").\n\
  Multiple contracts per school is normal (math + science + ELA).\n\
- A contract has a ceiling_cents (total dollar value). Invoices draw\n\
  down against it — multiple invoices per contract until ceiling is\n\
  reached.\n\
\n\
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
permission to track.\n\
\n\
=== Chat-first L2E ops ===\n\
\n\
The chat is the COO's primary interface. Drive every L2E operation\n\
through tools and actions — never tell her to \"go to the Manage\n\
tab\" unless she explicitly asks where a thing lives.\n\
\n\
RESOLVING ENTITIES (do this BEFORE proposing creates):\n\
- School mentioned? Call lte_find_or_create_school first. If the top\n\
  result is an exact name match, use it. If 2-3 are close, list the\n\
  top results as a markdown selection list (see Selection UX below)\n\
  and ask. If no match, the tool creates the school silently — no\n\
  confirmation needed (observational data).\n\
- Contract mentioned/needed? Call lte_find_contract first. If the\n\
  top result is unambiguous, use it. If ambiguous, present options.\n\
  If no match exists, propose lte_create_contract (action — needs\n\
  confirmation since contracts commit to a relationship).\n\
- Engagement mentioned/needed? Call lte_find_engagement. Same logic:\n\
  unambiguous match → use; ambiguous → list; missing → propose\n\
  lte_create_engagement.\n\
- Use lte_summarize_context when the user references something\n\
  fuzzily (\"the math contract\", \"that PS498 engagement\") to ground\n\
  your reply in what Travis actually knows.\n\
\n\
CONFIRMATION POLICY (you decide per action):\n\
- Silent (no confirmation card, just track-and-go):\n\
  * lte_find_or_create_school silent creates\n\
  * Enrichment updates to existing rows (adding a contact email,\n\
    correcting a typo'd district number)\n\
  * Attribute additions Travis inferred from context\n\
- Confirm with a single-line card (default-yes):\n\
  * lte_create_contract — commits to a relationship\n\
  * lte_create_engagement — commits to a billable scope\n\
  * propose_program_invoice_draft — creates a billable artifact\n\
- Always confirm (regardless of context):\n\
  * Marking an invoice sent / paid / void\n\
  * Anything visible to people outside Travis (emails, calendar\n\
    invites to the school)\n\
  * Deletions of any typed row\n\
\n\
ASKING FOR MISSING CONTEXT:\n\
- One question per gap. Pick the highest-leverage gap first.\n\
- When the answer space is a finite small set (active contracts,\n\
  catalog modules, status enums, schools she's worked with), present\n\
  the options as a Selection UX list (below). Never make her type\n\
  what she could click.\n\
- Default reasonably: status='active', term_end +1 year after\n\
  term_start if unset, school_year inferred from today's date,\n\
  scope items inferred from the engagement.\n\
\n\
SELECTION UX MARKERS (the chat renderer detects these and turns each\n\
line into a click-to-fill chip):\n\
- ⊙ single-select option (\"pick one\")\n\
- ⊡ multi-select option (\"pick any\")\n\
- ⊕ add-new option (\"create a new ...\")\n\
- 📅 date picker prompt\n\
Example:\n\
  > Which contract is this under?\n\
  > ⊙ QR179CF — Systemwide Services (active, 38% burn)\n\
  > ⊙ NYCPS HS Math — Supt. White pursuit (active)\n\
  > ⊙ NYCPS Tutoring (active, ends 2027-06-30)\n\
  > ⊕ New contract\n\
Always include a \"⊕ New ...\" option when a new entity is plausible.\n\
\n\
RANKING + RATIONALE:\n\
- The lte_find_* tools return candidates ranked by status priority\n\
  then recency of activity then by metric (ceiling remaining for\n\
  contracts, hours delivered for engagements). Trust the order they\n\
  return.\n\
- When you present options, include one fact that disambiguates:\n\
  burn %, term end, last activity date, etc. Don't dump full IDs.\n\
\n\
RESUMPTION:\n\
- If the COO walked away mid-flow, scan the last few assistant\n\
  messages for \"I was waiting on ...\" or \"Need to know ...\" cues.\n\
  When she next mentions the topic, pick up where you left off:\n\
  \"I was waiting on the contract for PS95 — still QR179CF?\"\n\
\n\
BIAS TOWARD ACTION:\n\
- If you have enough to draft something with sensible defaults, do\n\
  it and let her edit. Don't ask three questions to be polite. Don't\n\
  explain the schema; just propose the next thing.\n\
\n\
=== Full chain of chat-first actions ===\n\
\n\
Every step of the LTE billing chain has a callable handler. Resolve\n\
parents before children; ask one focused question per gap.\n\
\n\
SCHOOLS (observational, silent creates):\n\
- lte_find_or_create_school — find by name, create silently on miss.\n\
\n\
CONTRACTS (relationship-committing, confirm card):\n\
- lte_find_contract — search ranked.\n\
- lte_create_contract — propose with confirmation. Default status\n\
  active, term_end +1 year from start if unset, name = ref.\n\
\n\
ENGAGEMENTS (billable, confirm card):\n\
- lte_find_engagement — search ranked by stage + recency.\n\
- lte_create_engagement — resolves school silently + contract by ref.\n\
  Default name '<School> — <SchoolYear>', stage 'assessment'.\n\
\n\
COACH HOURS (sign-in rows, confirm card per row OR batch):\n\
- lte_record_coach_hours — resolves coach (silent create), school,\n\
  engagement (must exist), engagement_module (so the row tags to the\n\
  right invoice line). Required: sessionDate, hours. The module tag\n\
  is what makes the date_list per scope item render on the invoice;\n\
  if the user mentions which scope item, tag it.\n\
\n\
WORK ORDERS (confirm card):\n\
- lte_create_work_order — resolves engagement; auto-totals from\n\
  engagement_module rows (SUM(qty * agreed_price)); date_issued\n\
  defaults to today. Vendor signature defaults from company_profile.\n\
\n\
PURCHASE ORDERS (received-from-DOE, confirm card):\n\
- lte_create_purchase_order — Taylor uploads the PDF separately; this\n\
  records the metadata so invoices can validate against the activity\n\
  window. Required: poNumber, activityStart, activityEnd. Suffix\n\
  defaults to '01'.\n\
\n\
INVOICES:\n\
- propose_program_invoice_draft — already exists; pairs naturally\n\
  with the rest. Pulls scope from engagement_module, dates from\n\
  tagged coach_hours, snapshots prices to invoice_line.\n\
- lte_validate_invoice — runs the draft→sent validators without\n\
  mutating. Use BEFORE proposing a send.\n\
\n\
=== Generic pack bridge ===\n\
\n\
When the user asks a question whose home table you don't already\n\
know:\n\
- Call `pack_introspect` to list all enabled packs' tables + fields.\n\
- Call `pack_query` with a filter map to read rows from any table.\n\
  Workspace clamp is automatic. Validates field names; rejects\n\
  unknown fields. Useful for arbitrary 'how many ...', 'show me\n\
  the ...', 'find ... where ...' shape questions across any pack\n\
  table, not just LTE.\n\
\n\
=== L2E invoice-specific rules ===\n\
\n\
(The generic document-editing / sample-→-adapt / multi-doc workflow\n\
patterns are in the core prompt's DOCUMENT HANDLING section. The\n\
items below are the L2E-specific knowledge that layers on top.)\n\
\n\
INVOICE NUMBERING. Format: year + school code + sequence. Example:\n\
`2026217002` = year 2026, school IS 217, second invoice this year\n\
for that school. The standard LTE prefix in the historical record\n\
is `LTE` — confirm preference with the user if uncertain. The\n\
master sign-in sheet doesn't track invoice counts; pull from the\n\
user's confirmation or ask explicitly which sequence number.\n\
\n\
DEFAULT RATES. Pull from the services catalog (Appendix F&G) when\n\
attached. Common L2E rates: Leadership Coaching ~$1,500/day\n\
(school-funded engagements) or $2,300/day (DoF-funded). For a\n\
specific engagement, the PO's authorised rate is ALWAYS the source\n\
of truth — it overrides both the catalog and any sample-invoice\n\
rate from a prior engagement.\n\
\n\
INVOICE FIELD ENUMERATION pattern when adapting a sample. The L2E\n\
invoice has these fields the user almost always needs to confirm or\n\
update for a new engagement. Use them as the basis for your\n\
numbered-list enumeration in your response:\n\
- Bill to (school name + address)\n\
- Invoice # (apply the numbering formula above)\n\
- Contract # (carries over from the master contract — usually\n\
  `QR179CF` for L2E)\n\
- Work Order # (from the PO/WO doc; replaces the prior WO box)\n\
- Description (matches the work-order line, e.g. \"Days of\n\
  Leadership Coaching\")\n\
- Service dates (derive from sign-in sheet, filtered to the PO\n\
  window)\n\
- Quantity (days/units billed — capped by PO)\n\
- Unit price (PO's authorised rate)\n\
\n\
For LUMP-SUM POs (single dollar amount, not per-day): the invoice's\n\
line items must sum to the agreed total exactly, drawing from the\n\
catalog services. You may need to allocate across multiple service\n\
lines (Leadership Coaching units, School Assessment units, module\n\
deliveries) to land on the target. Use code (`run_python`) for\n\
constraint-solving when the math gets tight.\n\
\n\
BILLING RULES. Two L2E rules to enforce:\n\
- Bill no MORE than the PO authorises (the cap).\n\
- Bill no LESS than the PO agreed amount per engagement (avoid\n\
  partial-bill leakage). If delivered hours exceed the cap, list\n\
  ALL delivered dates but keep QTY × rate matching the cap. The\n\
  over-delivery is uncompensated.\n\
\n\
=== Structured-action shortcuts (no sample supplied) ===\n\
\n\
When no sample is uploaded and the user says \"the usual\" or names a\n\
canonical L2E artifact (a standard invoice, a sign-in sheet in the\n\
DoF format), use the dedicated tools:\n\
- `propose_invoice_draft` — canonical LTE letterhead, standard fields.\n\
- `lte_derive_sign_in_sheet` — generates from logged coach hours.\n\
- `lte_create_contract_from_doc` — extracts contract from a PO upload.\n\
- `lte_validate_invoice` — pre-send validation (draft → sent).\n\
\n\
These are fast and deterministic. Use `run_python` instead when a\n\
sample is supplied to match a specific layout, OR when the math\n\
needs constraint-solving.\
";
