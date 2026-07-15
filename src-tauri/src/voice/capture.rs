//! Native mic capture + VAD.
//!
//! Threading: `cpal::Stream` is `!Send` (audio drivers assume single-
//! thread ownership). Tauri command state requires `Send + Sync`, so
//! we can't stash the Stream in state directly. Instead we spawn a
//! dedicated worker thread that owns the Stream for its whole
//! lifetime and expose only a mpsc control channel to the rest of the
//! app. The channel Sender is Send + Sync.

use std::sync::{
    mpsc::{channel, Receiver, Sender},
    Arc, Mutex,
};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use tauri::{AppHandle, Emitter};

const TARGET_HZ: u32 = 16_000;
const AMPLITUDE_EMIT_MS: u64 = 60;
// v0.28.57 — bumped from 0.008/0.004 to 0.014/0.007 after users
// reported the auto-arm window picking up "background noise" as
// speech. 0.008 was tuned to catch quiet whisper-level speech at
// arm's length, but that band overlaps typical office ambient (open
// window, HVAC start-up, another person talking in the next room).
// 0.014 still triggers on normal desk-distance speech (~0.03-0.08
// RMS in testing) while skipping the ambient band. If quiet mumbling
// stops triggering, lower this in 0.002 increments.
const VAD_SPEECH_RMS: f32 = 0.014;
const VAD_SILENCE_RMS: f32 = 0.007;
const VAD_ONSET_MS: u64 = 100;
// v0.28.18 — bumped from 700ms to 2500ms. 700ms was tripping on
// natural mid-sentence pauses; 2500ms lets people finish sentences
// but still auto-ends within a few seconds of them being done.
const VAD_HANGOVER_MS: u64 = 2500;

/// Control commands sent from the Tauri command layer to the worker
/// thread that owns the cpal Stream. Each variant that needs a reply
/// carries a reply channel.
enum Cmd {
    Stop,
    SetBargeIn(bool),
    /// v0.28.2 — arm/disarm capture. When armed, the next speech-end
    /// fires voice://transcript-ready and the accumulated utterance
    /// is kept for finalizeTranscript. When unarmed, VAD still runs
    /// (for wake-word + amplitude events) but the utterance is
    /// discarded on end-of-speech so no wasted whisper cycles + no
    /// unintended submissions.
    SetArmed(bool),
    /// v0.28.58 — arm/disarm the openWakeWord detector. Independent
    /// of `SetArmed`: wake stays on whenever the user has toggled
    /// wake-word capture in Settings, regardless of whether an
    /// intent capture is in flight. When it fires, the frontend
    /// dispatches `travis:arm-voice`, which flips SetArmed(true).
    SetWakeEnabled(bool),
    TakeUtterance(Sender<Vec<f32>>),
}

/// Send-safe handle stashed in Tauri state. Only holds a Sender + a
/// join guard for the worker thread.
pub struct VoiceHandle {
    tx: Sender<Cmd>,
    // JoinHandle isn't strictly needed but keeping it lets us know the
    // worker is still alive. Dropping VoiceHandle sends Stop; the
    // thread returns; nothing to join on the drop path though.
}

impl VoiceHandle {
    pub fn take_utterance(&self) -> Vec<f32> {
        let (tx, rx) = channel::<Vec<f32>>();
        if self.tx.send(Cmd::TakeUtterance(tx)).is_err() {
            return Vec::new();
        }
        rx.recv_timeout(Duration::from_secs(2)).unwrap_or_default()
    }
    pub fn set_barge_in(&self, on: bool) {
        let _ = self.tx.send(Cmd::SetBargeIn(on));
    }
    pub fn set_armed(&self, on: bool) {
        let _ = self.tx.send(Cmd::SetArmed(on));
    }
    pub fn set_wake_enabled(&self, on: bool) {
        let _ = self.tx.send(Cmd::SetWakeEnabled(on));
    }
    pub fn stop(&self) {
        let _ = self.tx.send(Cmd::Stop);
    }
}

