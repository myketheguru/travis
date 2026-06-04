//! Schema metadata for the L2E pack's typed tables. Drives the auto-CRUD
//! UI and generic Tauri commands (PLUGIN_PLATFORM.md). The actual SQL
//! tables come from core's `0003_domain.sql` migration; this file only
//! describes them in enough detail that the auto-UI can render them.

use crate::packs::{FieldDef, FieldType, ListViewDef, SortDir, TableDef};

// ---------------------------------------------------------------------------
// coach
// ---------------------------------------------------------------------------

static COACH_FIELDS: &[FieldDef] = &[
    FieldDef {
        slug: "id",
        label: "ID",
        field_type: FieldType::Integer,
        required: false,
        help: None,
        default_in_list: false,
    },
    FieldDef {
        slug: "name",
        label: "Name",
        field_type: FieldType::Text,
        required: true,
        help: None,
        default_in_list: true,
    },
    FieldDef {
        slug: "email",
        label: "Email",
        field_type: FieldType::Email,
        required: false,
        help: None,
        default_in_list: true,
    },
    FieldDef {
        slug: "rate_cents",
        label: "Hourly Rate",
        field_type: FieldType::Currency,
        required: false,
        help: Some("What this coach charges per hour."),
        default_in_list: true,
    },
    FieldDef {
        slug: "notes",
        label: "Notes",
        field_type: FieldType::LongText,
        required: false,
        help: None,
        default_in_list: false,
    },
    FieldDef {
        slug: "created_at",
        label: "Created",
        field_type: FieldType::Timestamp,
        required: false,
        help: None,
        default_in_list: false,
    },
    FieldDef {
        slug: "updated_at",
        label: "Updated",
        field_type: FieldType::Timestamp,
        required: false,
        help: None,
        default_in_list: false,
    },
];

static COACH: TableDef = TableDef {
    slug: "coach",
    display_name: "Coaches",
    singular_name: "Coach",
    display_field: "name",
    entity_kind: Some("coach"),
    fields: COACH_FIELDS,
    primary: true,
    list_view: ListViewDef {
        columns: &["name", "email", "rate_cents"],
        default_sort: Some("name"),
        default_sort_dir: SortDir::Asc,
        page_size: 50,
    },
};

// ---------------------------------------------------------------------------
// school
// ---------------------------------------------------------------------------

static SCHOOL_FIELDS: &[FieldDef] = &[
    FieldDef {
        slug: "id",
        label: "ID",
        field_type: FieldType::Integer,
        required: false,
        help: None,
        default_in_list: false,
    },
    FieldDef {
        slug: "name",
        label: "Name",
        field_type: FieldType::Text,
        required: true,
        help: None,
        default_in_list: true,
    },
    FieldDef {
        slug: "district",
        label: "District",
        field_type: FieldType::Text,
        required: false,
        help: None,
        default_in_list: true,
    },
    FieldDef {
        slug: "contact_name",
        label: "Contact Name",
        field_type: FieldType::Text,
        required: false,
        help: None,
        default_in_list: true,
    },
    FieldDef {
        slug: "contact_email",
        label: "Contact Email",
        field_type: FieldType::Email,
        required: false,
        help: None,
        default_in_list: false,
    },
    FieldDef {
        slug: "notes",
        label: "Notes",
        field_type: FieldType::LongText,
        required: false,
        help: None,
        default_in_list: false,
    },
    FieldDef {
        slug: "created_at",
        label: "Created",
        field_type: FieldType::Timestamp,
        required: false,
        help: None,
        default_in_list: false,
    },
    FieldDef {
        slug: "updated_at",
        label: "Updated",
        field_type: FieldType::Timestamp,
        required: false,
        help: None,
        default_in_list: false,
    },
];

static SCHOOL: TableDef = TableDef {
    slug: "school",
    display_name: "Schools",
    singular_name: "School",
    display_field: "name",
    entity_kind: Some("school"),
    fields: SCHOOL_FIELDS,
    primary: true,
    list_view: ListViewDef {
        columns: &["name", "district", "contact_name"],
        default_sort: Some("name"),
        default_sort_dir: SortDir::Asc,
        page_size: 50,
    },
};

// ---------------------------------------------------------------------------
// coach_hours
// ---------------------------------------------------------------------------

static COACH_HOURS_FIELDS: &[FieldDef] = &[
    FieldDef {
        slug: "id",
        label: "ID",
        field_type: FieldType::Integer,
        required: false,
        help: None,
        default_in_list: false,
    },
    FieldDef {
        slug: "coach_id",
        label: "Coach",
        field_type: FieldType::Ref { table: "coach" },
        required: true,
        help: None,
        default_in_list: true,
    },
    FieldDef {
        slug: "school_id",
        label: "School",
        field_type: FieldType::Ref { table: "school" },
        required: true,
        help: None,
        default_in_list: true,
    },
    FieldDef {
        slug: "session_date",
        label: "Date",
        field_type: FieldType::Date,
        required: true,
        help: None,
        default_in_list: true,
    },
    FieldDef {
        slug: "hours",
        label: "Hours",
        field_type: FieldType::Number,
        required: true,
        help: Some("Decimal hours worked (e.g. 1.5 for 90 minutes)."),
        default_in_list: true,
    },
    FieldDef {
        slug: "description",
        label: "Description",
        field_type: FieldType::LongText,
        required: false,
        help: None,
        default_in_list: false,
    },
    FieldDef {
        slug: "created_at",
        label: "Created",
        field_type: FieldType::Timestamp,
        required: false,
        help: None,
        default_in_list: false,
    },
    FieldDef {
        slug: "updated_at",
        label: "Updated",
        field_type: FieldType::Timestamp,
        required: false,
        help: None,
        default_in_list: false,
    },
];

static COACH_HOURS: TableDef = TableDef {
    slug: "coach_hours",
    display_name: "Hours",
    singular_name: "Hours Entry",
    display_field: "session_date",
    entity_kind: None,
    fields: COACH_HOURS_FIELDS,
    primary: true,
    list_view: ListViewDef {
        columns: &["session_date", "coach_id", "school_id", "hours"],
        default_sort: Some("session_date"),
        default_sort_dir: SortDir::Desc,
        page_size: 100,
    },
};

// ---------------------------------------------------------------------------
// signing_sheet
// ---------------------------------------------------------------------------

