//! Native mic capture + VAD.
//!
//! A single global capture session owned by a Mutex. Only one mic
//! stream may be running at a time; start_capture kills any existing
//! stream first.
//!
//! Threading:
//! - cpal's build_input_stream runs the audio callback on a private
//!   thread it owns. We push samples into a shared Mutex-guarded
//!   VecDeque and let the tick loop (spawned as tokio task) drain +
//!   compute RMS + run VAD + emit events.
//! - We don't do heavy work inside the audio callback (allocations,
//!   IO). Just append to the buffer and update peak RMS.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use tauri::{AppHandle, Emitter};

const TARGET_HZ: u32 = 16_000;
const AMPLITUDE_EMIT_MS: u64 = 60;
/// RMS threshold above which we consider a frame to contain speech.
/// Empirical — quiet room mic RMS is ~0.001–0.005, speech is > 0.015.
const VAD_SPEECH_RMS: f32 = 0.018;
const VAD_SILENCE_RMS: f32 = 0.010;
/// How long we need continuous "above speech" energy before we call
/// it a real speech-start (vs a cough / door slam).
const VAD_ONSET_MS: u64 = 120;
/// How long we need continuous "below silence" energy after speech
/// before we call it end-of-utterance. Longer = more forgiving of
/// mid-thought pauses; shorter = snappier.
const VAD_HANGOVER_MS: u64 = 700;

/// Handle keeping the cpal stream alive. Dropping this kills the mic.
struct StreamHandle {
    _stream: cpal::Stream,
}

/// Shared state the audio callback + tick loop both touch.
struct SharedState {
    /// Samples at input_hz (whatever cpal gave us), waiting to be
    /// decimated + fed to VAD + accumulated for whisper.
    incoming: Vec<f32>,
    /// Decimated samples at 16 kHz, since the last VAD reset.
    /// Cleared on end-of-utterance after whisper consumes them.
    utterance: Vec<f32>,
    /// Peak RMS of the last window we emitted.
    last_rms: f32,
    /// Is the app currently in barge-in-listening mode (i.e. Piper is
    /// playing and we want to detect user interruption). Set by the
    /// frontend via a command.
    barge_in_arm: bool,
    /// Current VAD phase.
    vad_state: VadState,
    /// When we crossed above/below thresholds — for hysteresis timing.
    vad_edge_at: Option<Instant>,
    /// True while speech is being accumulated. Emits speech-start on
    /// transition to true, speech-end on transition to false.
    in_speech: bool,
    /// Set true when the tick loop should exit. Signalled by stop.
    shutdown: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum VadState {
    Silent,
    /// Above threshold — checking if it holds for VAD_ONSET_MS.
    ProbablySpeech,
    Speech,
    /// Below threshold — checking if silence holds VAD_HANGOVER_MS.
    ProbablySilence,
}

pub struct VoiceSession {
    _stream: StreamHandle,
    shared: Arc<Mutex<SharedState>>,
}

impl VoiceSession {
    pub fn take_utterance(&self) -> Vec<f32> {
        let mut s = self.shared.lock().unwrap();
        std::mem::take(&mut s.utterance)
    }
    pub fn set_barge_in(&self, on: bool) {
        let mut s = self.shared.lock().unwrap();
        s.barge_in_arm = on;
    }
}

impl Drop for VoiceSession {
    fn drop(&mut self) {
        self.shared.lock().unwrap().shutdown = true;
    }
}

pub fn start_capture(app: AppHandle) -> Result<VoiceSession, String> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| "no default input device".to_string())?;
    let config = device
        .default_input_config()
        .map_err(|e| format!("default input config: {e}"))?;
    let input_hz = config.sample_rate().0;
    let sample_format = config.sample_format();
    let channels = config.channels() as usize;

    tracing::info!(
        "[voice] cpal input: {:?} @ {} Hz, {} channel(s), format {:?}",
        device.name().unwrap_or_else(|_| "<unknown>".into()),
        input_hz,
        channels,
        sample_format
    );

    let shared = Arc::new(Mutex::new(SharedState {
        incoming: Vec::with_capacity(input_hz as usize),
        utterance: Vec::with_capacity(TARGET_HZ as usize * 10),
        last_rms: 0.0,
        barge_in_arm: false,
        vad_state: VadState::Silent,
        vad_edge_at: None,
        in_speech: false,
        shutdown: false,
    }));

    let shared_cb = Arc::clone(&shared);
    let stream = match sample_format {
        cpal::SampleFormat::F32 => build_stream_f32(&device, &config.into(), shared_cb, channels)?,
        cpal::SampleFormat::I16 => build_stream_i16(&device, &config.into(), shared_cb, channels)?,
        cpal::SampleFormat::U16 => build_stream_u16(&device, &config.into(), shared_cb, channels)?,
        other => return Err(format!("unsupported sample format {other:?}")),
    };
    stream
        .play()
        .map_err(|e| format!("start cpal stream: {e}"))?;

    // Tick loop — pulls incoming, decimates, runs VAD, emits events.
    let shared_tick = Arc::clone(&shared);
    let app_tick = app.clone();
    std::thread::spawn(move || {
        run_tick_loop(app_tick, shared_tick, input_hz);
    });

    Ok(VoiceSession {
        _stream: StreamHandle { _stream: stream },
        shared,
    })
}

