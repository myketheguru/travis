//! Voice — v0.28 native audio pipeline.
//!
//! Replaces the WebView AudioContext + ScriptProcessor path with
//! native cpal capture. Rust owns the mic stream, runs energy-based
//! VAD on it, emits amplitude + speech-start/end events to the
//! frontend via Tauri, and hands the accumulated samples to whisper
//! automatically on end-of-utterance.
//!
//! The frontend never sees raw samples — it just gets:
//!   voice://amplitude        (f32 in [0,1], ~20 Hz)
//!   voice://speech-start
//!   voice://speech-end
//!   voice://barge-in         (speech detected while Piper is playing)
//!   voice://transcript-final (final text after whisper runs)
//!
//! Design notes:
//! - Sample rate: whisper wants 16kHz mono f32. Windows WASAPI usually
//!   gives us 48kHz — we decimate 3:1. For 44.1kHz we approximate.
//! - VAD: simple RMS energy threshold with hysteresis + a silence
//!   timeout. Good enough for the "auto-transcribe on pause" behavior;
//!   a proper Silero-VAD is a future upgrade.
//! - Barge-in: when the frontend flips activity=speaking (Piper is
//!   playing), we still run VAD. If speech-start fires, we emit
//!   voice://barge-in so the frontend can stop Piper immediately.

pub mod capture;
pub mod cmd;
pub mod whisper_cache;
/// v0.28.26 — Piper TTS subprocess wrapper. Turns text into WAV
/// bytes we hand back to the frontend for playback. Bundled binary +
/// voice model live under `resources/piper/`; runtime falls back to
/// speechSynthesis if either is missing.
pub mod piper;

/// Target sample rate for whisper (16kHz). Exposed so the WAV writer
/// in cmd.rs uses the same value the capture pipeline decimates to.
pub fn capture_target_hz() -> u32 {
    16_000
}