static SIGNING_SHEET_FIELDS: &[FieldDef] = &[
    FieldDef {
        slug: "id",
        label: "ID",
        field_type: FieldType::Integer,
        required: false,
        help: None,
        default_in_list: false,
    },
    FieldDef {
        slug: "coach_id",
        label: "Coach",
        field_type: FieldType::Ref { table: "coach" },
        required: true,
        help: None,
        default_in_list: true,
    },
    FieldDef {
        slug: "school_id",
        label: "School",
        field_type: FieldType::Ref { table: "school" },
        required: true,
        help: None,
        default_in_list: true,
    },
    FieldDef {
        slug: "period_start",
        label: "Period Start",
        field_type: FieldType::Date,
        required: true,
        help: None,
        default_in_list: true,
    },
    FieldDef {
        slug: "period_end",
        label: "Period End",
        field_type: FieldType::Date,
        required: true,
        help: None,
        default_in_list: true,
    },
    FieldDef {
        slug: "signed_at",
        label: "Signed",
        field_type: FieldType::DateTime,
        required: false,
        help: Some("When the school signed off on the sheet."),
        default_in_list: true,
    },
    FieldDef {
        slug: "signed_by",
        label: "Signed By",
        field_type: FieldType::Text,
        required: false,
        help: None,
        default_in_list: false,
    },
    FieldDef {
        slug: "pdf_path",
        label: "PDF Path",
        field_type: FieldType::Text,
        required: false,
        help: None,
        default_in_list: false,
    },
    FieldDef {
        slug: "notes",
        label: "Notes",
        field_type: FieldType::LongText,
        required: false,
        help: None,
        default_in_list: false,
    },
    FieldDef {
        slug: "created_at",
        label: "Created",
        field_type: FieldType::Timestamp,
        required: false,
        help: None,
        default_in_list: false,
    },
    FieldDef {
        slug: "updated_at",
        label: "Updated",
        field_type: FieldType::Timestamp,
        required: false,
        help: None,
        default_in_list: false,
    },
];

static SIGNING_SHEET: TableDef = TableDef {
    slug: "signing_sheet",
    display_name: "Signing Sheets",
    singular_name: "Signing Sheet",
    display_field: "period_end",
    entity_kind: None,
    fields: SIGNING_SHEET_FIELDS,
    primary: true,
    list_view: ListViewDef {
        columns: &["coach_id", "school_id", "period_start", "period_end", "signed_at"],
        default_sort: Some("period_end"),
        default_sort_dir: SortDir::Desc,
        page_size: 50,
    },
};

// ---------------------------------------------------------------------------
// invoice
// ---------------------------------------------------------------------------

static INVOICE_FIELDS: &[FieldDef] = &[
    FieldDef {
        slug: "id",
        label: "ID",
        field_type: FieldType::Integer,
        required: false,
        help: None,
        default_in_list: false,
    },
    FieldDef {
        slug: "number",
        label: "Number",
        field_type: FieldType::Text,
        required: true,
        help: None,
        default_in_list: true,
    },
    FieldDef {
        slug: "recipient",
        label: "Recipient",
        field_type: FieldType::Text,
        required: true,
        help: None,
        default_in_list: true,
    },
    FieldDef {
        slug: "coach_id",
        label: "Coach",
        field_type: FieldType::Ref { table: "coach" },
        required: false,
        help: None,
        default_in_list: true,
    },
    FieldDef {
        slug: "school_id",
        label: "School",
        field_type: FieldType::Ref { table: "school" },
        required: false,
        help: None,
        default_in_list: false,
    },
    FieldDef {
        slug: "signing_sheet_id",
        label: "Signing Sheet",
        field_type: FieldType::Ref { table: "signing_sheet" },
        required: false,
        help: None,
        default_in_list: false,
    },
    FieldDef {
        slug: "period_start",
        label: "Period Start",
        field_type: FieldType::Date,
        required: true,
        help: None,
        default_in_list: false,
    },
    FieldDef {
        slug: "period_end",
        label: "Period End",
        field_type: FieldType::Date,
        required: true,
        help: None,
        default_in_list: false,
    },
    FieldDef {
        slug: "hours_total",
        label: "Hours",
        field_type: FieldType::Number,
        required: false,
        help: None,
        default_in_list: true,
    },
    FieldDef {
        slug: "rate_cents",
        label: "Rate",
        field_type: FieldType::Currency,
        required: false,
        help: None,
        default_in_list: false,
    },
    FieldDef {
        slug: "amount_cents",
        label: "Amount",
        field_type: FieldType::Currency,
        required: false,
        help: None,
        default_in_list: true,
    },
    FieldDef {
        slug: "status",
        label: "Status",
        field_type: FieldType::Enum {
            options: &["draft", "sent", "paid", "void"],
        },
        required: true,
        help: None,
        default_in_list: true,
    },
    FieldDef {
        slug: "issued_at",
        label: "Issued",
        field_type: FieldType::DateTime,
        required: false,
        help: None,
        default_in_list: false,
    },
    FieldDef {
        slug: "paid_at",
        label: "Paid",
        field_type: FieldType::DateTime,
        required: false,
        help: None,
        default_in_list: false,
    },
    FieldDef {
        slug: "engagement_id",
        label: "Engagement",
        field_type: FieldType::Ref { table: "engagement" },
        required: false,
        help: Some("Tie this invoice to its engagement — drives validators + multi-line drafting."),
        default_in_list: false,
    },
    FieldDef {
        slug: "purchase_order_id",
        label: "Purchase Order",
        field_type: FieldType::Ref { table: "purchase_order" },
        required: false,
        help: Some("The PO this invoice bills against. Period checks reference its activity window."),
        default_in_list: false,
    },
    FieldDef {
        slug: "school_signed_at",
        label: "Principal Signed",
        field_type: FieldType::DateTime,
        required: false,
        help: Some("Date the school principal countersigned the invoice."),
        default_in_list: false,
    },
    FieldDef {
        slug: "school_signed_by_name",
        label: "Signed By (Principal)",
        field_type: FieldType::Text,
        required: false,
        help: None,
        default_in_list: false,
    },
    FieldDef {
        slug: "submitted_to_polaris_at",
        label: "Submitted to Polaris",
        field_type: FieldType::DateTime,
        required: false,
        help: Some("Manual marker — set when the invoice is uploaded to the DOE Polaris portal."),
        default_in_list: false,
    },
    FieldDef {
        slug: "notes",
        label: "Notes",
        field_type: FieldType::LongText,
        required: false,
        help: None,
        default_in_list: false,
    },
    FieldDef {
        slug: "created_at",
        label: "Created",
        field_type: FieldType::Timestamp,
        required: false,
        help: None,
        default_in_list: false,
    },
    FieldDef {
        slug: "updated_at",
        label: "Updated",
        field_type: FieldType::Timestamp,
        required: false,
        help: None,
        default_in_list: false,
    },
];

