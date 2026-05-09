//! Tauri command surface for full-instance data export.
//!
//! Writes a JSON file to the user's Downloads folder (falling back
//! to the app data dir if Downloads can't be resolved) and returns
//! the absolute path. The frontend renders the path with a "reveal
//! in folder" button (using tauri_plugin_opener) so the user can
//! pluck the file up and email it.

use serde::Serialize;
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

use crate::data_export::{self, ExportOptions};
use crate::AppState;

/// What the frontend gets back after a successful export.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    /// Absolute path to the file on disk.
    pub path: String,
    /// Size in bytes — UI shows a formatted size for context.
    pub size_bytes: u64,
    /// Counts so the user can see how much they're sharing at a
    /// glance: { table_name: row_count }.
    pub table_row_counts: serde_json::Value,
    /// What was redacted or skipped — surfaced in the UI for
    /// transparency.
    pub redactions: Vec<String>,
}

#[tauri::command]
pub async fn export_data(
    app: AppHandle,
    state: State<'_, AppState>,
    include_sensitive_workspaces: Option<bool>,
) -> Result<ExportResult, String> {
    let opts = ExportOptions {
        include_sensitive_workspaces: include_sensitive_workspaces.unwrap_or(false),
    };

    let export = data_export::build_export(&state.db.pool, &state.enabled_packs, opts.clone())
        .await
        .map_err(|e| format!("build export: {e}"))?;

    // Per-table row counts for the UI.
    let counts = match &export.tables {
        serde_json::Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (table, rows) in map {
                let n = rows.as_array().map(|a| a.len()).unwrap_or(0);
                out.insert(table.clone(), serde_json::Value::from(n));
            }
            serde_json::Value::Object(out)
        }
        _ => serde_json::Value::Object(serde_json::Map::new()),
    };
    let redactions = export.redactions.clone();

    // Resolve the export path. The user's Downloads folder is where
    // they expect files to land; falling back to the app data dir
    // only if Downloads can't be resolved (rare on every supported
    // OS — Windows / macOS / Linux all expose XDG-equivalent
    // Downloads).
    let exports_dir = match app.path().download_dir() {
        Ok(dir) => dir,
        Err(_) => app
            .path()
            .app_data_dir()
            .map_err(|e| format!("resolve fallback export dir: {e}"))?
            .join("exports"),
    };
    std::fs::create_dir_all(&exports_dir)
        .map_err(|e| format!("create exports dir {}: {e}", exports_dir.display()))?;

    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let suffix = if opts.include_sensitive_workspaces {
        "-full"
    } else {
        ""
    };
    let filename = format!("travis-export-{stamp}{suffix}.json");
    let path: PathBuf = exports_dir.join(&filename);

    let json = serde_json::to_string_pretty(&export)
        .map_err(|e| format!("serialize export: {e}"))?;
    std::fs::write(&path, &json)
        .map_err(|e| format!("write export to {}: {e}", path.display()))?;

    let size_bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);

    Ok(ExportResult {
        path: path.to_string_lossy().to_string(),
        size_bytes,
        table_row_counts: counts,
        redactions,
    })
}

/// Reveal an exported file in the OS file manager. Convenience for
/// the Settings UI — clicking opens the containing folder so the
/// user can pluck the file up and email it.
#[tauri::command]
pub async fn reveal_export(app: AppHandle, path: String) -> Result<(), String> {
    let p = PathBuf::from(&path);
    if !p.exists() {
        return Err(format!("file does not exist: {path}"));
    }
    let target = p
        .parent()
        .map(|d| d.to_path_buf())
        .unwrap_or(p);
    app.opener()
        .open_path(target.to_string_lossy().as_ref(), None::<&str>)
        .map_err(|e| format!("open folder: {e}"))?;
    Ok(())
}
