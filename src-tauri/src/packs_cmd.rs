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

use crate::packs::{self, AlertSeverity, FieldType, TableDef};
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
        Some(s) => {
            let available = table
                .fields
                .iter()
                .map(|f| f.slug)
                .collect::<Vec<_>>()
                .join(", ");
            return Err(format!(
                "unknown sort field {s:?} for table {} (available: {available})",
                table.slug
            ));
        }
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

    // Workspace filter — every scoped pack table has workspace_id;
    // the auto-CRUD reads only rows from visible workspaces.
    let visible_ids = state.workspace.read().await.visible_ids.clone();
    let placeholders = workspace_in_placeholders(visible_ids.len());

    let sql = format!(
        "SELECT {columns} FROM {table} \
         WHERE workspace_id IN ({placeholders}) \
         ORDER BY {sort} {sort_dir} LIMIT {limit} OFFSET {offset}",
        table = table.slug,
    );

    let mut q = sqlx::query(&sql);
    for id in &visible_ids {
        q = q.bind(id);
    }
    let rows = q
        .fetch_all(&state.db.pool)
        .await
        .map_err(|e| format!("query {}: {e}", table.slug))?;

    let result = rows
        .iter()
        .map(|row| row_to_json(row, table.fields))
        .collect::<Vec<_>>();
    Ok(result)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableGetParams {
    pub pack_slug: String,
    pub table_slug: String,
    pub id: i64,
}