fn build_stream_f32(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    shared: Arc<Mutex<SharedState>>,
    channels: usize,
) -> Result<cpal::Stream, String> {
    device
        .build_input_stream(
            config,
            move |data: &[f32], _| push_mono_f32(data, channels, &shared),
            move |err| tracing::warn!("[voice] cpal stream err: {err}"),
            None,
        )
        .map_err(|e| format!("build cpal stream: {e}"))
}

fn build_stream_i16(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    shared: Arc<Mutex<SharedState>>,
    channels: usize,
) -> Result<cpal::Stream, String> {
    device
        .build_input_stream(
            config,
            move |data: &[i16], _| {
                let f: Vec<f32> = data.iter().map(|s| *s as f32 / 32768.0).collect();
                push_mono_f32(&f, channels, &shared);
            },
            move |err| tracing::warn!("[voice] cpal stream err: {err}"),
            None,
        )
        .map_err(|e| format!("build cpal stream: {e}"))
}

fn build_stream_u16(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    shared: Arc<Mutex<SharedState>>,
    channels: usize,
) -> Result<cpal::Stream, String> {
    device
        .build_input_stream(
            config,
            move |data: &[u16], _| {
                let f: Vec<f32> = data
                    .iter()
                    .map(|s| (*s as f32 - 32768.0) / 32768.0)
                    .collect();
                push_mono_f32(&f, channels, &shared);
            },
            move |err| tracing::warn!("[voice] cpal stream err: {err}"),
            None,
        )
        .map_err(|e| format!("build cpal stream: {e}"))
}

/// Downmix to mono (average across channels) + append to `incoming`.
fn push_mono_f32(data: &[f32], channels: usize, shared: &Arc<Mutex<SharedState>>) {
    if channels == 0 {
        return;
    }
    let mut mono: Vec<f32> = Vec::with_capacity(data.len() / channels);
    for frame in data.chunks_exact(channels) {
        let sum: f32 = frame.iter().sum();
        mono.push(sum / channels as f32);
    }
    let mut s = shared.lock().unwrap();
    s.incoming.extend_from_slice(&mono);
}

