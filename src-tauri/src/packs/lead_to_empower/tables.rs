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

static CATALOG_MODULE: TableDef = TableDef {
    slug: "catalog_module",
    display_name: "Catalog",
    singular_name: "Module",
    display_field: "name",
    entity_kind: Some("module"),
    fields: CATALOG_MODULE_FIELDS,
    primary: true,
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

static ENGAGEMENT_FIELDS: &[FieldDef] = &[
    FieldDef { slug: "id", label: "ID", field_type: FieldType::Integer, required: false, help: None, default_in_list: false },
    FieldDef { slug: "name", label: "Name", field_type: FieldType::Text, required: true, help: Some("e.g. \"Roosevelt HS — Algebra I implementation 26-27\"."), default_in_list: true },
    FieldDef { slug: "school_id", label: "School", field_type: FieldType::Ref { table: "school" }, required: false, help: None, default_in_list: true },
    FieldDef {
        slug: "stage",
        label: "Stage",
        field_type: FieldType::Enum { options: &["assessment", "action_planning", "accountable", "closed"] },
        required: false,
        help: Some("The 3 A's. Travis advances this from conversation — confirm, don't hand-edit."),
        default_in_list: true,
    },
    FieldDef { slug: "contract_ref", label: "Contract Ref", field_type: FieldType::Text, required: false, help: Some("e.g. \"MTAC R1179\" or \"NYCPS HS Math — Supt. White\"."), default_in_list: false },
    FieldDef { slug: "school_year", label: "School Year", field_type: FieldType::Text, required: false, help: Some("e.g. \"2026-2027\"."), default_in_list: false },
    FieldDef { slug: "metrics_agreement_signed", label: "Metrics Agreement Signed", field_type: FieldType::Bool, required: false, help: Some("The gate between Action Planning and delivery."), default_in_list: true },
    FieldDef { slug: "metrics_signed_on", label: "Metrics Signed On", field_type: FieldType::Date, required: false, help: None, default_in_list: false },
    FieldDef { slug: "summary", label: "Summary", field_type: FieldType::LongText, required: false, help: None, default_in_list: false },
    FieldDef { slug: "created_at", label: "Created", field_type: FieldType::Timestamp, required: false, help: None, default_in_list: false },
    FieldDef { slug: "updated_at", label: "Updated", field_type: FieldType::Timestamp, required: false, help: None, default_in_list: false },
];

static ENGAGEMENT: TableDef = TableDef {
    slug: "engagement",
    display_name: "Engagements",
    singular_name: "Engagement",
    display_field: "name",
    entity_kind: Some("engagement"),
    fields: ENGAGEMENT_FIELDS,
    primary: true,
    list_view: ListViewDef {
        columns: &["name", "school_id", "stage", "metrics_agreement_signed"],
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
        columns: &["engagement_id", "module_id", "participant_count", "agreed_price_cents"],
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

// ---------------------------------------------------------------------------
// All tables in display order. Billing spine first (existing), then the
// program-delivery surface: catalog + engagements are primary tabs;
// assessments / scope items / reviews are secondary (reached via refs);
// quotes are a primary pre-sale modeling tab.
// ---------------------------------------------------------------------------

pub static TABLES: &[TableDef] = &[
    COACH,
    SCHOOL,
    COACH_HOURS,
    SIGNING_SHEET,
    INVOICE,
    CATALOG_MODULE,
    ENGAGEMENT,
    ASSESSMENT,
    ENGAGEMENT_MODULE,
    ACCOUNTABILITY_REVIEW,
    QUOTE,
];
