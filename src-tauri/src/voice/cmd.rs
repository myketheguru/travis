//! Tauri command surface for the native voice pipeline.

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use super::capture::{start_capture, VoiceSession};
use crate::speech_runtime::{self, bootstrap};

/// State container the plugin manages. Holds the single active
/// VoiceSession (or None when the mic isn't running).
pub struct VoiceState(pub Mutex<Option<VoiceSession>>);

impl VoiceState {
    pub fn new() -> Self {
        Self(Mutex::new(None))
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceStartResult {
    pub started: bool,
}

/// Start native mic capture. If already running, this is a no-op that
/// still returns started: true so the frontend can idempotently ensure
/// the mic is armed.
#[tauri::command]
pub async fn voice_start(
    app: AppHandle,
    state: State<'_, VoiceState>,
) -> Result<VoiceStartResult, String> {
    {
        let guard = state.0.lock().unwrap();
        if guard.is_some() {
            return Ok(VoiceStartResult { started: true });
        }
    }
    let session = start_capture(app.clone())?;
    *state.0.lock().unwrap() = Some(session);
    Ok(VoiceStartResult { started: true })
}

/// Stop native mic capture. Drops the cpal stream + tick loop.
#[tauri::command]
pub async fn voice_stop(state: State<'_, VoiceState>) -> Result<(), String> {
    let mut guard = state.0.lock().unwrap();
    guard.take();
    Ok(())
}

/// Toggle barge-in arm. When true, speech-start events also emit
/// voice://barge-in so the frontend can stop Piper mid-playback.
#[tauri::command]
pub async fn voice_set_barge_in(
    on: bool,
    state: State<'_, VoiceState>,
) -> Result<(), String> {
    if let Some(session) = state.0.lock().unwrap().as_ref() {
        session.set_barge_in(on);
    }
    Ok(())
}

/// Consume the currently-buffered utterance samples and run whisper
/// on them synchronously (blocking task). Returns the transcript.
/// Called by the frontend after voice://speech-end fires.
#[tauri::command]
pub async fn voice_finalize_transcript(
    app: AppHandle,
    state: State<'_, VoiceState>,
) -> Result<String, String> {
    let samples = {
        let guard = state.0.lock().unwrap();
        match guard.as_ref() {
            Some(session) => session.take_utterance(),
            None => return Ok(String::new()),
        }
    };
    if samples.len() < 4000 {
        // < 250ms of audio at 16kHz. Almost certainly noise or a
        // spurious VAD trip — skip the whisper round-trip.
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

    // Emit the transcript as an event too so any listener can react.
    let _ = app.emit("voice://transcript-final", transcript.clone());
    Ok(transcript)
}