fn run_tick_loop(app: AppHandle, shared: Arc<Mutex<SharedState>>, input_hz: u32) {
    let mut last_amp_emit = Instant::now();
    loop {
        // Sleep short enough for VAD hangover to trigger promptly.
        std::thread::sleep(Duration::from_millis(30));
        let (incoming, barge_arm, shutdown) = {
            let mut s = shared.lock().unwrap();
            if s.shutdown {
                return;
            }
            let inc = std::mem::take(&mut s.incoming);
            (inc, s.barge_in_arm, s.shutdown)
        };
        if shutdown {
            return;
        }
        if incoming.is_empty() {
            // Even with no new samples, we should still check VAD
            // hangover timing (silence timer might have crossed).
            check_vad_edge(&app, &shared, barge_arm);
            continue;
        }

        // Decimate to TARGET_HZ.
        let decimated = decimate(&incoming, input_hz, TARGET_HZ);
        let rms = rms_of(&decimated);

        {
            let mut s = shared.lock().unwrap();
            s.utterance.extend_from_slice(&decimated);
            s.last_rms = rms;
            step_vad(&app, &mut s, rms, barge_arm);
        }

        if last_amp_emit.elapsed() >= Duration::from_millis(AMPLITUDE_EMIT_MS) {
            let payload = (rms.min(0.5) * 2.0).min(1.0);
            let _ = app.emit("voice://amplitude", payload);
            last_amp_emit = Instant::now();
        }
    }
}

/// Called on ticks with no new samples so hangover timing still
/// resolves promptly.
fn check_vad_edge(app: &AppHandle, shared: &Arc<Mutex<SharedState>>, barge_arm: bool) {
    let mut s = shared.lock().unwrap();
    // Feed near-zero RMS so the state machine can progress silence.
    step_vad(app, &mut s, 0.0, barge_arm);
}

fn step_vad(app: &AppHandle, s: &mut SharedState, rms: f32, barge_arm: bool) {
    let now = Instant::now();
    match s.vad_state {
        VadState::Silent => {
            if rms >= VAD_SPEECH_RMS {
                s.vad_state = VadState::ProbablySpeech;
                s.vad_edge_at = Some(now);
            }
        }
        VadState::ProbablySpeech => {
            if rms < VAD_SILENCE_RMS {
                s.vad_state = VadState::Silent;
                s.vad_edge_at = None;
            } else if let Some(edge) = s.vad_edge_at {
                if now.duration_since(edge) >= Duration::from_millis(VAD_ONSET_MS) {
                    s.vad_state = VadState::Speech;
                    s.in_speech = true;
                    s.vad_edge_at = None;
                    let _ = app.emit("voice://speech-start", ());
                    if barge_arm {
                        let _ = app.emit("voice://barge-in", ());
                    }
                }
            }
        }
        VadState::Speech => {
            if rms < VAD_SILENCE_RMS {
                s.vad_state = VadState::ProbablySilence;
                s.vad_edge_at = Some(now);
            }
        }
        VadState::ProbablySilence => {
            if rms >= VAD_SPEECH_RMS {
                s.vad_state = VadState::Speech;
                s.vad_edge_at = None;
            } else if let Some(edge) = s.vad_edge_at {
                if now.duration_since(edge) >= Duration::from_millis(VAD_HANGOVER_MS) {
                    s.vad_state = VadState::Silent;
                    s.in_speech = false;
                    s.vad_edge_at = None;
                    let _ = app.emit("voice://speech-end", ());
                }
            }
        }
    }
}

fn rms_of(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f32 = samples.iter().map(|s| s * s).sum();
    (sum_sq / samples.len() as f32).sqrt()
}

/// Naive nearest-sample decimator. Fine for whisper — the model
/// already handles a lot of variance. Proper polyphase resampling
/// is a v0.28.1 upgrade.
fn decimate(samples: &[f32], in_hz: u32, out_hz: u32) -> Vec<f32> {
    if in_hz == out_hz {
        return samples.to_vec();
    }
    let ratio = in_hz as f64 / out_hz as f64;
    let out_len = ((samples.len() as f64) / ratio) as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src = ((i as f64) * ratio) as usize;
        if src < samples.len() {
            out.push(samples[src]);
        }
    }
    out
}