impl Drop for VoiceHandle {
    fn drop(&mut self) {
        let _ = self.tx.send(Cmd::Stop);
    }
}

/// Shared audio buffer state — used only inside the worker thread and
/// the cpal callback. Not exposed outside this file.
struct AudioBuf {
    incoming: Vec<f32>,
}

/// Spawn the worker thread that owns the cpal Stream + runs the tick
/// loop. Returns a Send handle for the Tauri command layer.
pub fn spawn_capture(app: AppHandle) -> Result<VoiceHandle, String> {
    let (tx, rx) = channel::<Cmd>();
    let (started_tx, started_rx) = channel::<Result<(), String>>();

    let started_tx_thread = started_tx.clone();
    std::thread::spawn(move || {
        if let Err(e) = capture_thread_main(app, rx, started_tx_thread.clone()) {
            // If capture_thread_main errored before signalling start,
            // relay the error to the outer waiter.
            let _ = started_tx_thread.send(Err(e));
        }
    });

    // Wait for the worker to signal 'stream up' (fires from inside
    // capture_thread_main once stream.play() succeeds) or an error.
    // v0.28.1 fix — previously the send never fired until Stop, so
    // this always timed out at 3s even when cpal was fine.
    match started_rx.recv_timeout(Duration::from_secs(5)) {
        Ok(Ok(())) => Ok(VoiceHandle { tx }),
        Ok(Err(e)) => Err(e),
        Err(_) => Err("cpal init timed out".to_string()),
    }
}

fn capture_thread_main(
    app: AppHandle,
    rx: Receiver<Cmd>,
    started_tx: Sender<Result<(), String>>,
) -> Result<(), String> {
    let host = cpal::default_host();

    // v0.28.1 — enumerate all input devices for diagnostics so we can
    // tell whether Windows is defaulting us to a bad device (loopback,
    // 'Stereo Mix', disconnected mic).
    if let Ok(devices) = host.input_devices() {
        for (idx, d) in devices.enumerate() {
            tracing::info!(
                "[voice] input device #{}: {:?}",
                idx,
                d.name().unwrap_or_else(|_| "<unknown>".into())
            );
        }
    }

    // v0.28.1 — Windows commonly defaults to "Stereo Mix" (system
    // audio loopback) which records NOTHING when nothing is playing.
    // Prefer a real mic if the default looks like a loopback.
    let device = pick_input_device(&host)?;
    let config = device
        .default_input_config()
        .map_err(|e| format!("default input config: {e}"))?;
    let input_hz = config.sample_rate().0;
    let sample_format = config.sample_format();
    let channels = config.channels() as usize;

    tracing::info!(
        "[voice] cpal SELECTED input: {:?} @ {} Hz, {} channel(s), format {:?}",
        device.name().unwrap_or_else(|_| "<unknown>".into()),
        input_hz,
        channels,
        sample_format
    );

    let audio = Arc::new(Mutex::new(AudioBuf {
        incoming: Vec::with_capacity(input_hz as usize),
    }));

    let audio_cb = Arc::clone(&audio);
    let stream = match sample_format {
        cpal::SampleFormat::F32 => build_stream_f32(&device, &config.into(), audio_cb, channels)?,
        cpal::SampleFormat::I16 => build_stream_i16(&device, &config.into(), audio_cb, channels)?,
        cpal::SampleFormat::U16 => build_stream_u16(&device, &config.into(), audio_cb, channels)?,
        other => return Err(format!("unsupported sample format {other:?}")),
    };
    stream
        .play()
        .map_err(|e| format!("start cpal stream: {e}"))?;

    // v0.28.1 — signal 'started' to the outer spawn_capture waiter
    // NOW that the stream is playing. Previously this send happened
    // AFTER tick_loop returned (i.e. on Cmd::Stop), so voice_start
    // always timed out at 3s even though cpal was fine.
    let _ = started_tx.send(Ok(()));

    tick_loop(app, audio, rx, input_hz);
    drop(stream);
    Ok(())
}

