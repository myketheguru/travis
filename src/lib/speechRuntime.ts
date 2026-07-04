/**
 * Speech-to-text IPC bindings (v0.22.12).
 *
 * whisper.cpp runs entirely local. Frontend captures audio via
 * AudioContext at 16 kHz mono, sends the raw f32 samples to Rust,
 * gets back a transcript.
 */
import { invoke } from "@tauri-apps/api/core";

export interface SpeechStatus {
  ready: boolean;
  model: string;
  inProgress: boolean;
}

export const speechRuntimeStatus = () =>
  invoke<SpeechStatus>("speech_runtime_status");

export const speechRuntimeEnsure = () =>
  invoke<void>("speech_runtime_ensure");

export const speechTranscribe = (samples: Float32Array) =>
  invoke<string>("speech_transcribe", {
    // Tauri IPC serializes Float32Array as a regular number array;
    // that's fine at MVP call rates (a couple hundred KB max).
    samples: Array.from(samples),
  });
