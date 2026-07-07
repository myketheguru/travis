//! Tauri commands for speech-to-text (v0.22.12).
//!
//! Frontend records 16 kHz mono audio via AudioContext, sends the raw
//! f32 sample array to `transcribe`. Rust ensures the model is present
//! (kicking off the bootstrap loader if not) and runs whisper.cpp.

use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, State};

use crate::speech_runtime::{self, bootstrap};

pub struct SpeechBootstrapState(pub Arc<tokio::sync::Mutex<Option<bootstrap::BootstrapHandle>>>);
impl SpeechBootstrapState {
    pub fn new() -> Self {
        Self(Arc::new(tokio::sync::Mutex::new(None)))
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechStatus {
    pub ready: bool,
    pub model: String,
    pub in_progress: bool,
}

#[tauri::command]
pub async fn speech_runtime_status(
    app: AppHandle,
    state: State<'_, SpeechBootstrapState>,
) -> Result<SpeechStatus, String> {
    let model = bootstrap::DEFAULT_MODEL;
    Ok(SpeechStatus {
        ready: speech_runtime::model_ready(&app, model),
        model: model.to_string(),
        in_progress: state.0.lock().await.is_some(),
    })
}

#[tauri::command]
pub async fn speech_runtime_ensure(
    app: AppHandle,
    state: State<'_, SpeechBootstrapState>,
) -> Result<(), String> {
    let model = bootstrap::DEFAULT_MODEL;
    if speech_runtime::model_ready(&app, model) {
        return Ok(());
    }
    let mut guard = state.0.lock().await;
    if guard.is_some() {
        return Ok(());
    }
    let handle = bootstrap::BootstrapHandle::default();
    *guard = Some(handle.clone());
    drop(guard);

    let app_clone = app.clone();
    let state_arc: Arc<tokio::sync::Mutex<Option<bootstrap::BootstrapHandle>>> =
        state.0.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = bootstrap::ensure_ready(&app_clone, handle, model).await {
            tracing::warn!("speech bootstrap failed: {e}");
        }
        *state_arc.lock().await = None;
    });
    Ok(())
}

/// Transcribe raw 16 kHz mono f32 samples. Frontend is responsible for
/// getting the audio into this shape (AudioContext at 16000 Hz).
#[tauri::command]
pub async fn speech_transcribe(
    app: AppHandle,
    samples: Vec<f32>,
) -> Result<String, String> {
    let model_name = bootstrap::DEFAULT_MODEL;
    if !speech_runtime::model_ready(&app, model_name) {
        // Kick off the bootstrap synchronously — user is actively
        // trying to use the feature, we can wait for the model.
        let handle = bootstrap::BootstrapHandle::default();
        bootstrap::ensure_ready(&app, handle, model_name)
            .await
            .map_err(|e| format!("model bootstrap: {e}"))?;
    }
    let model_path = speech_runtime::cache_model_path(&app, model_name)?;
    let model_path_str = model_path.to_string_lossy().to_string();

    // Whisper runs on a blocking thread so we don't stall the async
    // runtime. The whisper-rs API is synchronous.
    let transcript = tokio::task::spawn_blocking(move || -> Result<String, String> {
        use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};
        let ctx = WhisperContext::new_with_params(
            &model_path_str,
            WhisperContextParameters::default(),
        )
        .map_err(|e| format!("load whisper model: {e}"))?;
        let mut state = ctx
            .create_state()
            .map_err(|e| format!("create whisper state: {e}"))?;
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_print_progress(false);
        params.set_print_special(false);
        params.set_print_realtime(false);
        params.set_language(Some("en"));
        params.set_translate(false);
        state
            .full(params, &samples)
            .map_err(|e| format!("whisper full: {e}"))?;

        // whisper-rs 0.16 — full_n_segments returns c_int (raw count),
        // segments accessed via get_segment(i).to_str().
        let n = state.full_n_segments();
        let mut out = String::new();
        for i in 0..n {
            if let Some(seg) = state.get_segment(i) {
                if let Ok(text) = seg.to_str() {
                    out.push_str(text);
                }
            }
        }
        Ok(out.trim().to_string())
    })
    .await
    .map_err(|e| format!("blocking task join: {e}"))??;

    Ok(transcript)
}
