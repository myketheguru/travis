/**
 * nativeVoice — v0.28 native-mic bindings.
 *
 * Thin wrapper over the Rust `voice_*` Tauri commands + the events
 * they emit. Consumers subscribe with useNativeVoice() which handles
 * subscription lifecycle + wires the results into the app store.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface FinalizeResult {
  text: string;
  audioPath: string;
  durationMs: number;
}

export interface VoiceUtterance {
  audioPath: string;
  durationMs: number;
  transcript: string;
}

export const nativeVoice = {
  start: () => invoke<{ started: boolean }>("voice_start"),
  stop: () => invoke<void>("voice_stop"),
  setBargeIn: (on: boolean) => invoke<void>("voice_set_barge_in", { on }),
  setArmed: (on: boolean) => invoke<void>("voice_set_armed", { on }),
  // v0.28.58 — openWakeWord toggle. Persists to the local meta store
  // so it survives restarts; ships as opt-in (off by default).
  setWakeEnabled: (on: boolean) =>
    invoke<void>("voice_set_wake_enabled", { on }),
  wakeEnabled: () => invoke<boolean>("voice_wake_enabled"),
  finalizeTranscript: () => invoke<FinalizeResult>("voice_finalize_transcript"),
  linkUtterance: (args: {
    messageId: number;
    audioPath: string;
    durationMs: number;
    transcript: string;
  }) => invoke<number>("voice_utterance_link", args),
  utteranceForMessage: (messageId: number) =>
    invoke<VoiceUtterance | null>("voice_utterance_for_message", { messageId }),
};

export type VoiceEvent =
  | "voice://amplitude"
  | "voice://speech-start"
  | "voice://speech-end"
  | "voice://barge-in"
  | "voice://transcript-final"
  | "voice://wake-detected";

export function onVoiceEvent<T = unknown>(
  event: VoiceEvent,
  handler: (payload: T) => void,
): Promise<UnlistenFn> {
  return listen<T>(event, (e) => handler(e.payload));
}
