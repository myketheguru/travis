//! Tauri command surface for the native voice pipeline.

use std::path::PathBuf;
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

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
    app_state: State<'_, crate::AppState>,
) -> Result<VoiceStartResult, String> {
    {
        let guard = state.inner.lock().unwrap();
        if guard.is_some() {
            return Ok(VoiceStartResult { started: true });
        }
    }
    let handle = spawn_capture(app)?;
    // v0.28.58 — restore the persisted wake-enabled state so users
    // who opted in don't have to re-toggle after every launch.
    if let Ok(Some(v)) = app_state.db.meta("voice.wake.enabled").await {
        if v == "1" {
            handle.set_wake_enabled(true);
        }
    }
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

/// v0.28.2 — arm/disarm the capture. Only armed utterances get
/// accumulated + transcribed. Manual arms from the mic button or
/// spacebar longpress. Wake word (when wired in) will also arm.
#[tauri::command]
pub async fn voice_set_armed(
    on: bool,
    state: State<'_, VoiceState>,
) -> Result<(), String> {
    if let Some(handle) = state.inner.lock().unwrap().as_ref() {
        handle.set_armed(on);
    }
    Ok(())
}

/// v0.28.58 — toggle openWakeWord ("Hey Jarvis") detection. When on,
/// the capture thread loads the ONNX model chain and runs it on
/// every 80ms of decimated audio. Firing emits `voice://wake-detected`
/// which the frontend converts into `travis:arm-voice`. Also
/// persists the preference so it survives restarts.
#[tauri::command]
pub async fn voice_set_wake_enabled(
    on: bool,
    state: State<'_, VoiceState>,
    app_state: State<'_, crate::AppState>,
) -> Result<(), String> {
    app_state
        .db
        .set_meta("voice.wake.enabled", if on { "1" } else { "0" })
        .await
        .map_err(|e| e.to_string())?;
    if let Some(handle) = state.inner.lock().unwrap().as_ref() {
        handle.set_wake_enabled(on);
    }
    Ok(())
}

/// v0.28.58 — read the persisted wake-enabled flag. Called on
/// startup to restore state + by the Settings toggle to reflect
/// the current value.
#[tauri::command]
pub async fn voice_wake_enabled(
    app_state: State<'_, crate::AppState>,
) -> Result<bool, String> {
    let v = app_state
        .db
        .meta("voice.wake.enabled")
        .await
        .map_err(|e| e.to_string())?;
    Ok(v.as_deref() == Some("1"))
}

/// v0.28.59 — external wake pause. Frontend calls this on
/// (chatBusy || activity==="thinking" || activity==="speaking") so
/// a false positive on TV/phone audio can't hijack an in-flight
/// turn. Distinct from `voice_set_wake_enabled` because we don't
/// want to tear down the worker for a 30-second LLM turn — pausing
/// keeps the ONNX chain loaded and ready to resume in <1ms.
#[tauri::command]
pub async fn voice_set_wake_paused(
    paused: bool,
    state: State<'_, VoiceState>,
) -> Result<(), String> {
    if let Some(handle) = state.inner.lock().unwrap().as_ref() {
        handle.set_wake_paused(paused);
    }
    Ok(())
}

/// v0.28.19 — voice_finalize_transcript now returns the audio path
/// and duration alongside the transcript so the frontend can render
/// an audio card the user can replay.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FinalizeResult {
    pub text: String,
    /// Absolute path to the saved WAV. Empty when nothing was
    /// captured (below the 4000-sample threshold).
    pub audio_path: String,
    pub duration_ms: u32,
}

