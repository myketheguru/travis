//! Tauri commands for runtime pack selection.
//!
//! `list_packs` enumerates every compiled-in pack with its current
//! runtime-enabled state. `set_pack_enabled` writes the
//! `meta.pack.<slug>.enabled` flag.
//!
//! Toggling takes effect on the next app launch — action / tool
//! registries and prompt fragments are constructed once during
//! startup. The frontend surfaces a "Restart Travis" hint after a
//! successful toggle.

use serde::{Deserialize, Serialize};
use sqlx::Row;
use tauri::State;

use crate::packs::{self, FieldType, TableDef};
use crate::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackInfo {
    pub slug: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub enabled: bool,
}

#[tauri::command]
pub async fn list_packs(state: State<'_, AppState>) -> Result<Vec<PackInfo>, String> {
    let mut out = Vec::new();
    for pack in packs::compiled_in_packs() {
        let enabled = packs::is_pack_enabled(&state.db.pool, *pack)
            .await
            .map_err(|e| e.to_string())?;
        out.push(PackInfo {
            slug: pack.slug().to_string(),
            name: pack.name().to_string(),
            description: pack.description().to_string(),
            version: pack.version().to_string(),
            enabled,
        });
    }
    Ok(out)
}

#[tauri::command]
pub async fn set_pack_enabled(
    state: State<'_, AppState>,
    slug: String,
    enabled: bool,
) -> Result<(), String> {
    packs::set_pack_enabled(&state.db.pool, &slug, enabled)
        .await
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Plugin platform — schema introspection + auto-CRUD (PLUGIN_PLATFORM.md)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackSchema {
    pub slug: String,
    pub name: String,
    pub tables: Vec<TableDef>,
}

/// Schema metadata for every enabled pack — drives the frontend auto-CRUD
/// renderer (Manage tabs, ListView columns, FormView inputs). Only enabled
/// packs are returned, so disabled packs' tables don't show up in the UI.
#[tauri::command]
pub async fn pack_schemas(state: State<'_, AppState>) -> Result<Vec<PackSchema>, String> {
    Ok(state
        .enabled_packs
        .iter()
        .map(|p| PackSchema {
            slug: p.slug().to_string(),
            name: p.name().to_string(),
            tables: p.tables().to_vec(),
        })
        .collect())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableListParams {
    pub pack_slug: String,
    pub table_slug: String,
    #[serde(default)]
    pub sort: Option<String>,
    #[serde(default)]
    pub sort_dir: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
}

/// Auto-CRUD list. Builds a SELECT from the table's field metadata, runs
/// it, and returns rows as JSON objects keyed by field slug.
///
/// SQL-injection-safe: every identifier in the generated SQL (table name,
/// column names, sort field) comes from compile-time `&'static str`
/// values in the pack's `TableDef`. User-supplied params (sort, dir,
/// limit, offset) are validated against the metadata before being
/// interpolated.
#[tauri::command]
pub async fn pack_table_list(
    state: State<'_, AppState>,
    params: TableListParams,
) -> Result<Vec<serde_json::Value>, String> {
    let table = lookup_table(&state, &params.pack_slug, &params.table_slug)?;

    // Sort field — must be one of the declared field slugs.
    let requested_sort = params.sort.as_deref().or(table.list_view.default_sort);
    let sort = match requested_sort {
        Some(s) if table.fields.iter().any(|f| f.slug == s) => s,
        Some(_) => return Err(format!("unknown sort field for table {}", table.slug)),
        None => "id",
    };
    let sort_dir = match params.sort_dir.as_deref() {
        Some(s) if s.eq_ignore_ascii_case("desc") => "DESC",
        Some(s) if s.eq_ignore_ascii_case("asc") => "ASC",
        Some(_) => return Err("sort_dir must be 'asc' or 'desc'".into()),
        None => match table.list_view.default_sort_dir {
            packs::SortDir::Asc => "ASC",
            packs::SortDir::Desc => "DESC",
        },
    };

    let limit = params
        .limit
        .unwrap_or(table.list_view.page_size as i64)
        .clamp(1, 1000);
    let offset = params.offset.unwrap_or(0).max(0);

    let columns = table
        .fields
        .iter()
        .map(|f| f.slug)
        .collect::<Vec<_>>()
        .join(", ");

    let sql = format!(
        "SELECT {columns} FROM {table} ORDER BY {sort} {sort_dir} LIMIT {limit} OFFSET {offset}",
        table = table.slug,
    );

    let rows = sqlx::query(&sql)
        .fetch_all(&state.db.pool)
        .await
        .map_err(|e| format!("query {}: {e}", table.slug))?;

    let result = rows
        .iter()
        .map(|row| row_to_json(row, table.fields))
        .collect::<Vec<_>>();
    Ok(result)
}

// ---------- helpers ----------

fn lookup_table<'a>(
    state: &'a State<'_, AppState>,
    pack_slug: &str,
    table_slug: &str,
) -> Result<&'static TableDef, String> {
    let pack = state
        .enabled_packs
        .iter()
        .find(|p| p.slug() == pack_slug)
        .ok_or_else(|| format!("pack {pack_slug} is not enabled"))?;
    pack.tables()
        .iter()
        .find(|t| t.slug == table_slug)
        .ok_or_else(|| format!("table {table_slug} not declared by pack {pack_slug}"))
}

fn row_to_json(row: &sqlx::sqlite::SqliteRow, fields: &[packs::FieldDef]) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    for f in fields {
        let value = column_to_json(row, f);
        obj.insert(f.slug.to_string(), value);
    }
    serde_json::Value::Object(obj)
}

fn column_to_json(row: &sqlx::sqlite::SqliteRow, field: &packs::FieldDef) -> serde_json::Value {
    use serde_json::Value;
    match field.field_type {
        FieldType::Integer | FieldType::Currency | FieldType::Ref { .. } => {
            row.try_get::<Option<i64>, _>(field.slug)
                .ok()
                .flatten()
                .map(Value::from)
                .unwrap_or(Value::Null)
        }
        FieldType::Number => row
            .try_get::<Option<f64>, _>(field.slug)
            .ok()
            .flatten()
            .and_then(serde_json::Number::from_f64)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        FieldType::Bool => row
            .try_get::<Option<bool>, _>(field.slug)
            .ok()
            .flatten()
            .map(Value::from)
            .unwrap_or(Value::Null),
        // Every other type stores as TEXT in SQLite.
        FieldType::Text
        | FieldType::LongText
        | FieldType::Email
        | FieldType::Phone
        | FieldType::Date
        | FieldType::DateTime
        | FieldType::Enum { .. }
        | FieldType::Json
        | FieldType::Timestamp => row
            .try_get::<Option<String>, _>(field.slug)
            .ok()
            .flatten()
            .map(Value::from)
            .unwrap_or(Value::Null),
    }
}
