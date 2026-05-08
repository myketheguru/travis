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

// ---------------------------------------------------------------------------
// All tables in display order.
// ---------------------------------------------------------------------------

pub static TABLES: &[TableDef] = &[COACH, SCHOOL, COACH_HOURS, SIGNING_SHEET, INVOICE];