/// v0.28.1 — Windows quirk: `default_input_device()` frequently
/// returns "Stereo Mix" (a system-audio loopback) even when a real
/// microphone is enumerated. If the default name matches known
/// loopback-y strings, walk the input_devices list and pick the first
/// entry whose name looks like a real mic instead.
fn pick_input_device(host: &cpal::Host) -> Result<cpal::Device, String> {
    let default = host
        .default_input_device()
        .ok_or_else(|| "no default input device".to_string())?;
    let default_name = default.name().unwrap_or_default().to_lowercase();
    let looks_like_loopback = default_name.contains("stereo mix")
        || default_name.contains("loopback")
        || default_name.contains("what u hear");
    if !looks_like_loopback {
        return Ok(default);
    }
    tracing::warn!(
        "[voice] default input {:?} looks like a loopback; searching for a real mic",
        default.name().unwrap_or_default()
    );
    if let Ok(devices) = host.input_devices() {
        for d in devices {
            let name = d.name().unwrap_or_default().to_lowercase();
            if name.contains("microphone")
                || name.contains("mic array")
                || name.contains("headset")
                || name.contains("input")
            {
                tracing::info!("[voice] override to {:?}", d.name().unwrap_or_default());
                return Ok(d);
            }
        }
    }
    // No obvious mic — fall back to the default anyway; user can pick.
    Ok(default)
}