/// Auto-CRUD single-row fetch. Returns a JSON object keyed by field slug,
/// or an error if the row doesn't exist OR isn't in a visible workspace.
#[tauri::command]
pub async fn pack_table_get(
    state: State<'_, AppState>,
    params: TableGetParams,
) -> Result<serde_json::Value, String> {
    let table = lookup_table(&state, &params.pack_slug, &params.table_slug)?;
    let columns = table
        .fields
        .iter()
        .map(|f| f.slug)
        .collect::<Vec<_>>()
        .join(", ");

    let visible_ids = state.workspace.read().await.visible_ids.clone();
    let placeholders = workspace_in_placeholders(visible_ids.len());

    let sql = format!(
        "SELECT {columns} FROM {} WHERE id = ? AND workspace_id IN ({placeholders})",
        table.slug,
    );
    let mut q = sqlx::query(&sql).bind(params.id);
    for id in &visible_ids {
        q = q.bind(id);
    }
    let row = q
        .fetch_optional(&state.db.pool)
        .await
        .map_err(|e| format!("query {}: {e}", table.slug))?;
    row.map(|r| row_to_json(&r, table.fields))
        .ok_or_else(|| format!("not found: {}#{}", table.slug, params.id))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableUpsertParams {
    pub pack_slug: String,
    pub table_slug: String,
    pub payload: serde_json::Map<String, serde_json::Value>,
}

/// Auto-CRUD insert / update. Builds SQL from the table's writable fields
/// (every field except the auto-managed `id` and `Timestamp`-typed
/// columns). Spine-syncs to `entity` when the table declares
/// `entity_kind`. Returns the resulting row.
#[tauri::command]
pub async fn pack_table_upsert(
    state: State<'_, AppState>,
    params: TableUpsertParams,
) -> Result<serde_json::Value, String> {
    let table = lookup_table(&state, &params.pack_slug, &params.table_slug)?;

    // Validate required fields are present and non-null.
    for f in table.fields {
        if f.required && !is_managed_field(f.slug, &f.field_type) {
            match params.payload.get(f.slug) {
                None | Some(serde_json::Value::Null) => {
                    return Err(format!("required field '{}' is missing", f.slug));
                }
                _ => {}
            }
        }
    }

    let writable: Vec<&packs::FieldDef> = table
        .fields
        .iter()
        .filter(|f| !is_managed_field(f.slug, &f.field_type))
        .collect();

    let existing_id = params.payload.get("id").and_then(|v| v.as_i64());

    let ws = state.workspace.read().await.clone();

    let id = if let Some(existing) = existing_id {
        // Updates can only target rows in visible workspaces. The
        // workspace_id stays whatever it was — we don't move rows.
        let set_clause = writable
            .iter()
            .enumerate()
            .map(|(i, f)| format!("{} = ?{}", f.slug, i + 1))
            .collect::<Vec<_>>()
            .join(", ");
        let updated_clause = if table.fields.iter().any(|f| f.slug == "updated_at") {
            ", updated_at = CURRENT_TIMESTAMP"
        } else {
            ""
        };
        let id_placeholder = writable.len() + 1;
        let placeholders = workspace_in_placeholders(ws.visible_ids.len());
        let sql = format!(
            "UPDATE {} SET {set_clause}{updated_clause} \
             WHERE id = ?{id_placeholder} AND workspace_id IN ({placeholders})",
            table.slug,
        );

        let mut q = sqlx::query(&sql);
        for f in &writable {
            let v = params
                .payload
                .get(f.slug)
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            q = bind_for_field(q, &f.field_type, v);
        }
        q = q.bind(existing);
        for id in &ws.visible_ids {
            q = q.bind(id);
        }
        let res = q
            .execute(&state.db.pool)
            .await
            .map_err(|e| format!("update {}: {e}", table.slug))?;
        if res.rows_affected() == 0 {
            return Err(format!(
                "row {}#{} not found in any visible workspace",
                table.slug, existing
            ));
        }
        existing
    } else {
        // Inserts stamp the active workspace.
        let columns = writable
            .iter()
            .map(|f| f.slug)
            .chain(std::iter::once("workspace_id"))
            .collect::<Vec<_>>()
            .join(", ");
        let placeholders = (1..=writable.len() + 1)
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "INSERT INTO {} ({columns}) VALUES ({placeholders})",
            table.slug,
        );

        let mut q = sqlx::query(&sql);
        for f in &writable {
            let v = params
                .payload
                .get(f.slug)
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            q = bind_for_field(q, &f.field_type, v);
        }
        q = q.bind(ws.active_id);
        let result = q
            .execute(&state.db.pool)
            .await
            .map_err(|e| format!("insert {}: {e}", table.slug))?;
        result.last_insert_rowid()
    };

    // Spine sync — register the row's display field as an entity for
    // cross-pack retrieval.
    if let Some(kind) = table.entity_kind {
        let display_sql = format!(
            "SELECT {} FROM {} WHERE id = ?1",
            table.display_field, table.slug
        );
        let display_value: Result<Option<String>, _> = sqlx::query_scalar(&display_sql)
            .bind(id)
            .fetch_optional(&state.db.pool)
            .await
            .map(|opt| opt.flatten());
        if let Ok(Some(name)) = display_value {
            if let Err(e) = crate::spine::entity::upsert(
                &state.db.pool,
                crate::spine::entity::UpsertParams {
                    kind,
                    display_name: &name,
                    pack_slug: Some(params.pack_slug.as_str()),
                    attributes_json: None,
                    workspace_id: ws.active_id,
                    pack_table_id: Some(id),
                },
            )
            .await
            {
                tracing::warn!("spine entity sync ({} {}): {e}", params.pack_slug, table.slug);
            }
        }
    }

    // Return the resulting row in the same shape as pack_table_get.
    let columns = table
        .fields
        .iter()
        .map(|f| f.slug)
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!("SELECT {columns} FROM {} WHERE id = ?1", table.slug);
    let row = sqlx::query(&sql)
        .bind(id)
        .fetch_one(&state.db.pool)
        .await
        .map_err(|e| format!("refetch {}: {e}", table.slug))?;
    Ok(row_to_json(&row, table.fields))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableDeleteParams {
    pub pack_slug: String,
    pub table_slug: String,
    pub id: i64,
}

/// Operational alerts from every enabled pack. Runs each pack's
/// `alerts()` SQL and returns rows with non-zero counts. The Splash
/// screen renders these as the "metric to capitalise on" — a sharper
/// signal than raw entity counts.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlertResult {
    pub pack_slug: String,
    pub pack_name: String,
    pub alert_slug: String,
    pub label: String,
    pub severity: AlertSeverity,
    pub count: i64,
    pub sample_label: Option<String>,
    pub sample_id: Option<i64>,
}

