//! Full-instance data export.
//!
//! Lets a Travis user dump everything their device knows into a
//! JSON file. Built for the pre-commercialization research
//! arrangement (consented COO observation) — Travis is a black box
//! today; this is the transparency hatch.
//!
//! Posture: the export is the user's view of their own data.
//! Sensitive bytes (oauth tokens, embedding vectors) are stripped
//! at row-construction time. Sensitive workspaces respect the
//! asymmetric isolation rule by default — opt in via the
//! `include_sensitive_workspaces` flag if the user wants the full
//! picture.

use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

use crate::packs::PackHandle;

/// Options for [`build_export`]. Defaults are conservative — full
/// dump *except* sensitive workspaces unless explicitly included.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportOptions {
    /// Include rows in workspaces with categories
    /// {health, therapy, legal, finance}. Default off; flip to true
    /// when the user wants the unredacted picture (e.g. inspecting
    /// their own instance).
    #[serde(default)]
    pub include_sensitive_workspaces: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            include_sensitive_workspaces: false,
        }
    }
}

/// Top-level export envelope. The bag is intentionally
/// conservatively-typed — keys are tables, values are arrays of
/// JSON objects matching that table's schema. Consumers can diff
/// across exports to track behaviour over time.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Export {
    pub exported_at: String,
    pub app_version: &'static str,
    pub schema_version: Option<i64>,
    /// Pack slugs that contributed schema to this export.
    pub enabled_packs: Vec<String>,
    /// True when the user opted into including sensitive
    /// workspaces. Recorded so the recipient knows whether the
    /// export is partial.
    pub include_sensitive_workspaces: bool,
    /// One key per SQLite table; value is the array of rows.
    /// `meta` carries the redaction sentinel — values for sensitive
    /// columns appear as `null` so the structure stays inspectable
    /// without leaking creds.
    pub tables: serde_json::Value,
    /// Notes about anything that was redacted or skipped — gives
    /// the recipient a clear picture of what's NOT in the export.
    pub redactions: Vec<String>,
}

/// Tables we never include (system tables, migration ledger,
/// and the affect_signal table — capability #7 wellbeing data is
/// the most sensitive thing Travis tracks; never leaves the device
/// even in an explicit user-initiated export. See BRAIN.md §6
/// failure modes ("surveillance creep").
fn system_tables() -> &'static [&'static str] {
    &["sqlite_sequence", "affect_signal"]
}

/// Tables (or specific columns) we always redact for security.
/// Format: (table, column).
fn redacted_columns() -> &'static [(&'static str, &'static str)] {
    &[
        ("oauth_account", "credentials_json"),
        ("oauth_account", "access_token"),
        ("oauth_account", "refresh_token"),
    ]
}

const SENSITIVE_CATEGORIES: &[&str] = &["health", "therapy", "legal", "finance"];

/// Build the full export.
pub async fn build_export(
    pool: &SqlitePool,
    enabled_packs: &[&dyn PackHandle],
    opts: ExportOptions,
) -> anyhow::Result<Export> {
    let tables = list_user_tables(pool).await?;

    // Compute the workspace filter once — used by every workspace-
    // scoped table dump.
    let allowed_workspace_ids: Option<Vec<i64>> = if opts.include_sensitive_workspaces {
        None
    } else {
        Some(non_sensitive_workspace_ids(pool).await.unwrap_or_default())
    };

    let mut bag = serde_json::Map::new();
    let mut redactions: Vec<String> = Vec::new();

    if !opts.include_sensitive_workspaces {
        redactions.push(
            "Sensitive workspaces (health/therapy/legal/finance) are excluded. \
             Re-export with includeSensitiveWorkspaces=true to include them."
                .into(),
        );
    }

    for table in &tables {
        let (rows, table_redactions) =
            dump_table(pool, table, allowed_workspace_ids.as_deref()).await?;
        bag.insert(table.clone(), serde_json::Value::Array(rows));
        for r in table_redactions {
            redactions.push(r);
        }
    }

    let schema_version: Option<i64> =
        match sqlx::query_as::<_, (String,)>("SELECT value FROM meta WHERE key='schema_version'")
            .fetch_optional(pool)
            .await
        {
            Ok(Some((v,))) => v.parse::<i64>().ok(),
            _ => None,
        };

    let exported_at = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let pack_slugs: Vec<String> = enabled_packs
        .iter()
        .map(|p| p.slug().to_string())
        .collect();

    Ok(Export {
        exported_at,
        app_version: env!("CARGO_PKG_VERSION"),
        schema_version,
        enabled_packs: pack_slugs,
        include_sensitive_workspaces: opts.include_sensitive_workspaces,
        tables: serde_json::Value::Object(bag),
        redactions,
    })
}

/// User-facing tables — everything that isn't internal SQLite
/// state or the sqlx migration ledger.
async fn list_user_tables(pool: &SqlitePool) -> anyhow::Result<Vec<String>> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT name FROM sqlite_master
         WHERE type='table'
           AND name NOT LIKE 'sqlite_%'
           AND name NOT LIKE 'sqlx_%'
           AND name NOT LIKE '_sqlx_%'
         ORDER BY name",
    )
    .fetch_all(pool)
    .await?;
    let skip = system_tables();
    Ok(rows
        .into_iter()
        .map(|(n,)| n)
        .filter(|n| !skip.contains(&n.as_str()))
        .collect())
}

