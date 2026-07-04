//! Speech-to-text runtime (v0.22.12).
//!
//! Uses whisper.cpp via whisper-rs bindings. Model is downloaded on
//! first use into `<app_data>/speech/models/` and cached. Transcription
//! happens entirely local; no audio ever leaves the device.
//!
//! Default model: base.en (~74 MB, English-only, decent quality for
//! command-length utterances). Multilingual users can switch to
//! ggml-base.bin (also ~74 MB) — Settings UX for that lands separately.

pub mod bootstrap;
pub mod cmd;

use std::path::PathBuf;

use tauri::{AppHandle, Manager};

/// Resolve the cache path where the whisper model is stored.
pub fn cache_model_path(app: &AppHandle, model: &str) -> Result<PathBuf, String> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app_data_dir unavailable: {e}"))?;
    Ok(base.join("speech").join("models").join(model))
}

/// True if the given model is present in the cache and looks valid.
/// We do a quick file-size sanity check to catch half-downloaded files;
/// full-shot corruption would require a hash check we defer to v0.23+.
pub fn model_ready(app: &AppHandle, model: &str) -> bool {
    let path = match cache_model_path(app, model) {
        Ok(p) => p,
        Err(_) => return false,
    };
    match std::fs::metadata(&path) {
        Ok(m) => m.len() > 1_000_000, // >1MB = plausibly whole
        Err(_) => false,
    }
}