#[tauri::command]
pub async fn pack_alerts(state: State<'_, AppState>) -> Result<Vec<AlertResult>, String> {
    let mut out = Vec::new();
    for pack in &state.enabled_packs {
        for alert in pack.alerts() {
            let row: Result<(i64, Option<String>, Option<i64>), sqlx::Error> =
                sqlx::query_as(alert.sql).fetch_one(&state.db.pool).await;
            match row {
                Ok((count, sample_label, sample_id)) => {
                    if count > 0 {
                        out.push(AlertResult {
                            pack_slug: pack.slug().to_string(),
                            pack_name: pack.name().to_string(),
                            alert_slug: alert.slug.to_string(),
                            label: alert.label.to_string(),
                            severity: alert.severity,
                            count,
                            sample_label,
                            sample_id,
                        });
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        "alert {}/{} sql failed: {e}",
                        pack.slug(),
                        alert.slug
                    );
                }
            }
        }
    }
    Ok(out)
}

/// Auto-CRUD delete. ON DELETE CASCADE / SET NULL clauses on FKs handle
/// dependent rows; pack code can override (compile a typed delete cmd) if
/// the cascade rules need to be different. Only rows in visible workspaces
/// are deletable.
#[tauri::command]
pub async fn pack_table_delete(
    state: State<'_, AppState>,
    params: TableDeleteParams,
) -> Result<(), String> {
    let table = lookup_table(&state, &params.pack_slug, &params.table_slug)?;
    let visible_ids = state.workspace.read().await.visible_ids.clone();
    let placeholders = workspace_in_placeholders(visible_ids.len());
    let sql = format!(
        "DELETE FROM {} WHERE id = ? AND workspace_id IN ({placeholders})",
        table.slug
    );
    let mut q = sqlx::query(&sql).bind(params.id);
    for id in &visible_ids {
        q = q.bind(id);
    }
    q.execute(&state.db.pool)
        .await
        .map_err(|e| format!("delete {}: {e}", table.slug))?;
    Ok(())
}

// ---------- helpers ----------

/// Build a comma-separated `?` placeholder list of length `n` for use
/// in a SQL `IN (...)` clause. Caller binds `n` values in order.
fn workspace_in_placeholders(n: usize) -> String {
    if n == 0 {
        // No visible workspaces — return a clause that matches nothing.
        // Caller's sqlx binding is empty in this case.
        return "NULL".into();
    }
    vec!["?"; n].join(", ")
}

/// Fields the auto-CRUD upsert never binds — `id` is auto-incremented;
/// `Timestamp`-typed fields are DB-managed (CURRENT_TIMESTAMP defaults
/// or the special `updated_at` clause in UPDATE).
fn is_managed_field(slug: &str, field_type: &FieldType) -> bool {
    slug == "id" || matches!(field_type, FieldType::Timestamp)
}

/// Bind a JSON value to a sqlx Query in the right SQL type for a given
/// FieldType. Always binds owned values so lifetimes don't fight us.
fn bind_for_field<'q>(
    q: sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>>,
    field_type: &FieldType,
    value: serde_json::Value,
) -> sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>> {
    use serde_json::Value;
    match field_type {
        FieldType::Integer | FieldType::Currency | FieldType::Ref { .. } => {
            q.bind(value.as_i64())
        }
        FieldType::Number => q.bind(value.as_f64()),
        FieldType::Bool => q.bind(value.as_bool()),
        FieldType::Json => match value {
            Value::Null => q.bind(None::<String>),
            other => q.bind(Some(other.to_string())),
        },
        // Every text-shaped field stores TEXT in SQLite.
        _ => q.bind(value.as_str().map(|s| s.to_string())),
    }
}

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