#[tauri::command]
pub async fn voice_finalize_transcript(
    app: AppHandle,
    state: State<'_, VoiceState>,
    app_state: State<'_, crate::AppState>,
) -> Result<FinalizeResult, String> {
    let samples = {
        let guard = state.inner.lock().unwrap();
        match guard.as_ref() {
            Some(handle) => handle.take_utterance(),
            None => {
                return Ok(FinalizeResult {
                    text: String::new(),
                    audio_path: String::new(),
                    duration_ms: 0,
                });
            }
        }
    };
    if samples.len() < 4000 {
        return Ok(FinalizeResult {
            text: String::new(),
            audio_path: String::new(),
            duration_ms: 0,
        });
    }

    // v0.28.19 — save the utterance to a WAV before transcribing. If
    // whisper hiccups we still have the audio artifact linked to the
    // message via voice_utterance_link. Writes to <app_data>/voice.
    let audio_path = save_utterance_wav(&app, &samples)?;
    let duration_ms = (samples.len() as u64 * 1000 / super::capture_target_hz() as u64) as u32;

    // v0.28.19 — build the dynamic whisper seed prompt from recent
    // entities + last few user messages. Best-effort; falls back to
    // the base phrase if the DB read hiccups.
    let seeded_prompt = build_whisper_seed(&app_state.db.pool)
        .await
        .unwrap_or_else(|_| BASE_WHISPER_SEED.to_string());

    let model_name = bootstrap::DEFAULT_MODEL;
    if !speech_runtime::model_ready(&app, model_name) {
        let handle = bootstrap::BootstrapHandle::default();
        bootstrap::ensure_ready(&app, handle, model_name)
            .await
            .map_err(|e| format!("model bootstrap: {e}"))?;
    }
    let model_path = speech_runtime::cache_model_path(&app, model_name)?;
    let model_path_str = model_path.to_string_lossy().to_string();

    // v0.28.14 — use the cached WhisperContext instead of loading from
    // disk every call. First call still pays the load cost; every
    // subsequent call is warm.
    let whisper = app_state.whisper.clone();

    let transcript = tokio::task::spawn_blocking(move || -> Result<String, String> {
        use whisper_rs::{FullParams, SamplingStrategy};
        let ctx = whisper.get_or_load(&model_path_str)?;
        let mut state = ctx
            .create_state()
            .map_err(|e| format!("create whisper state: {e}"))?;
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_print_progress(false);
        params.set_print_special(false);
        params.set_print_realtime(false);
        params.set_language(Some("en"));
        params.set_translate(false);
        // v0.28.59 — cap whisper's internal thread pool. Gemini's
        // note: beyond 4 threads diminishing returns kick in and
        // steal CPU from the Tauri UI thread. min(available_cores,
        // 4) is the sweet spot per whisper.cpp bench numbers.
        let cores = std::thread::available_parallelism()
            .map(|n| n.get() as i32)
            .unwrap_or(4);
        params.set_n_threads(cores.min(4));
        // v0.28.19 — dynamic seed. Base phrase + recent entity names
        // + last user messages so proper nouns the user actually
        // talks about (Sarah Chen, PS 498, MTAC) transcribe cleanly
        // instead of getting phoneticized.
        params.set_initial_prompt(seeded_prompt.as_str());
        state
            .full(params, &samples)
            .map_err(|e| format!("whisper full: {e}"))?;
        // whisper-rs 0.16 API — n is c_int, segments via get_segment.
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
    .map_err(|e| format!("join: {e}"))??;

    let _ = app.emit("voice://transcript-final", transcript.clone());
    Ok(FinalizeResult {
        text: transcript,
        audio_path,
        duration_ms,
    })
}

/// v0.28.20 — base whisper seed. Rich, densely-worded so whisper's
/// language model sees the target vocabulary (work docs, calendar,
/// email, tasks, places, everyday errands) before transcribing the
/// user's actual audio. Extended dynamically with entity display
/// names in build_whisper_seed().
///
/// Written as natural prose because whisper's LM was trained on
/// prose; comma-separated word lists rank lower than sentences.
const BASE_WHISPER_SEED: &str = "Hey Travis. Please create an invoice, \
contract, quote, purchase order, sign-in sheet, timesheet, calendar event, \
note, reminder, memo, proposal, or report. Draft an email to my client. \
Schedule a meeting for Monday morning. What's on my calendar this week? \
Route from my office to the client's address. Add milk, eggs, and coffee \
to the grocery list. Remind me to call the dentist on Tuesday. Log this in \
my journal. Show me a map of the neighborhood.";

/// Build the whisper initial_prompt from static base + top-recent
/// entities. Capped at ~450 chars because whisper.cpp truncates
/// around ~448 text tokens.
async fn build_whisper_seed(pool: &sqlx::SqlitePool) -> Result<String, sqlx::Error> {
    use sqlx::Row;
    let mut buf = String::with_capacity(512);
    buf.push_str(BASE_WHISPER_SEED);

    // Top-6 recently-updated entities. These are the user-specific
    // proper nouns whisper doesn't know from its training set — coach
    // names, school numbers, product SKUs. Kept short to leave room
    // for the base seed which already covers everyday vocabulary.
    let entity_rows = sqlx::query(
        "SELECT display_name FROM entity
         WHERE display_name IS NOT NULL AND display_name != ''
         ORDER BY updated_at DESC LIMIT 6",
    )
    .fetch_all(pool)
    .await?;
    if !entity_rows.is_empty() {
        let mut names: Vec<String> = Vec::new();
        for row in entity_rows.iter() {
            if let Ok(n) = row.try_get::<String, _>("display_name") {
                names.push(n);
            }
        }
        if !names.is_empty() {
            buf.push_str(" Proper nouns to spell right: ");
            buf.push_str(&names.join(", "));
            buf.push('.');
        }
    }

    if buf.len() > 450 {
        buf.truncate(450);
    }
    Ok(buf)
}

/// Save f32 mono @ 16kHz samples to a WAV file under
/// <app_data>/voice/<uuid>.wav.
fn save_utterance_wav(app: &AppHandle, samples: &[f32]) -> Result<String, String> {
    let dir: PathBuf = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app_data_dir: {e}"))?
        .join("voice");
    std::fs::create_dir_all(&dir).map_err(|e| format!("mkdir voice: {e}"))?;
    let id = uuid::Uuid::new_v4().to_string();
    let path = dir.join(format!("{id}.wav"));
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: super::capture_target_hz(),
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer =
        hound::WavWriter::create(&path, spec).map_err(|e| format!("wav create: {e}"))?;
    for s in samples {
        let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        writer
            .write_sample(v)
            .map_err(|e| format!("wav write: {e}"))?;
    }
    writer.finalize().map_err(|e| format!("wav finalize: {e}"))?;
    Ok(path.to_string_lossy().to_string())
}

/// v0.28.19 — link a saved WAV to a conversation_message row. Called
/// from the frontend after journal_ingest returns the message id.
#[tauri::command]
pub async fn voice_utterance_link(
    app_state: State<'_, crate::AppState>,
    message_id: i64,
    audio_path: String,
    duration_ms: i64,
    transcript: String,
) -> Result<i64, String> {
    use sqlx::Row;
    let row = sqlx::query(
        "INSERT INTO voice_utterance (message_id, audio_path, duration_ms, transcript)
         VALUES (?1, ?2, ?3, ?4)
         RETURNING id",
    )
    .bind(message_id)
    .bind(&audio_path)
    .bind(duration_ms)
    .bind(&transcript)
    .fetch_one(&app_state.db.pool)
    .await
    .map_err(|e| format!("voice_utterance insert: {e}"))?;
    row.try_get(0).map_err(|e| format!("voice_utterance id: {e}"))
}

/// v0.28.19 — fetch the audio metadata for a message, if any.
#[tauri::command]
pub async fn voice_utterance_for_message(
    app_state: State<'_, crate::AppState>,
    message_id: i64,
) -> Result<Option<serde_json::Value>, String> {
    use sqlx::Row;
    let row = sqlx::query(
        "SELECT audio_path, duration_ms, transcript
         FROM voice_utterance
         WHERE message_id = ?1
         ORDER BY id DESC
         LIMIT 1",
    )
    .bind(message_id)
    .fetch_optional(&app_state.db.pool)
    .await
    .map_err(|e| format!("voice_utterance select: {e}"))?;
    Ok(row.map(|r| {
        serde_json::json!({
            "audioPath": r.try_get::<String, _>("audio_path").unwrap_or_default(),
            "durationMs": r.try_get::<i64, _>("duration_ms").unwrap_or(0),
            "transcript": r.try_get::<String, _>("transcript").unwrap_or_default(),
        })
    }))
}

/// v0.28.26 — synthesize speech via bundled Piper. Returns
/// base64-encoded WAV bytes so the frontend can decode + play through
/// an <audio> element without touching Tauri's asset protocol. Errors
/// are surfaced as-is; the frontend swaps to speechSynthesis on any
/// failure without user-visible drama.
#[tauri::command]
pub async fn piper_speak(app: AppHandle, text: String) -> Result<String, String> {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    match super::piper::synthesize(&app, &text).await {
        Ok(bytes) => Ok(STANDARD.encode(bytes)),
        Err(e) => Err(e.to_string()),
    }
}

/// v0.28.26 — cheap capability probe so the frontend knows whether
/// to bother calling `piper_speak` at all.
#[tauri::command]
pub fn piper_available(app: AppHandle) -> bool {
    super::piper::is_available(&app)
}
