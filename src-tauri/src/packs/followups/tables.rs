//! Follow-ups pack — table schema metadata.

use crate::packs::{FieldDef, FieldType, ListViewDef, SortDir, TableDef};

pub static TABLES: &[TableDef] = &[TableDef {
    slug: "followup",
    display_name: "Follow-ups",
    singular_name: "Follow-up",
    display_field: "title",
    entity_kind: None,
    fields: &[
        FieldDef { slug: "title", label: "Title", field_type: FieldType::Text, required: true, help: Some("What you committed to — 'send Sarah the Q3 deck'."), default_in_list: true },
        FieldDef { slug: "person", label: "Person", field_type: FieldType::Text, required: false, help: Some("Who you owe this to."), default_in_list: true },
        FieldDef { slug: "due_by", label: "Due by", field_type: FieldType::Text, required: false, help: Some("Optional target date."), default_in_list: true },
        FieldDef { slug: "status", label: "Status", field_type: FieldType::Text, required: true, help: Some("open, done, dropped"), default_in_list: true },
        FieldDef { slug: "source", label: "Source", field_type: FieldType::Text, required: false, help: Some("user, ambient, inbox"), default_in_list: false },
        FieldDef { slug: "notes", label: "Notes", field_type: FieldType::LongText, required: false, help: None, default_in_list: false },
        FieldDef { slug: "created_at", label: "Captured", field_type: FieldType::Timestamp, required: false, help: None, default_in_list: false },
        FieldDef { slug: "completed_at", label: "Completed", field_type: FieldType::Timestamp, required: false, help: None, default_in_list: false },
    ],
    primary: true,
    list_view: ListViewDef {
        columns: &["title", "person", "due_by", "status"],
        default_sort: Some("created_at"),
        default_sort_dir: SortDir::Desc,
        page_size: 100,
    },
}];
