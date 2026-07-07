//! Tauri command surface for the native voice pipeline.

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use super::capture::{spawn_capture, VoiceHandle};
use crate::speech_runtime::{self, bootstrap};

/// Managed state. Send-safe because VoiceHandle only holds a
/// mpsc::Sender + no !Send data (the cpal Stream lives on its own
/// dedicated thread).
pub struct VoiceState {
    inner: Mutex<Option<VoiceHandle>>,
}

impl VoiceState {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(None),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceStartResult {
    pub started: bool,
}

#[tauri::command]
pub async fn voice_start(
    app: AppHandle,
    state: State<'_, VoiceState>,
) -> Result<VoiceStartResult, String> {
    {
        let guard = state.inner.lock().unwrap();
        if guard.is_some() {
            return Ok(VoiceStartResult { started: true });
        }
    }
    let handle = spawn_capture(app)?;
    *state.inner.lock().unwrap() = Some(handle);
    Ok(VoiceStartResult { started: true })
}

#[tauri::command]
pub async fn voice_stop(state: State<'_, VoiceState>) -> Result<(), String> {
    let mut guard = state.inner.lock().unwrap();
    if let Some(h) = guard.as_ref() {
        h.stop();
    }
    guard.take();
    Ok(())
}

#[tauri::command]
pub async fn voice_set_barge_in(
    on: bool,
    state: State<'_, VoiceState>,
) -> Result<(), String> {
    if let Some(handle) = state.inner.lock().unwrap().as_ref() {
        handle.set_barge_in(on);
    }
    Ok(())
}

#[tauri::command]
pub async fn voice_finalize_transcript(
    app: AppHandle,
    state: State<'_, VoiceState>,
) -> Result<String, String> {
    let samples = {
        let guard = state.inner.lock().unwrap();
        match guard.as_ref() {
            Some(handle) => handle.take_utterance(),
            None => return Ok(String::new()),
        }
    };
    if samples.len() < 4000 {
        return Ok(String::new());
    }

    let model_name = bootstrap::DEFAULT_MODEL;
    if !speech_runtime::model_ready(&app, model_name) {
        let handle = bootstrap::BootstrapHandle::default();
        bootstrap::ensure_ready(&app, handle, model_name)
            .await
            .map_err(|e| format!("model bootstrap: {e}"))?;
    }
    let model_path = speech_runtime::cache_model_path(&app, model_name)?;
    let model_path_str = model_path.to_string_lossy().to_string();

    let transcript = tokio::task::spawn_blocking(move || -> Result<String, String> {
        use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};
        let ctx = WhisperContext::new_with_params(
            &model_path_str,
            WhisperContextParameters::default(),
        )
        .map_err(|e| format!("load whisper: {e}"))?;
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
        let n = state
            .full_n_segments()
            .map_err(|e| format!("whisper n_segments: {e}"))?;
        let mut out = String::new();
        for i in 0..n {
            let seg = state
                .full_get_segment_text(i)
                .map_err(|e| format!("whisper seg {i}: {e}"))?;
            out.push_str(&seg);
        }
        Ok(out.trim().to_string())
    })
    .await
    .map_err(|e| format!("join: {e}"))??;

    let _ = app.emit("voice://transcript-final", transcript.clone());
    Ok(transcript)
}