static INVOICE: TableDef = TableDef {
    slug: "invoice",
    display_name: "Invoices",
    singular_name: "Invoice",
    display_field: "number",
    entity_kind: Some("invoice"),
    fields: INVOICE_FIELDS,
    primary: true,
    list_view: ListViewDef {
        columns: &["number", "recipient", "coach_id", "amount_cents", "status"],
        default_sort: Some("created_at"),
        default_sort_dir: SortDir::Desc,
        page_size: 50,
    },
};

// ===========================================================================
// Program delivery — the "3 A's" + Appendix F catalog (LTE_PACK_SPEC.md).
// Tables created by the pack-owned migration 0001_program_delivery.sql.
// Auto-CRUD only: no typed domain modules. Stage advancement is
// LLM/conversation-driven (track-everything; minimal surfaces), not a
// form the user fills.
// ===========================================================================

// ---------------------------------------------------------------------------
// catalog_module — the 21 priced product lines
// ---------------------------------------------------------------------------

static CATALOG_MODULE_FIELDS: &[FieldDef] = &[
    FieldDef { slug: "id", label: "ID", field_type: FieldType::Integer, required: false, help: None, default_in_list: false },
    FieldDef { slug: "line_no", label: "Line #", field_type: FieldType::Integer, required: false, help: Some("Appendix F line number (1–21)."), default_in_list: true },
    FieldDef { slug: "name", label: "Module", field_type: FieldType::Text, required: true, help: None, default_in_list: true },
    FieldDef {
        slug: "pillar",
        label: "Pillar",
        field_type: FieldType::Enum { options: &["leadership_development", "dddm_teacher_effectiveness"] },
        required: true,
        help: None,
        default_in_list: true,
    },
    FieldDef { slug: "grade_band", label: "Grade Band", field_type: FieldType::Text, required: false, help: None, default_in_list: false },
    FieldDef { slug: "audience", label: "Audience", field_type: FieldType::Text, required: false, help: None, default_in_list: false },
    FieldDef { slug: "description", label: "Description", field_type: FieldType::LongText, required: false, help: None, default_in_list: false },
    FieldDef { slug: "list_price_cents", label: "List Price", field_type: FieldType::Currency, required: false, help: Some("Total price per delivery instance (Appendix F)."), default_in_list: true },
    FieldDef { slug: "sessions", label: "Sessions", field_type: FieldType::Integer, required: false, help: None, default_in_list: false },
    FieldDef { slug: "hours_per_session", label: "Hours / Session", field_type: FieldType::Number, required: false, help: None, default_in_list: false },
    FieldDef { slug: "duration_weeks", label: "Duration (weeks)", field_type: FieldType::Integer, required: false, help: None, default_in_list: false },
    FieldDef { slug: "min_participants", label: "Min Participants", field_type: FieldType::Integer, required: false, help: None, default_in_list: false },
    FieldDef { slug: "max_participants", label: "Max Participants", field_type: FieldType::Integer, required: false, help: None, default_in_list: false },
    FieldDef { slug: "instructors_per_session", label: "Instructors / Session", field_type: FieldType::Integer, required: false, help: Some("2 for workshops, 1 for coaching."), default_in_list: false },
    FieldDef {
        slug: "kind",
        label: "Kind",
        field_type: FieldType::Enum { options: &["workshop", "coaching", "school_assessment"] },
        required: false,
        help: None,
        default_in_list: false,
    },
    FieldDef { slug: "created_at", label: "Created", field_type: FieldType::Timestamp, required: false, help: None, default_in_list: false },
    FieldDef { slug: "updated_at", label: "Updated", field_type: FieldType::Timestamp, required: false, help: None, default_in_list: false },
];

// Catalog is reference data — 21 priced modules from MTAC #R1179.
// User can edit prices when LTE re-prices, but doesn't browse daily.
// Per `feedback_minimal_surfaces` — surfaces appear in Manage only
// when actively managed. Travis answers catalog questions through
// chat (e.g. "what's the price of Data Coaching?"). Marked
// primary: false; rows still queryable via pack_query / lte_*
// tools and editable via auto-CRUD endpoints, just not a sidebar
// tab.
static CATALOG_MODULE: TableDef = TableDef {
    slug: "catalog_module",
    display_name: "Catalog",
    singular_name: "Module",
    display_field: "name",
    entity_kind: Some("module"),
    fields: CATALOG_MODULE_FIELDS,
    primary: false,
    list_view: ListViewDef {
        columns: &["line_no", "name", "pillar", "list_price_cents"],
        default_sort: Some("line_no"),
        default_sort_dir: SortDir::Asc,
        page_size: 50,
    },
};

// ---------------------------------------------------------------------------
// engagement — one 3 A's run at a school
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// contract — first-class master agreement. Engagements (and through
// them, work orders, POs, invoices) roll up under a contract for
// ceiling/expiry reporting. Backfilled from engagement.contract_ref
// strings by migration 0004.
// ---------------------------------------------------------------------------

