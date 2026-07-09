//! Finance pack — table schema metadata.

use crate::packs::{FieldDef, FieldType, ListViewDef, SortDir, TableDef};

pub static TABLES: &[TableDef] = &[
    TableDef {
        slug: "bill",
        display_name: "Bills",
        singular_name: "Bill",
        display_field: "name",
        entity_kind: None,
        fields: &[
            FieldDef { slug: "name", label: "Bill", field_type: FieldType::Text, required: true, help: Some("Provider or purpose."), default_in_list: true },
            FieldDef { slug: "amount_cents", label: "Amount", field_type: FieldType::Number, required: false, help: Some("In cents."), default_in_list: true },
            FieldDef { slug: "cadence", label: "Cadence", field_type: FieldType::Text, required: true, help: Some("monthly, quarterly, yearly, one-time"), default_in_list: true },
            FieldDef { slug: "next_due_at", label: "Next due", field_type: FieldType::Text, required: false, help: None, default_in_list: true },
            FieldDef { slug: "autopay", label: "Autopay", field_type: FieldType::Bool, required: false, help: None, default_in_list: true },
            FieldDef { slug: "paid_last_at", label: "Paid last", field_type: FieldType::Timestamp, required: false, help: None, default_in_list: false },
            FieldDef { slug: "notes", label: "Notes", field_type: FieldType::LongText, required: false, help: None, default_in_list: false },
        ],
        primary: true,
        list_view: ListViewDef {
            columns: &["name", "amount_cents", "cadence", "next_due_at", "autopay"],
            default_sort: Some("next_due_at"),
            default_sort_dir: SortDir::Asc,
            page_size: 100,
        },
    },
    TableDef {
        slug: "subscription",
        display_name: "Subscriptions",
        singular_name: "Subscription",
        display_field: "name",
        entity_kind: None,
        fields: &[
            FieldDef { slug: "name", label: "Service", field_type: FieldType::Text, required: true, help: None, default_in_list: true },
            FieldDef { slug: "amount_cents", label: "Amount", field_type: FieldType::Number, required: false, help: Some("In cents."), default_in_list: true },
            FieldDef { slug: "cadence", label: "Cadence", field_type: FieldType::Text, required: true, help: None, default_in_list: true },
            FieldDef { slug: "category", label: "Category", field_type: FieldType::Text, required: false, help: Some("streaming, software, health, etc."), default_in_list: true },
            FieldDef { slug: "status", label: "Status", field_type: FieldType::Text, required: true, help: Some("active, cancelled, paused"), default_in_list: true },
            FieldDef { slug: "next_renewal_at", label: "Renews", field_type: FieldType::Text, required: false, help: None, default_in_list: true },
            FieldDef { slug: "notes", label: "Notes", field_type: FieldType::LongText, required: false, help: None, default_in_list: false },
        ],
        primary: false,
        list_view: ListViewDef {
            columns: &["name", "amount_cents", "cadence", "category", "status"],
            default_sort: Some("next_renewal_at"),
            default_sort_dir: SortDir::Asc,
            page_size: 100,
        },
    },
];
