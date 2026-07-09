//! People pack — table schema metadata for auto-CRUD UI.

use crate::packs::{FieldDef, FieldType, ListViewDef, SortDir, TableDef};

pub static TABLES: &[TableDef] = &[TableDef {
    slug: "contact",
    display_name: "People",
    singular_name: "Contact",
    display_field: "display_name",
    entity_kind: Some("person"),
    fields: &[
        FieldDef { slug: "display_name", label: "Name", field_type: FieldType::Text, required: true, help: Some("How you refer to them — 'Sarah Chen', 'Mom', 'Dr. Ellis'."), default_in_list: true },
        FieldDef { slug: "relationship", label: "Relationship", field_type: FieldType::Text, required: false, help: Some("friend, family, coworker, client, partner, other"), default_in_list: true },
        FieldDef { slug: "organization", label: "Organization", field_type: FieldType::Text, required: false, help: Some("Company, school, church."), default_in_list: true },
        FieldDef { slug: "email", label: "Email", field_type: FieldType::Text, required: false, help: None, default_in_list: false },
        FieldDef { slug: "phone", label: "Phone", field_type: FieldType::Text, required: false, help: None, default_in_list: false },
        FieldDef { slug: "birthday", label: "Birthday", field_type: FieldType::Text, required: false, help: Some("ISO date; year optional."), default_in_list: false },
        FieldDef { slug: "notes", label: "Notes", field_type: FieldType::LongText, required: false, help: Some("How you know them, what they're working on, gift ideas."), default_in_list: false },
        FieldDef { slug: "last_contact_at", label: "Last contact", field_type: FieldType::Timestamp, required: false, help: None, default_in_list: true },
    ],
    primary: true,
    list_view: ListViewDef {
        columns: &["display_name", "relationship", "organization", "last_contact_at"],
        default_sort: Some("last_contact_at"),
        default_sort_dir: SortDir::Desc,
        page_size: 100,
    },
}];
