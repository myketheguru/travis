use tauri::{AppHandle, Manager};

const OVERLAY_LABEL: &str = "overlay";

#[allow(dead_code)]
pub fn show(app: &AppHandle) -> tauri::Result<()> {
    if let Some(w) = app.get_webview_window(OVERLAY_LABEL) {
        let _ = w.center();
        w.show()?;
        w.set_focus()?;
    }
    Ok(())
}

pub fn hide(app: &AppHandle) -> tauri::Result<()> {
    if let Some(w) = app.get_webview_window(OVERLAY_LABEL) {
        w.hide()?;
    }
    Ok(())
}

pub fn toggle(app: &AppHandle) -> tauri::Result<()> {
    if let Some(w) = app.get_webview_window(OVERLAY_LABEL) {
        if w.is_visible().unwrap_or(false) {
            w.hide()?;
        } else {
            let _ = w.center();
            w.show()?;
            w.set_focus()?;
        }
    }
    Ok(())
}

#[tauri::command]
pub fn toggle_overlay(app: AppHandle) -> Result<(), String> {
    toggle(&app).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn hide_overlay(app: AppHandle) -> Result<(), String> {
    hide(&app).map_err(|e| e.to_string())
}