static CONTRACT_FIELDS: &[FieldDef] = &[
    FieldDef { slug: "id", label: "ID", field_type: FieldType::Integer, required: false, help: None, default_in_list: false },
    FieldDef { slug: "ref", label: "Contract #", field_type: FieldType::Text, required: true, help: Some("The external identifier — e.g. 'QR179CF'. Must be unique within the workspace."), default_in_list: true },
    FieldDef { slug: "name", label: "Name", field_type: FieldType::Text, required: false, help: Some("Human-readable name — defaults to the contract # if not set."), default_in_list: true },
    FieldDef { slug: "counterparty", label: "Counterparty", field_type: FieldType::Text, required: false, help: Some("Who the contract is with — 'NYC DOE', a specific district, a specific school."), default_in_list: true },
    FieldDef { slug: "parent_solicitation", label: "Parent Solicitation", field_type: FieldType::Text, required: false, help: Some("The bid/MTAC/RFP this contract came out of (e.g. 'MTAC #R1179')."), default_in_list: false },
    FieldDef { slug: "term_start", label: "Term Start", field_type: FieldType::Date, required: false, help: None, default_in_list: false },
    FieldDef { slug: "term_end", label: "Term End", field_type: FieldType::Date, required: false, help: Some("Drives the contract_expiring_soon alert at 60 days out."), default_in_list: true },
    FieldDef { slug: "ceiling_cents", label: "Ceiling", field_type: FieldType::Currency, required: false, help: Some("Total billable ceiling. Drives contract_near_ceiling at 90% burn. Leave 0 to skip ceiling tracking."), default_in_list: true },
    FieldDef { slug: "signed_at", label: "Signed", field_type: FieldType::Date, required: false, help: None, default_in_list: false },
    FieldDef {
        slug: "status",
        label: "Status",
        field_type: FieldType::Enum { options: &["draft","active","expired","terminated","archived"] },
        required: true,
        help: None,
        default_in_list: true,
    },
    FieldDef { slug: "notes", label: "Notes", field_type: FieldType::LongText, required: false, help: None, default_in_list: false },
    FieldDef { slug: "pdf_path", label: "PDF Path", field_type: FieldType::Text, required: false, help: Some("Path to the executed contract document."), default_in_list: false },
    FieldDef { slug: "created_at", label: "Created", field_type: FieldType::Timestamp, required: false, help: None, default_in_list: false },
    FieldDef { slug: "updated_at", label: "Updated", field_type: FieldType::Timestamp, required: false, help: None, default_in_list: false },
];

static CONTRACT: TableDef = TableDef {
    slug: "contract",
    display_name: "Contracts (legacy)",
    singular_name: "Contract (legacy)",
    display_field: "ref",
    entity_kind: None,
    fields: CONTRACT_FIELDS,
    // Hidden from the Manage sidebar after Taylor's "engagement and
    // contract is too broad" feedback (2026-06-04). Engagement is now
    // the unified "Contract" record — the standalone contract table
    // stays for backward compat but isn't surfaced.
    primary: false,
    list_view: ListViewDef {
        columns: &["ref", "name", "counterparty", "status", "term_end", "ceiling_cents"],
        default_sort: Some("term_end"),
        default_sort_dir: SortDir::Asc,
        page_size: 50,
    },
};

// engagement IS the contract after migration 0005. UI shows "Contract"
// everywhere; the SQL table stays named `engagement` for code stability
// and backwards-compatible refs from PO/WO/invoice/etc.
static ENGAGEMENT_FIELDS: &[FieldDef] = &[
    FieldDef { slug: "id", label: "ID", field_type: FieldType::Integer, required: false, help: None, default_in_list: false },
    FieldDef { slug: "name", label: "Name", field_type: FieldType::Text, required: true, help: Some("e.g. \"PS 498 Math team coaching 2026-2027\"."), default_in_list: true },
    FieldDef { slug: "school_id", label: "School", field_type: FieldType::Ref { table: "school" }, required: false, help: None, default_in_list: true },
    FieldDef { slug: "ref", label: "Contract Ref", field_type: FieldType::Text, required: false, help: Some("Free-text reference from the PO / master agreement — e.g. \"WR260363316\"."), default_in_list: true },
    FieldDef { slug: "counterparty", label: "Counterparty", field_type: FieldType::Text, required: false, help: Some("Who issued the contract — usually the school or DOE."), default_in_list: false },
    FieldDef { slug: "ceiling_cents", label: "Total Amount", field_type: FieldType::Currency, required: false, help: Some("Total dollar value of the contract. Invoices draw down against this."), default_in_list: true },
    FieldDef { slug: "term_start", label: "Term Start", field_type: FieldType::Date, required: false, help: None, default_in_list: true },
    FieldDef { slug: "term_end", label: "Term End", field_type: FieldType::Date, required: false, help: None, default_in_list: true },
    FieldDef { slug: "signed_at", label: "Signed", field_type: FieldType::Date, required: false, help: None, default_in_list: false },
    FieldDef {
        slug: "contract_status",
        label: "Status",
        field_type: FieldType::Enum { options: &["draft", "active", "expired", "terminated", "archived"] },
        required: false,
        help: None,
        default_in_list: true,
    },
    FieldDef { slug: "parent_solicitation", label: "Parent Solicitation", field_type: FieldType::Text, required: false, help: Some("Master vehicle this contract rides on (e.g. MTAC pool ID)."), default_in_list: false },
    FieldDef { slug: "pdf_path", label: "PDF Path", field_type: FieldType::Text, required: false, help: Some("Local path to the signed contract PDF."), default_in_list: false },
    FieldDef {
        slug: "stage",
        label: "Delivery Stage",
        field_type: FieldType::Enum { options: &["assessment", "action_planning", "accountable", "closed"] },
        required: false,
        help: Some("The 3 A's lifecycle. Travis advances this from conversation — confirm, don't hand-edit."),
        default_in_list: false,
    },
    FieldDef { slug: "school_year", label: "School Year", field_type: FieldType::Text, required: false, help: Some("e.g. \"2026-2027\"."), default_in_list: false },
    FieldDef { slug: "metrics_agreement_signed", label: "Metrics Signed", field_type: FieldType::Bool, required: false, help: Some("The gate between Action Planning and delivery."), default_in_list: false },
    FieldDef { slug: "metrics_signed_on", label: "Metrics Signed On", field_type: FieldType::Date, required: false, help: None, default_in_list: false },
    FieldDef { slug: "summary", label: "Summary", field_type: FieldType::LongText, required: false, help: None, default_in_list: false },
    FieldDef { slug: "created_at", label: "Created", field_type: FieldType::Timestamp, required: false, help: None, default_in_list: false },
    FieldDef { slug: "updated_at", label: "Updated", field_type: FieldType::Timestamp, required: false, help: None, default_in_list: false },
];

static ENGAGEMENT: TableDef = TableDef {
    slug: "engagement",
    display_name: "Contracts",
    singular_name: "Contract",
    display_field: "name",
    entity_kind: Some("engagement"),
    fields: ENGAGEMENT_FIELDS,
    primary: true,
    list_view: ListViewDef {
        columns: &["name", "school_id", "ref", "ceiling_cents", "term_end", "contract_status"],
        default_sort: Some("updated_at"),
        default_sort_dir: SortDir::Desc,
        page_size: 50,
    },
};

