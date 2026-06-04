//! Tauri commands for invoice PDF export.

use std::path::PathBuf;

use tauri::{Manager, State};

use crate::AppState;

#[tauri::command]
pub async fn export_invoice_pdf(
    app: tauri::AppHandle,
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

    // Round-trip ([[feedback-docs-first]]): register the generated PDF
    // as a document so it's re-ingestible later. Failures here log but
    // don't break the export — the file is on disk regardless.
    if let Err(e) = crate::documents::cmd::register_generated_document(
        &app,
        state.inner(),
        &saved,
        "invoice",
        Some(&format!("Invoice #{invoice_id}")),
        None,
        None,
    )
    .await
    {
        tracing::warn!("could not register generated invoice PDF: {e}");
    }

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

/// Render a Work Order to the user's Downloads folder. Single-page NYC
/// DOE layout pulling vendor block from company_profile, scope from
/// engagement_module.
#[tauri::command]
pub async fn export_work_order_pdf(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    work_order_id: i64,
) -> Result<String, String> {
    let downloads = app
        .path()
        .download_dir()
        .or_else(|_| app.path().app_data_dir())
        .map_err(|e| format!("resolve downloads dir: {e}"))?;
    std::fs::create_dir_all(&downloads)
        .map_err(|e| format!("create downloads dir: {e}"))?;
    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let dest = downloads.join(format!("lte-wo-{work_order_id}-{stamp}.pdf"));

    let saved = super::pdf::render_work_order(&state.db.pool, work_order_id, &dest)
        .await
        .map_err(|e| format!("render work order {work_order_id}: {e}"))?;

    if let Err(e) = crate::documents::cmd::register_generated_document(
        &app,
        state.inner(),
        &saved,
        "wo",
        Some(&format!("Work Order #{work_order_id}")),
        None,
        None,
    )
    .await
    {
        tracing::warn!("could not register generated WO PDF: {e}");
    }

    Ok(saved.to_string_lossy().into_owned())
}

/// Render a Sign-in Sheet for an engagement over a period. Replaces the
/// Excel cleanup dance: pulls coach_hours rows, joins them on
/// engagement_module -> catalog_module for the Scope column, totals
/// hours at the bottom.
#[tauri::command]
pub async fn export_sign_in_sheet_pdf(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    engagement_id: i64,
    period_start: String,
    period_end: String,
) -> Result<String, String> {
    let downloads = app
        .path()
        .download_dir()
        .or_else(|_| app.path().app_data_dir())
        .map_err(|e| format!("resolve downloads dir: {e}"))?;
    std::fs::create_dir_all(&downloads)
        .map_err(|e| format!("create downloads dir: {e}"))?;
    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let dest = downloads.join(format!("lte-signin-eng{engagement_id}-{stamp}.pdf"));

    let saved = super::pdf::render_sign_in_sheet(
        &state.db.pool,
        engagement_id,
        &period_start,
        &period_end,
        &dest,
    )
    .await
    .map_err(|e| format!("render sign-in sheet for engagement {engagement_id}: {e}"))?;

    if let Err(e) = crate::documents::cmd::register_generated_document(
        &app,
        state.inner(),
        &saved,
        "signed_sheet",
        Some(&format!(
            "Sign-in Sheet · eng#{engagement_id} · {period_start}..{period_end}"
        )),
        None,
        None,
    )
    .await
    {
        tracing::warn!("could not register generated sign-in sheet PDF: {e}");
    }

    Ok(saved.to_string_lossy().into_owned())
}