/// Workspace ids whose category is NOT in [SENSITIVE_CATEGORIES].
async fn non_sensitive_workspace_ids(pool: &SqlitePool) -> anyhow::Result<Vec<i64>> {
    let rows: Vec<(i64,)> = sqlx::query_as(
        "SELECT id FROM workspace
         WHERE category NOT IN ('health','therapy','legal','finance')",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}

/// Dump a single table to a Vec of JSON objects. Honours the
/// workspace filter (when supplied and the table has a
/// `workspace_id` column) and the redacted-columns list. Returns
/// (rows, redaction_messages).
async fn dump_table(
    pool: &SqlitePool,
    table: &str,
    allowed_workspace_ids: Option<&[i64]>,
) -> anyhow::Result<(Vec<serde_json::Value>, Vec<String>)> {
    // Discover columns + types via PRAGMA. Each column becomes
    // (name, type) — type is what was DECLARED, which guides our
    // JSON encoding.
    let pragma_sql = format!("PRAGMA table_info(\"{table}\")");
    let cols: Vec<(i64, String, String, i64, Option<String>, i64)> =
        sqlx::query_as(&pragma_sql).fetch_all(pool).await?;
    if cols.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }
    let column_specs: Vec<(String, String)> = cols
        .into_iter()
        .map(|(_, name, ty, _, _, _)| (name, ty))
        .collect();

    let column_names: Vec<String> = column_specs.iter().map(|(n, _)| n.clone()).collect();
    let has_workspace_id = column_names.iter().any(|n| n == "workspace_id");

    // SELECT all columns. Quoted in case any is a reserved word.
    let select_cols = column_names
        .iter()
        .map(|n| format!("\"{}\"", n))
        .collect::<Vec<_>>()
        .join(", ");
    let mut sql = format!("SELECT {select_cols} FROM \"{table}\"");
    let mut binds: Vec<i64> = Vec::new();
    if has_workspace_id {
        if let Some(allowed) = allowed_workspace_ids {
            if allowed.is_empty() {
                // No allowed workspaces — return empty, but skip the
                // query so we don't generate a syntactically-broken
                // `IN ()`.
                return Ok((Vec::new(), Vec::new()));
            }
            let placeholders = (1..=allowed.len())
                .map(|i| format!("?{i}"))
                .collect::<Vec<_>>()
                .join(", ");
            sql.push_str(&format!(" WHERE workspace_id IN ({placeholders})"));
            binds = allowed.to_vec();
        }
    }

    let mut q = sqlx::query(&sql);
    for ws in &binds {
        q = q.bind(ws);
    }
    let rows = q.fetch_all(pool).await?;

    let redacted = redacted_columns();
    let mut redactions: Vec<String> = Vec::new();

    let mut out = Vec::with_capacity(rows.len());
    for row in &rows {
        let mut obj = serde_json::Map::new();
        for (col_name, col_type) in &column_specs {
            let is_redacted = redacted
                .iter()
                .any(|(t, c)| *t == table && *c == col_name);
            if is_redacted {
                obj.insert(
                    col_name.clone(),
                    serde_json::Value::String("[REDACTED]".into()),
                );
                continue;
            }
            let value = encode_value(row, col_name, col_type);
            obj.insert(col_name.clone(), value);
        }
        out.push(serde_json::Value::Object(obj));
    }

    // Note redactions for the recipient.
    if !rows.is_empty() {
        for (t, c) in redacted {
            if *t == table && column_names.iter().any(|n| n == c) {
                redactions.push(format!("{table}.{c} replaced with \"[REDACTED]\""));
            }
        }
        if column_names.iter().any(|n| n == "embedding_vector")
            || column_names.iter().any(|n| n == "vector")
        {
            redactions.push(format!(
                "{table} embedding blobs are summarised as byte length only"
            ));
        }
    }

    Ok((out, redactions))
}

/// Encode one column value to JSON based on its declared type.
/// SQLite is duck-typed in practice, so this is a best-effort
/// mapping; falls back to TEXT for anything we can't recognise.
fn encode_value(
    row: &sqlx::sqlite::SqliteRow,
    col_name: &str,
    col_type: &str,
) -> serde_json::Value {
    use serde_json::Value;
    let normalised = col_type.to_uppercase();

    if normalised.contains("BLOB") {
        // Replace blob payload with a length sentinel so the
        // export stays human-inspectable. Embedding vectors are
        // ~3 KB each — useless in JSON, expensive in size.
        return row
            .try_get::<Option<Vec<u8>>, _>(col_name)
            .ok()
            .flatten()
            .map(|b| serde_json::json!({ "_blob_bytes": b.len() }))
            .unwrap_or(Value::Null);
    }

    if normalised.contains("INT") {
        return row
            .try_get::<Option<i64>, _>(col_name)
            .ok()
            .flatten()
            .map(Value::from)
            .unwrap_or(Value::Null);
    }

    if normalised.contains("REAL") || normalised.contains("FLOAT") || normalised.contains("DOUBLE") {
        return row
            .try_get::<Option<f64>, _>(col_name)
            .ok()
            .flatten()
            .and_then(serde_json::Number::from_f64)
            .map(Value::Number)
            .unwrap_or(Value::Null);
    }

    // Default: TEXT (and anything else SQLite stores as text — TEXT,
    // VARCHAR, DATETIME, etc.). Tries i64 as a fallback for columns
    // that didn't declare a type and store integers (boolean is the
    // common case).
    if let Ok(Some(s)) = row.try_get::<Option<String>, _>(col_name) {
        return Value::String(s);
    }
    if let Ok(Some(n)) = row.try_get::<Option<i64>, _>(col_name) {
        return Value::from(n);
    }
    Value::Null
}