// ---------------------------------------------------------------------------
// assessment — the A1 diagnostic (secondary; reached via engagement)
// ---------------------------------------------------------------------------

static ASSESSMENT_FIELDS: &[FieldDef] = &[
    FieldDef { slug: "id", label: "ID", field_type: FieldType::Integer, required: false, help: None, default_in_list: false },
    FieldDef { slug: "engagement_id", label: "Engagement", field_type: FieldType::Ref { table: "engagement" }, required: true, help: None, default_in_list: true },
    FieldDef {
        slug: "method",
        label: "Method",
        field_type: FieldType::Enum { options: &["leadership_survey", "personal_eval", "walkthrough", "observation", "interview", "data_analysis"] },
        required: false,
        help: None,
        default_in_list: true,
    },
    FieldDef { slug: "conducted_on", label: "Conducted On", field_type: FieldType::Date, required: false, help: None, default_in_list: true },
    FieldDef { slug: "rubric_score", label: "Rubric Score", field_type: FieldType::Number, required: false, help: None, default_in_list: false },
    FieldDef { slug: "recommended_focus", label: "Recommended Focus", field_type: FieldType::LongText, required: false, help: None, default_in_list: false },
    FieldDef { slug: "summary", label: "Summary", field_type: FieldType::LongText, required: false, help: None, default_in_list: false },
    FieldDef { slug: "created_at", label: "Created", field_type: FieldType::Timestamp, required: false, help: None, default_in_list: false },
    FieldDef { slug: "updated_at", label: "Updated", field_type: FieldType::Timestamp, required: false, help: None, default_in_list: false },
];

static ASSESSMENT: TableDef = TableDef {
    slug: "assessment",
    display_name: "Assessments",
    singular_name: "Assessment",
    display_field: "method",
    entity_kind: None,
    fields: ASSESSMENT_FIELDS,
    primary: false,
    list_view: ListViewDef {
        columns: &["engagement_id", "method", "conducted_on"],
        default_sort: Some("created_at"),
        default_sort_dir: SortDir::Desc,
        page_size: 50,
    },
};

// ---------------------------------------------------------------------------
// engagement_module — the A2 scope of work (secondary)
// ---------------------------------------------------------------------------

static ENGAGEMENT_MODULE_FIELDS: &[FieldDef] = &[
    FieldDef { slug: "id", label: "ID", field_type: FieldType::Integer, required: false, help: None, default_in_list: false },
    FieldDef { slug: "engagement_id", label: "Engagement", field_type: FieldType::Ref { table: "engagement" }, required: true, help: None, default_in_list: true },
    FieldDef { slug: "module_id", label: "Module", field_type: FieldType::Ref { table: "catalog_module" }, required: true, help: None, default_in_list: true },
    FieldDef { slug: "qty", label: "Quantity", field_type: FieldType::Number, required: false, help: Some("Number of module deliveries — e.g. '2 days Data Coaching' = 2.0."), default_in_list: true },
    FieldDef { slug: "planned_start", label: "Planned Start", field_type: FieldType::Date, required: false, help: None, default_in_list: false },
    FieldDef { slug: "planned_end", label: "Planned End", field_type: FieldType::Date, required: false, help: None, default_in_list: false },
    FieldDef { slug: "participant_count", label: "Participants", field_type: FieldType::Integer, required: false, help: None, default_in_list: true },
    FieldDef { slug: "agreed_price_cents", label: "Agreed Price", field_type: FieldType::Currency, required: false, help: Some("Defaults to the module list price; override per deal."), default_in_list: true },
    FieldDef { slug: "coaching_sessions_planned", label: "Coaching Sessions", field_type: FieldType::Integer, required: false, help: Some("~10–11 per participant for a full-year arc."), default_in_list: false },
    FieldDef { slug: "notes", label: "Notes", field_type: FieldType::LongText, required: false, help: None, default_in_list: false },
    FieldDef { slug: "created_at", label: "Created", field_type: FieldType::Timestamp, required: false, help: None, default_in_list: false },
    FieldDef { slug: "updated_at", label: "Updated", field_type: FieldType::Timestamp, required: false, help: None, default_in_list: false },
];

static ENGAGEMENT_MODULE: TableDef = TableDef {
    slug: "engagement_module",
    display_name: "Scope Items",
    singular_name: "Scope Item",
    display_field: "notes",
    entity_kind: None,
    fields: ENGAGEMENT_MODULE_FIELDS,
    primary: false,
    list_view: ListViewDef {
        columns: &["engagement_id", "module_id", "qty", "agreed_price_cents"],
        default_sort: Some("created_at"),
        default_sort_dir: SortDir::Desc,
        page_size: 50,
    },
};

// ---------------------------------------------------------------------------
// accountability_review — the A3 metrics checkpoint (secondary)
// ---------------------------------------------------------------------------

static ACCOUNTABILITY_REVIEW_FIELDS: &[FieldDef] = &[
    FieldDef { slug: "id", label: "ID", field_type: FieldType::Integer, required: false, help: None, default_in_list: false },
    FieldDef { slug: "engagement_id", label: "Engagement", field_type: FieldType::Ref { table: "engagement" }, required: true, help: None, default_in_list: true },
    FieldDef {
        slug: "period",
        label: "Period",
        field_type: FieldType::Enum { options: &["baseline_sep", "mid_jan", "reflection_jun"] },
        required: false,
        help: None,
        default_in_list: true,
    },
    FieldDef { slug: "review_date", label: "Review Date", field_type: FieldType::Date, required: false, help: None, default_in_list: true },
    FieldDef { slug: "metrics_json", label: "Metrics", field_type: FieldType::Json, required: false, help: Some("Goals / metrics / milestones + actuals."), default_in_list: false },
    FieldDef { slug: "met", label: "Met", field_type: FieldType::Bool, required: false, help: None, default_in_list: true },
    FieldDef { slug: "notes", label: "Notes", field_type: FieldType::LongText, required: false, help: None, default_in_list: false },
    FieldDef { slug: "created_at", label: "Created", field_type: FieldType::Timestamp, required: false, help: None, default_in_list: false },
    FieldDef { slug: "updated_at", label: "Updated", field_type: FieldType::Timestamp, required: false, help: None, default_in_list: false },
];

