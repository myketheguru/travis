//! Schema metadata for the Tutoring pack's typed tables. Drives the
//! auto-CRUD UI and generic Tauri commands (PLUGIN_PLATFORM.md).

use crate::packs::{FieldDef, FieldType, ListViewDef, SortDir, TableDef};

// ---------------------------------------------------------------------------
// tutor
// ---------------------------------------------------------------------------

static TUTOR_FIELDS: &[FieldDef] = &[
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
        slug: "phone",
        label: "Phone",
        field_type: FieldType::Phone,
        required: false,
        help: None,
        default_in_list: false,
    },
    FieldDef {
        slug: "rate_cents",
        label: "Hourly Rate",
        field_type: FieldType::Currency,
        required: false,
        help: Some("What this tutor charges per hour."),
        default_in_list: true,
    },
    FieldDef {
        slug: "subjects",
        label: "Subjects",
        field_type: FieldType::Text,
        required: false,
        help: Some("Comma-separated, e.g. \"Math, Reading, Spanish\"."),
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

static TUTOR: TableDef = TableDef {
    slug: "tutor",
    display_name: "Tutors",
    singular_name: "Tutor",
    display_field: "name",
    entity_kind: Some("tutor"),
    fields: TUTOR_FIELDS,
    primary: true,
    list_view: ListViewDef {
        columns: &["name", "email", "subjects", "rate_cents"],
        default_sort: Some("name"),
        default_sort_dir: SortDir::Asc,
        page_size: 50,
    },
};

// ---------------------------------------------------------------------------
// student
// ---------------------------------------------------------------------------

static STUDENT_FIELDS: &[FieldDef] = &[
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
        slug: "grade",
        label: "Grade",
        field_type: FieldType::Text,
        required: false,
        help: None,
        default_in_list: true,
    },
    FieldDef {
        slug: "parent_name",
        label: "Parent Name",
        field_type: FieldType::Text,
        required: false,
        help: None,
        default_in_list: true,
    },
    FieldDef {
        slug: "parent_email",
        label: "Parent Email",
        field_type: FieldType::Email,
        required: false,
        help: None,
        default_in_list: false,
    },
    FieldDef {
        slug: "parent_phone",
        label: "Parent Phone",
        field_type: FieldType::Phone,
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

static STUDENT: TableDef = TableDef {
    slug: "student",
    display_name: "Students",
    singular_name: "Student",
    display_field: "name",
    entity_kind: Some("student"),
    fields: STUDENT_FIELDS,
    primary: true,
    list_view: ListViewDef {
        columns: &["name", "grade", "parent_name"],
        default_sort: Some("name"),
        default_sort_dir: SortDir::Asc,
        page_size: 50,
    },
};

// ---------------------------------------------------------------------------
// session
// ---------------------------------------------------------------------------

static SESSION_FIELDS: &[FieldDef] = &[
    FieldDef {
        slug: "id",
        label: "ID",
        field_type: FieldType::Integer,
        required: false,
        help: None,
        default_in_list: false,
    },
    FieldDef {
        slug: "tutor_id",
        label: "Tutor",
        field_type: FieldType::Ref { table: "tutor" },
        required: true,
        help: None,
        default_in_list: true,
    },
    FieldDef {
        slug: "student_id",
        label: "Student",
        field_type: FieldType::Ref { table: "student" },
        required: true,
        help: None,
        default_in_list: true,
    },
    FieldDef {
        slug: "subject",
        label: "Subject",
        field_type: FieldType::Text,
        required: false,
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
        slug: "duration_minutes",
        label: "Duration (min)",
        field_type: FieldType::Integer,
        required: true,
        help: None,
        default_in_list: true,
    },
    FieldDef {
        slug: "status",
        label: "Status",
        field_type: FieldType::Enum {
            options: &["scheduled", "completed", "cancelled", "no_show"],
        },
        required: true,
        help: Some("Only `completed` sessions count toward billing."),
        default_in_list: true,
    },
    FieldDef {
        slug: "notes",
        label: "Notes",
        field_type: FieldType::LongText,
        required: false,
        help: Some("What was covered in the session."),
        default_in_list: false,
    },
    FieldDef {
        slug: "homework",
        label: "Homework",
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

static SESSION: TableDef = TableDef {
    slug: "session",
    display_name: "Sessions",
    singular_name: "Session",
    display_field: "session_date",
    entity_kind: None,
    fields: SESSION_FIELDS,
    primary: true,
    list_view: ListViewDef {
        columns: &[
            "session_date",
            "tutor_id",
            "student_id",
            "subject",
            "duration_minutes",
            "status",
        ],
        default_sort: Some("session_date"),
        default_sort_dir: SortDir::Desc,
        page_size: 100,
    },
};

// ---------------------------------------------------------------------------
// progress_report
// ---------------------------------------------------------------------------

static PROGRESS_REPORT_FIELDS: &[FieldDef] = &[
    FieldDef {
        slug: "id",
        label: "ID",
        field_type: FieldType::Integer,
        required: false,
        help: None,
        default_in_list: false,
    },
    FieldDef {
        slug: "student_id",
        label: "Student",
        field_type: FieldType::Ref { table: "student" },
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
        slug: "content",
        label: "Content",
        field_type: FieldType::LongText,
        required: true,
        help: None,
        default_in_list: false,
    },
    FieldDef {
        slug: "sent_at",
        label: "Sent",
        field_type: FieldType::DateTime,
        required: false,
        help: None,
        default_in_list: true,
    },
    FieldDef {
        slug: "sent_to",
        label: "Sent To",
        field_type: FieldType::Email,
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

static PROGRESS_REPORT: TableDef = TableDef {
    slug: "progress_report",
    display_name: "Progress Reports",
    singular_name: "Progress Report",
    display_field: "period_end",
    entity_kind: None,
    fields: PROGRESS_REPORT_FIELDS,
    primary: true,
    list_view: ListViewDef {
        columns: &["student_id", "period_start", "period_end", "sent_at"],
        default_sort: Some("period_end"),
        default_sort_dir: SortDir::Desc,
        page_size: 50,
    },
};

// ---------------------------------------------------------------------------

pub static TABLES: &[TableDef] = &[TUTOR, STUDENT, SESSION, PROGRESS_REPORT];
