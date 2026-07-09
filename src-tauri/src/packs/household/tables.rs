//! Household pack — table schema metadata.

use crate::packs::{FieldDef, FieldType, ListViewDef, SortDir, TableDef};

pub static TABLES: &[TableDef] = &[
    TableDef {
        slug: "grocery_item",
        display_name: "Grocery list",
        singular_name: "Grocery item",
        display_field: "name",
        entity_kind: None,
        fields: &[
            FieldDef { slug: "name", label: "Item", field_type: FieldType::Text, required: true, help: None, default_in_list: true },
            FieldDef { slug: "quantity", label: "Qty", field_type: FieldType::Text, required: false, help: Some("'2 lbs', '1 gallon', 'a bunch'"), default_in_list: true },
            FieldDef { slug: "category", label: "Category", field_type: FieldType::Text, required: false, help: Some("produce, dairy, pantry, household"), default_in_list: true },
            FieldDef { slug: "store", label: "Store", field_type: FieldType::Text, required: false, help: None, default_in_list: false },
            FieldDef { slug: "purchased_at", label: "Purchased", field_type: FieldType::Timestamp, required: false, help: None, default_in_list: true },
            FieldDef { slug: "created_at", label: "Added", field_type: FieldType::Timestamp, required: false, help: None, default_in_list: false },
        ],
        primary: true,
        list_view: ListViewDef {
            columns: &["name", "quantity", "category", "purchased_at"],
            default_sort: Some("created_at"),
            default_sort_dir: SortDir::Desc,
            page_size: 100,
        },
    },
    TableDef {
        slug: "chore",
        display_name: "Chores",
        singular_name: "Chore",
        display_field: "name",
        entity_kind: None,
        fields: &[
            FieldDef { slug: "name", label: "Chore", field_type: FieldType::Text, required: true, help: None, default_in_list: true },
            FieldDef { slug: "cadence", label: "Cadence", field_type: FieldType::Text, required: false, help: Some("daily, weekly, monthly, as-needed"), default_in_list: true },
            FieldDef { slug: "assigned_to", label: "Assigned", field_type: FieldType::Text, required: false, help: None, default_in_list: true },
            FieldDef { slug: "last_done_at", label: "Last done", field_type: FieldType::Timestamp, required: false, help: None, default_in_list: true },
            FieldDef { slug: "notes", label: "Notes", field_type: FieldType::LongText, required: false, help: None, default_in_list: false },
        ],
        primary: false,
        list_view: ListViewDef {
            columns: &["name", "cadence", "assigned_to", "last_done_at"],
            default_sort: Some("last_done_at"),
            default_sort_dir: SortDir::Asc,
            page_size: 50,
        },
    },
];