static ACCOUNTABILITY_REVIEW: TableDef = TableDef {
    slug: "accountability_review",
    display_name: "Accountability Reviews",
    singular_name: "Accountability Review",
    display_field: "period",
    entity_kind: None,
    fields: ACCOUNTABILITY_REVIEW_FIELDS,
    primary: false,
    list_view: ListViewDef {
        columns: &["engagement_id", "period", "review_date", "met"],
        default_sort: Some("review_date"),
        default_sort_dir: SortDir::Desc,
        page_size: 50,
    },
};

// ---------------------------------------------------------------------------
// quote — persisted pricing scenario (LTE_QUOTE_SPEC.md). Stores the
// inputs; margin is computed on demand by the lte_quote_margin tool
// (pricing.rs). entity_kind None — a quote is a working document, not a
// cross-pack entity.
// ---------------------------------------------------------------------------

static QUOTE_FIELDS: &[FieldDef] = &[
    FieldDef { slug: "id", label: "ID", field_type: FieldType::Integer, required: false, help: None, default_in_list: false },
    FieldDef { slug: "name", label: "Name", field_type: FieldType::Text, required: true, help: Some("e.g. \"NYCPS Algebra — 1 facilitator option\"."), default_in_list: true },
    FieldDef { slug: "module_id", label: "Module", field_type: FieldType::Ref { table: "catalog_module" }, required: true, help: None, default_in_list: true },
    FieldDef { slug: "engagement_id", label: "Engagement", field_type: FieldType::Ref { table: "engagement" }, required: false, help: Some("Optional — tie this scenario to a real deal."), default_in_list: false },
    FieldDef { slug: "participants", label: "Participants", field_type: FieldType::Integer, required: false, help: Some("Informational + per-head figures; not a cost driver."), default_in_list: true },
    FieldDef { slug: "instructors", label: "Instructors / Session", field_type: FieldType::Integer, required: false, help: Some("Override; catalog default if blank."), default_in_list: false },
    FieldDef { slug: "sessions", label: "Sessions", field_type: FieldType::Integer, required: false, help: Some("Override; catalog default if blank."), default_in_list: false },
    FieldDef { slug: "hours_per_session", label: "Hours / Session", field_type: FieldType::Number, required: false, help: Some("Override; catalog default if blank."), default_in_list: false },
    FieldDef { slug: "facilitator_rate_cents", label: "Facilitator Rate / hr", field_type: FieldType::Currency, required: false, help: Some("Default $100/hr."), default_in_list: false },
    FieldDef { slug: "ga_cents", label: "G&A", field_type: FieldType::Currency, required: false, help: Some("Flat per-delivery; default $725 (estimate)."), default_in_list: false },
    FieldDef { slug: "material_cents", label: "Materials", field_type: FieldType::Currency, required: false, help: None, default_in_list: false },
    FieldDef { slug: "rental_cents", label: "Rental", field_type: FieldType::Currency, required: false, help: None, default_in_list: false },
    FieldDef { slug: "list_price_cents", label: "List / Bid Price", field_type: FieldType::Currency, required: false, help: Some("Override the catalog list price to test a bid."), default_in_list: true },
    FieldDef { slug: "in_kind_cents", label: "In-Kind", field_type: FieldType::Currency, required: false, help: Some("Reported, not subtracted from cost."), default_in_list: false },
    FieldDef { slug: "notes", label: "Notes", field_type: FieldType::LongText, required: false, help: None, default_in_list: false },
    FieldDef { slug: "created_at", label: "Created", field_type: FieldType::Timestamp, required: false, help: None, default_in_list: false },
    FieldDef { slug: "updated_at", label: "Updated", field_type: FieldType::Timestamp, required: false, help: None, default_in_list: false },
];

static QUOTE: TableDef = TableDef {
    slug: "quote",
    display_name: "Quotes",
    singular_name: "Quote",
    display_field: "name",
    entity_kind: None,
    fields: QUOTE_FIELDS,
    primary: true,
    list_view: ListViewDef {
        columns: &["name", "module_id", "participants", "list_price_cents"],
        default_sort: Some("updated_at"),
        default_sort_dir: SortDir::Desc,
        page_size: 50,
    },
};

// ===========================================================================
// Invoicing document layer (LTE_INVOICING_SPEC.md). Tables created by
// the pack-owned migration 0003_invoicing.sql. The four documents wrap
// engagement_module scope items into the NYC DOE billing flow:
// Work Order -> Purchase Order -> Sign-in Sheet (existing) -> Invoice
// (existing + new columns + invoice_line). company_profile parameterises
// branding so a sibling consultancy can swap the row and reuse every PDF.
// ===========================================================================

// ---------------------------------------------------------------------------
// company_profile — the brand strip pulled by every PDF. Single row per
// workspace (Settings → Company; not a primary tab — only one row).
// ---------------------------------------------------------------------------

