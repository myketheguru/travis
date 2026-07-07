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
// v0.28.1 — thresholds lowered after real-device testing revealed the
// v0.28.0 values (0.018 / 0.010) never triggered on typical laptop
// mics at desk distance. Values here calibrated to trigger reliably
// on quiet speech ~40cm from a built-in mic while staying above
// keyboard clatter (~0.003) and fan noise (~0.001).
const VAD_SPEECH_RMS: f32 = 0.008;
const VAD_SILENCE_RMS: f32 = 0.004;
const VAD_ONSET_MS: u64 = 100;
const VAD_HANGOVER_MS: u64 = 700;

/// Control commands sent from the Tauri command layer to the worker
/// thread that owns the cpal Stream. Each variant that needs a reply
/// carries a reply channel.
enum Cmd {
    Stop,
    SetBargeIn(bool),
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

    std::thread::spawn(move || {
        match capture_thread_main(app, rx) {
            Ok(_) => {
                let _ = started_tx.send(Ok(()));
            }
            Err(e) => {
                let _ = started_tx.send(Err(e));
            }
        }
    });

    // Wait briefly for the thread to signal 'stream up' or an error.
    // A hung thread here would be a serious platform issue; 3s is
    // more than enough for cpal + default input device.
    match started_rx.recv_timeout(Duration::from_secs(3)) {
        Ok(Ok(())) => Ok(VoiceHandle { tx }),
        Ok(Err(e)) => Err(e),
        Err(_) => Err("cpal init timed out".to_string()),
    }
}

fn capture_thread_main(app: AppHandle, rx: Receiver<Cmd>) -> Result<(), String> {
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

    // From this point the Stream MUST stay pinned to this thread —
    // it's !Send. The Stream is dropped at the end of this fn scope,
    // which is when we receive Cmd::Stop.
    tick_loop(app, audio, rx, input_hz);
    drop(stream);
    Ok(())
}

fn tick_loop(app: AppHandle, audio: Arc<Mutex<AudioBuf>>, rx: Receiver<Cmd>, input_hz: u32) {
    let mut utterance: Vec<f32> = Vec::with_capacity(TARGET_HZ as usize * 10);
    let mut barge_in_arm = false;
    let mut vad_state = VadState::Silent;
    let mut vad_edge_at: Option<Instant> = None;
    let mut last_amp_emit = Instant::now();
    // v0.28.1 diagnostic — periodic RMS + state log so we can tell
    // whether the mic is actually delivering audio and if VAD is
    // seeing it. Log line every 500ms; look for "rms=0.000" = mic
    // giving nothing OR "rms=0.005 silent" = mic works but too quiet
    // for current thresholds.
    let mut last_diag = Instant::now();
    let mut last_rms_seen: f32 = 0.0;

    loop {
        // Drain any pending control commands.
        while let Ok(cmd) = rx.try_recv() {
            match cmd {
                Cmd::Stop => return,
                Cmd::SetBargeIn(on) => barge_in_arm = on,
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
            step_vad(&app, &mut vad_state, &mut vad_edge_at, 0.0, barge_in_arm);
            continue;
        }

        let decimated = decimate(&incoming, input_hz, TARGET_HZ);
        let rms = rms_of(&decimated);
        last_rms_seen = rms;
        utterance.extend_from_slice(&decimated);
        step_vad(&app, &mut vad_state, &mut vad_edge_at, rms, barge_in_arm);

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
                    tracing::info!("[voice] speech-start (rms {:.4})", rms);
                    let _ = app.emit("voice://speech-start", ());
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
                    tracing::info!("[voice] speech-end");
                    let _ = app.emit("voice://speech-end", ());
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
