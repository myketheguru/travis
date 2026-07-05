//! Everyday pack — table schema metadata for auto-CRUD UI.

use crate::packs::{FieldDef, FieldType, ListViewDef, SortDir, TableDef};

pub static TABLES: &[TableDef] = &[TableDef {
    slug: "saved_place",
    display_name: "Saved places",
    singular_name: "Place",
    display_field: "name",
    entity_kind: Some("place"),
    fields: &[
        FieldDef {
            slug: "name",
            label: "Name",
            field_type: FieldType::Text,
            required: true,
            help: Some("What the user calls it — 'Dr. Chen's office'."),
            default_in_list: true,
        },
        FieldDef {
            slug: "address",
            label: "Address",
            field_type: FieldType::Text,
            required: true,
            help: Some("Human-readable street address."),
            default_in_list: true,
        },
        FieldDef {
            slug: "lat",
            label: "Latitude",
            field_type: FieldType::Number,
            required: true,
            help: None,
            default_in_list: false,
        },
        FieldDef {
            slug: "lng",
            label: "Longitude",
            field_type: FieldType::Number,
            required: true,
            help: None,
            default_in_list: false,
        },
        FieldDef {
            slug: "tags",
            label: "Tags",
            field_type: FieldType::Json,
            required: false,
            help: Some("JSON array of tags — 'clinic', 'friend', 'work'."),
            default_in_list: true,
        },
        FieldDef {
            slug: "notes",
            label: "Notes",
            field_type: FieldType::LongText,
            required: false,
            help: Some("Freeform notes — 'parking around the back'."),
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
    ],
    primary: true,
    list_view: ListViewDef {
        columns: &["name", "address", "tags"],
        default_sort: Some("created_at"),
        default_sort_dir: SortDir::Desc,
        page_size: 50,
    },
}];