static COMPANY_PROFILE_FIELDS: &[FieldDef] = &[
    FieldDef { slug: "id", label: "ID", field_type: FieldType::Integer, required: false, help: None, default_in_list: false },
    FieldDef { slug: "name", label: "Name", field_type: FieldType::Text, required: true, help: None, default_in_list: true },
    FieldDef { slug: "legal_name", label: "Legal Name", field_type: FieldType::Text, required: false, help: Some("e.g. 'Lead to Empower LLC' — appears on the invoice 'From' block."), default_in_list: false },
    FieldDef { slug: "address_line_1", label: "Address Line 1", field_type: FieldType::Text, required: false, help: None, default_in_list: false },
    FieldDef { slug: "address_line_2", label: "Address Line 2", field_type: FieldType::Text, required: false, help: None, default_in_list: false },
    FieldDef { slug: "city", label: "City", field_type: FieldType::Text, required: false, help: None, default_in_list: false },
    FieldDef { slug: "state", label: "State", field_type: FieldType::Text, required: false, help: None, default_in_list: false },
    FieldDef { slug: "zip", label: "ZIP", field_type: FieldType::Text, required: false, help: None, default_in_list: false },
    FieldDef { slug: "phone", label: "Phone", field_type: FieldType::Phone, required: false, help: None, default_in_list: false },
    FieldDef { slug: "email", label: "Email", field_type: FieldType::Email, required: false, help: None, default_in_list: false },
    FieldDef { slug: "website", label: "Website", field_type: FieldType::Text, required: false, help: None, default_in_list: false },
    FieldDef { slug: "ein", label: "EIN", field_type: FieldType::Text, required: false, help: Some("Federal Employer Identification Number — appears on the invoice."), default_in_list: false },
    FieldDef { slug: "nyc_doe_vendor_number", label: "NYC DOE Vendor #", field_type: FieldType::Text, required: false, help: Some("e.g. 'LEA991893' — required on every PO/invoice."), default_in_list: true },
    FieldDef { slug: "default_contract_ref", label: "Default Contract #", field_type: FieldType::Text, required: false, help: Some("Pre-filled on new engagements (e.g. 'QR179CF')."), default_in_list: true },
    FieldDef { slug: "tagline", label: "Tagline", field_type: FieldType::Text, required: false, help: None, default_in_list: false },
    FieldDef { slug: "logo_path", label: "Logo Path", field_type: FieldType::Text, required: false, help: Some("Local path to a PNG/SVG for the invoice letterhead."), default_in_list: false },
    FieldDef { slug: "default_invoice_signature_authority", label: "Default Signature Authority", field_type: FieldType::Text, required: false, help: Some("Name pre-filled into the invoice 'Authorized by' block."), default_in_list: false },
    FieldDef { slug: "created_at", label: "Created", field_type: FieldType::Timestamp, required: false, help: None, default_in_list: false },
    FieldDef { slug: "updated_at", label: "Updated", field_type: FieldType::Timestamp, required: false, help: None, default_in_list: false },
];

static COMPANY_PROFILE: TableDef = TableDef {
    slug: "company_profile",
    display_name: "Company Profile",
    singular_name: "Company Profile",
    display_field: "name",
    entity_kind: None,
    fields: COMPANY_PROFILE_FIELDS,
    primary: false,
    list_view: ListViewDef {
        columns: &["name", "nyc_doe_vendor_number", "default_contract_ref"],
        default_sort: Some("name"),
        default_sort_dir: SortDir::Asc,
        page_size: 5,
    },
};

// ---------------------------------------------------------------------------
// work_order — the engagement contract artifact. Vendor-issued,
// school-countersigned. One per engagement.
// ---------------------------------------------------------------------------

static WORK_ORDER_FIELDS: &[FieldDef] = &[
    FieldDef { slug: "id", label: "ID", field_type: FieldType::Integer, required: false, help: None, default_in_list: false },
    FieldDef { slug: "engagement_id", label: "Engagement", field_type: FieldType::Ref { table: "engagement" }, required: true, help: None, default_in_list: true },
    FieldDef { slug: "contract_id", label: "Contract", field_type: FieldType::Ref { table: "contract" }, required: false, help: Some("Master agreement this WO bills against."), default_in_list: false },
    FieldDef { slug: "contract_ref", label: "Contract #", field_type: FieldType::Text, required: false, help: Some("Snapshot of the engagement's contract reference at WO time (display)."), default_in_list: true },
    FieldDef { slug: "date_issued", label: "Date Issued", field_type: FieldType::Date, required: false, help: None, default_in_list: true },
    FieldDef { slug: "vendor_signed_at", label: "Vendor Signed", field_type: FieldType::DateTime, required: false, help: None, default_in_list: false },
    FieldDef { slug: "vendor_signed_by_name", label: "Signed By (Vendor)", field_type: FieldType::Text, required: false, help: None, default_in_list: false },
    FieldDef { slug: "school_signed_at", label: "Principal Signed", field_type: FieldType::DateTime, required: false, help: Some("Date the school principal countersigned. Blank means awaiting signature."), default_in_list: true },
    FieldDef { slug: "school_signed_by_name", label: "Signed By (Principal)", field_type: FieldType::Text, required: false, help: None, default_in_list: false },
    FieldDef { slug: "total_cents", label: "Total", field_type: FieldType::Currency, required: false, help: Some("Sum of engagement_module qty × agreed_price_cents."), default_in_list: true },
    FieldDef { slug: "pdf_path", label: "PDF Path", field_type: FieldType::Text, required: false, help: Some("Path to the generated/countersigned WO PDF."), default_in_list: false },
    FieldDef { slug: "notes", label: "Notes", field_type: FieldType::LongText, required: false, help: None, default_in_list: false },
    FieldDef { slug: "created_at", label: "Created", field_type: FieldType::Timestamp, required: false, help: None, default_in_list: false },
    FieldDef { slug: "updated_at", label: "Updated", field_type: FieldType::Timestamp, required: false, help: None, default_in_list: false },
];

static WORK_ORDER: TableDef = TableDef {
    slug: "work_order",
    display_name: "Work Orders",
    singular_name: "Work Order",
    display_field: "contract_ref",
    entity_kind: None,
    fields: WORK_ORDER_FIELDS,
    primary: true,
    list_view: ListViewDef {
        columns: &["engagement_id", "date_issued", "total_cents", "school_signed_at"],
        default_sort: Some("date_issued"),
        default_sort_dir: SortDir::Desc,
        page_size: 50,
    },
};

// ---------------------------------------------------------------------------
// purchase_order — the school's authorization to bill. Inbound from DOE;
// Taylor uploads the PDF she receives. One per engagement, typically.
// ---------------------------------------------------------------------------

