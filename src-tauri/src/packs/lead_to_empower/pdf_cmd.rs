//! Tauri commands for invoice PDF export.

use std::path::PathBuf;

use tauri::{Manager, State};

use crate::AppState;

#[tauri::command]
pub async fn export_invoice_pdf(
    state: State<'_, AppState>,
    invoice_id: i64,
    dest_path: String,
) -> Result<String, String> {
    let trimmed = dest_path.trim();
    if trimmed.is_empty() {
        return Err("dest_path is required".into());
    }
    let dest = PathBuf::from(trimmed);

    let profile = state
        .db
        .user_profile()
        .await
        .map_err(|e| format!("load user profile: {e}"))?
        .ok_or_else(|| {
            "user profile not set up — finish onboarding before exporting invoices".to_string()
        })?;

    let saved = super::pdf::export_invoice(&state.db.pool, invoice_id, &dest, &profile)
        .await
        .map_err(|e| format!("export invoice {invoice_id}: {e}"))?;

    Ok(saved.to_string_lossy().into_owned())
}

/// Render the invoice to a managed cache path and return the absolute path.
/// Used by the Manage UI's invoice viewer so the frontend doesn't have to
/// pick a destination or create directories.
#[tauri::command]
pub async fn export_invoice_pdf_preview(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    invoice_id: i64,
) -> Result<String, String> {
    let profile = state
        .db
        .user_profile()
        .await
        .map_err(|e| format!("load user profile: {e}"))?
        .ok_or_else(|| {
            "user profile not set up — finish onboarding before exporting invoices".to_string()
        })?;

    let cache_dir = app
        .path()
        .app_cache_dir()
        .map_err(|e| format!("resolve app cache dir: {e}"))?;
    let dir = cache_dir.join("invoices");
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("create invoice cache dir: {e}"))?;
    let dest = dir.join(format!("invoice-{invoice_id}.pdf"));

    let saved = super::pdf::export_invoice(&state.db.pool, invoice_id, &dest, &profile)
        .await
        .map_err(|e| format!("export invoice {invoice_id}: {e}"))?;

    Ok(saved.to_string_lossy().into_owned())
}