fn tick_loop(app: AppHandle, audio: Arc<Mutex<AudioBuf>>, rx: Receiver<Cmd>, input_hz: u32) {
    let mut utterance: Vec<f32> = Vec::with_capacity(TARGET_HZ as usize * 10);
    let mut barge_in_arm = false;
    let mut armed_for_submit = false;
    let mut vad_state = VadState::Silent;
    let mut vad_edge_at: Option<Instant> = None;
    let mut last_amp_emit = Instant::now();
    let mut last_diag = Instant::now();
    let mut last_rms_seen: f32 = 0.0;

    // v0.28.58 — wake detector state. `wake_detector` is only Some
    // once wake has been enabled from Settings; loading the ONNX
    // chain is deferred until then so users who never turn wake on
    // don't pay the ~4MB memory + startup cost.
    let mut wake_enabled = false;
    let mut wake_detector: Option<super::wake::WakeDetector> = None;
    let mut wake_buffer: Vec<f32> =
        Vec::with_capacity(super::wake::CHUNK_SAMPLES * 2);

    loop {
        // Drain any pending control commands.
        while let Ok(cmd) = rx.try_recv() {
            match cmd {
                Cmd::Stop => return,
                Cmd::SetBargeIn(on) => barge_in_arm = on,
                Cmd::SetArmed(on) => {
                    armed_for_submit = on;
                    if on {
                        // Fresh arm starts a clean utterance buffer.
                        utterance.clear();
                        tracing::info!("[voice] ARMED — capturing next utterance");
                    } else {
                        tracing::info!("[voice] disarmed");
                    }
                }
                Cmd::SetWakeEnabled(on) => {
                    wake_enabled = on;
                    if on && wake_detector.is_none() {
                        match super::wake::WakeDetector::load_from_resources(
                            &app,
                            super::wake::DEFAULT_THRESHOLD,
                        ) {
                            Ok(d) => {
                                wake_detector = Some(d);
                                tracing::info!("[voice] wake detector loaded (Hey Jarvis)");
                            }
                            Err(e) => {
                                tracing::warn!("[voice] wake init failed: {e}");
                                wake_enabled = false;
                            }
                        }
                    }
                    if !on {
                        wake_buffer.clear();
                    }
                }
                Cmd::TakeUtterance(reply) => {
                    let taken = std::mem::take(&mut utterance);
                    let _ = reply.send(taken);
                }
            }
        }

        std::thread::sleep(Duration::from_millis(30));
        let incoming = {
            let mut a = audio.lock().unwrap();
            std::mem::take(&mut a.incoming)
        };

        if incoming.is_empty() {
            // Feed near-zero into VAD so hangover timers still fire.
            step_vad(
                &app,
                &mut vad_state,
                &mut vad_edge_at,
                0.0,
                barge_in_arm,
                armed_for_submit,
            );
            continue;
        }

        let decimated = decimate(&incoming, input_hz, TARGET_HZ);
        let rms = rms_of(&decimated);
        last_rms_seen = rms;
        // v0.28.2 — only accumulate the utterance when armed. When
        // disarmed we drop the samples on the floor after VAD reads
        // them; no wasted memory, no accidental transcription of
        // ambient conversation.
        if armed_for_submit {
            utterance.extend_from_slice(&decimated);
            const MAX_UTTERANCE_SAMPLES: usize = 30 * TARGET_HZ as usize;
            if utterance.len() > MAX_UTTERANCE_SAMPLES {
                let drop = utterance.len() - MAX_UTTERANCE_SAMPLES;
                utterance.drain(..drop);
            }
        }

        // v0.28.58 — feed the wake detector in exactly 1280-sample
        // (80ms) chunks. Runs regardless of whether an intent capture
        // is in flight — wake is the "start a new turn" path, so it
        // needs to fire independently of the submit-armed state.
        if wake_enabled {
            if let Some(det) = wake_detector.as_mut() {
                wake_buffer.extend_from_slice(&decimated);
                while wake_buffer.len() >= super::wake::CHUNK_SAMPLES {
                    let chunk: Vec<f32> = wake_buffer
                        .drain(..super::wake::CHUNK_SAMPLES)
                        .collect();
                    match det.feed_chunk(&chunk) {
                        Ok((prob, true)) => {
                            tracing::info!("[voice] WAKE detected prob={prob:.3}");
                            super::wake::emit_wake(&app, prob);
                        }
                        Ok((prob, false)) => {
                            if prob > 0.1 {
                                tracing::debug!(
                                    "[voice] wake avg prob={prob:.3}"
                                );
                            }
                        }
                        Err(e) => {
                            tracing::warn!("[voice] wake inference err: {e}");
                        }
                    }
                }
                // Cap the buffer at ~2 chunks so we don't accumulate
                // if inference stalls.
                let cap = super::wake::CHUNK_SAMPLES * 2;
                if wake_buffer.len() > cap {
                    let drop = wake_buffer.len() - cap;
                    wake_buffer.drain(..drop);
                }
            }
        }
        // VAD still runs when disarmed — it drives amplitude events
        // for the spheroid + will eventually drive wake-word detection.
        step_vad(
            &app,
            &mut vad_state,
            &mut vad_edge_at,
            rms,
            barge_in_arm,
            armed_for_submit,
        );

        if last_amp_emit.elapsed() >= Duration::from_millis(AMPLITUDE_EMIT_MS) {
            let payload = (rms.min(0.5) * 2.0).min(1.0);
            let _ = app.emit("voice://amplitude", payload);
            last_amp_emit = Instant::now();
        }
        if last_diag.elapsed() >= Duration::from_millis(500) {
            let state_name = match vad_state {
                VadState::Silent => "silent",
                VadState::ProbablySpeech => "probably-speech",
                VadState::Speech => "speech",
                VadState::ProbablySilence => "probably-silence",
            };
            tracing::info!(
                "[voice] rms={:.4} state={} utterance_len={}",
                last_rms_seen,
                state_name,
                utterance.len()
            );
            last_diag = Instant::now();
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum VadState {
    Silent,
    ProbablySpeech,
    Speech,
    ProbablySilence,
}

fn step_vad(
    app: &AppHandle,
    state: &mut VadState,
    edge_at: &mut Option<Instant>,
    rms: f32,
    barge_in_arm: bool,
    armed_for_submit: bool,
) {
    let now = Instant::now();
    match *state {
        VadState::Silent => {
            if rms >= VAD_SPEECH_RMS {
                *state = VadState::ProbablySpeech;
                *edge_at = Some(now);
            }
        }
        VadState::ProbablySpeech => {
            if rms < VAD_SILENCE_RMS {
                *state = VadState::Silent;
                *edge_at = None;
            } else if let Some(edge) = *edge_at {
                if now.duration_since(edge) >= Duration::from_millis(VAD_ONSET_MS) {
                    *state = VadState::Speech;
                    *edge_at = None;
                    tracing::info!(
                        "[voice] speech-start rms={:.4} armed={}",
                        rms,
                        armed_for_submit
                    );
                    // Only emit speech-start to the frontend when
                    // armed. Otherwise we would flip the canvas to
                    // voice mode + show the spheroid every time
                    // someone in the room made noise.
                    if armed_for_submit {
                        let _ = app.emit("voice://speech-start", ());
                    }
                    if barge_in_arm {
                        let _ = app.emit("voice://barge-in", ());
                    }
                }
            }
        }
        VadState::Speech => {
            if rms < VAD_SILENCE_RMS {
                *state = VadState::ProbablySilence;
                *edge_at = Some(now);
            }
        }
        VadState::ProbablySilence => {
            if rms >= VAD_SPEECH_RMS {
                *state = VadState::Speech;
                *edge_at = None;
            } else if let Some(edge) = *edge_at {
                if now.duration_since(edge) >= Duration::from_millis(VAD_HANGOVER_MS) {
                    *state = VadState::Silent;
                    *edge_at = None;
                    tracing::info!("[voice] speech-end (armed={})", armed_for_submit);
                    // v0.28.2 — only fire the speech-end event to the
                    // frontend when armed, so the frontend never runs
                    // finalizeTranscript on ambient speech.
                    if armed_for_submit {
                        let _ = app.emit("voice://speech-end", ());
                    }
                }
            }
        }
    }
}

fn build_stream_f32(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    audio: Arc<Mutex<AudioBuf>>,
    channels: usize,
) -> Result<cpal::Stream, String> {
    device
        .build_input_stream(
            config,
            move |data: &[f32], _| push_mono_f32(data, channels, &audio),
            move |err| tracing::warn!("[voice] cpal stream err: {err}"),
            None,
        )
        .map_err(|e| format!("build cpal stream: {e}"))
}

fn build_stream_i16(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    audio: Arc<Mutex<AudioBuf>>,
    channels: usize,
) -> Result<cpal::Stream, String> {
    device
        .build_input_stream(
            config,
            move |data: &[i16], _| {
                let f: Vec<f32> = data.iter().map(|s| *s as f32 / 32768.0).collect();
                push_mono_f32(&f, channels, &audio);
            },
            move |err| tracing::warn!("[voice] cpal stream err: {err}"),
            None,
        )
        .map_err(|e| format!("build cpal stream: {e}"))
}

fn build_stream_u16(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    audio: Arc<Mutex<AudioBuf>>,
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
                push_mono_f32(&f, channels, &audio);
            },
            move |err| tracing::warn!("[voice] cpal stream err: {err}"),
            None,
        )
        .map_err(|e| format!("build cpal stream: {e}"))
}

fn push_mono_f32(data: &[f32], channels: usize, audio: &Arc<Mutex<AudioBuf>>) {
    if channels == 0 {
        return;
    }
    let mut mono: Vec<f32> = Vec::with_capacity(data.len() / channels);
    for frame in data.chunks_exact(channels) {
        let sum: f32 = frame.iter().sum();
        mono.push(sum / channels as f32);
    }
    let mut a = audio.lock().unwrap();
    a.incoming.extend_from_slice(&mono);
}

fn rms_of(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f32 = samples.iter().map(|s| s * s).sum();
    (sum_sq / samples.len() as f32).sqrt()
}

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