static PURCHASE_ORDER_FIELDS: &[FieldDef] = &[
    FieldDef { slug: "id", label: "ID", field_type: FieldType::Integer, required: false, help: None, default_in_list: false },
    FieldDef { slug: "po_number", label: "PO Number", field_type: FieldType::Text, required: true, help: Some("The 'WR…' identifier on the DOE Purchase Order."), default_in_list: true },
    FieldDef { slug: "suffix", label: "Suffix", field_type: FieldType::Text, required: false, help: Some("Almost always '01' in observed data."), default_in_list: false },
    FieldDef { slug: "tracking_number", label: "Tracking #", field_type: FieldType::Text, required: false, help: None, default_in_list: false },
    FieldDef { slug: "engagement_id", label: "Engagement", field_type: FieldType::Ref { table: "engagement" }, required: true, help: None, default_in_list: true },
    FieldDef { slug: "work_order_id", label: "Work Order", field_type: FieldType::Ref { table: "work_order" }, required: false, help: Some("The WO that triggered this PO."), default_in_list: false },
    FieldDef { slug: "contract_id", label: "Contract", field_type: FieldType::Ref { table: "contract" }, required: false, help: Some("Master agreement. Backfilled from the engagement's contract."), default_in_list: false },
    FieldDef { slug: "po_date", label: "PO Date", field_type: FieldType::Date, required: false, help: Some("DOE-side issue date on the PO."), default_in_list: true },
    FieldDef { slug: "activity_start", label: "Activity Start", field_type: FieldType::Date, required: true, help: Some("Billable window opens. Invoice periods must fall inside."), default_in_list: true },
    FieldDef { slug: "activity_end", label: "Activity End", field_type: FieldType::Date, required: true, help: Some("Billable window closes."), default_in_list: true },
    FieldDef { slug: "deliver_to_attention", label: "Deliver To (Attention)", field_type: FieldType::Text, required: false, help: None, default_in_list: false },
    FieldDef { slug: "deliver_to_address", label: "Deliver To (Address)", field_type: FieldType::Text, required: false, help: None, default_in_list: false },
    FieldDef { slug: "deliver_to_phone", label: "Deliver To (Phone)", field_type: FieldType::Phone, required: false, help: None, default_in_list: false },
    FieldDef { slug: "special_delivery", label: "Special Delivery", field_type: FieldType::LongText, required: false, help: None, default_in_list: false },
    FieldDef { slug: "authorized_by", label: "Authorized By", field_type: FieldType::Text, required: false, help: Some("Principal or DOE official who signed."), default_in_list: false },
    FieldDef { slug: "authorized_at", label: "Authorized On", field_type: FieldType::Date, required: false, help: None, default_in_list: false },
    FieldDef { slug: "total_cents", label: "Total", field_type: FieldType::Currency, required: false, help: None, default_in_list: true },
    FieldDef { slug: "pdf_path", label: "PDF Path", field_type: FieldType::Text, required: false, help: Some("Path to the PDF received from the school."), default_in_list: false },
    FieldDef { slug: "notes", label: "Notes", field_type: FieldType::LongText, required: false, help: None, default_in_list: false },
    FieldDef { slug: "created_at", label: "Created", field_type: FieldType::Timestamp, required: false, help: None, default_in_list: false },
    FieldDef { slug: "updated_at", label: "Updated", field_type: FieldType::Timestamp, required: false, help: None, default_in_list: false },
];

static PURCHASE_ORDER: TableDef = TableDef {
    slug: "purchase_order",
    display_name: "Purchase Orders",
    singular_name: "Purchase Order",
    display_field: "po_number",
    entity_kind: None,
    fields: PURCHASE_ORDER_FIELDS,
    primary: true,
    list_view: ListViewDef {
        columns: &["po_number", "engagement_id", "po_date", "activity_start", "activity_end", "total_cents"],
        default_sort: Some("po_date"),
        default_sort_dir: SortDir::Desc,
        page_size: 50,
    },
};

// ---------------------------------------------------------------------------
// invoice_line — multi-line invoice support. Reached via the invoice
// detail; not a primary tab. Validators (Slice 2) refuse draft→sent when
// unit_price_cents diverges from engagement_module.agreed_price_cents.
// ---------------------------------------------------------------------------

static INVOICE_LINE_FIELDS: &[FieldDef] = &[
    FieldDef { slug: "id", label: "ID", field_type: FieldType::Integer, required: false, help: None, default_in_list: false },
    FieldDef { slug: "invoice_id", label: "Invoice", field_type: FieldType::Ref { table: "invoice" }, required: true, help: None, default_in_list: true },
    FieldDef { slug: "engagement_module_id", label: "Scope Item", field_type: FieldType::Ref { table: "engagement_module" }, required: true, help: None, default_in_list: true },
    FieldDef { slug: "description", label: "Description", field_type: FieldType::Text, required: true, help: Some("e.g. 'DATA COACHING' — renders on the invoice PDF."), default_in_list: true },
    FieldDef { slug: "qty", label: "Qty", field_type: FieldType::Number, required: true, help: Some("How much of this scope item is billed on THIS invoice."), default_in_list: true },
    FieldDef { slug: "unit_price_cents", label: "Unit Price", field_type: FieldType::Currency, required: true, help: Some("Snapshot of engagement_module.agreed_price_cents at billing."), default_in_list: true },
    FieldDef { slug: "subtotal_cents", label: "Subtotal", field_type: FieldType::Currency, required: true, help: Some("qty × unit_price_cents."), default_in_list: true },
    FieldDef { slug: "date_list", label: "Dates", field_type: FieldType::LongText, required: false, help: Some("Rendered 'Jan: 29, Feb: 24…' string for the PDF."), default_in_list: false },
    FieldDef { slug: "sort_order", label: "Sort", field_type: FieldType::Integer, required: false, help: None, default_in_list: false },
    FieldDef { slug: "created_at", label: "Created", field_type: FieldType::Timestamp, required: false, help: None, default_in_list: false },
];

static INVOICE_LINE: TableDef = TableDef {
    slug: "invoice_line",
    display_name: "Invoice Lines",
    singular_name: "Invoice Line",
    display_field: "description",
    entity_kind: None,
    fields: INVOICE_LINE_FIELDS,
    primary: false,
    list_view: ListViewDef {
        columns: &["invoice_id", "description", "qty", "unit_price_cents", "subtotal_cents"],
        default_sort: Some("sort_order"),
        default_sort_dir: SortDir::Asc,
        page_size: 50,
    },
};

// ---------------------------------------------------------------------------
// All tables in display order. Billing spine first (existing), then the
// program-delivery surface: catalog + engagements are primary tabs;
// assessments / scope items / reviews are secondary (reached via refs);
// quotes are a primary pre-sale modeling tab. Then the invoicing
// document layer: work_order and purchase_order are primary; invoice_line
// is reached via invoice detail; company_profile is Settings-only.
// ---------------------------------------------------------------------------

pub static TABLES: &[TableDef] = &[
    COACH,
    SCHOOL,
    COACH_HOURS,
    SIGNING_SHEET,
    INVOICE,
    CATALOG_MODULE,
    CONTRACT,
    ENGAGEMENT,
    ASSESSMENT,
    ENGAGEMENT_MODULE,
    ACCOUNTABILITY_REVIEW,
    QUOTE,
    WORK_ORDER,
    PURCHASE_ORDER,
    INVOICE_LINE,
    COMPANY_PROFILE,
];
