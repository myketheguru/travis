/**
 * nativeVoice — v0.28 native-mic bindings.
 *
 * Thin wrapper over the Rust `voice_*` Tauri commands + the events
 * they emit. Consumers subscribe with useNativeVoice() which handles
 * subscription lifecycle + wires the results into the app store.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export const nativeVoice = {
  start: () => invoke<{ started: boolean }>("voice_start"),
  stop: () => invoke<void>("voice_stop"),
  setBargeIn: (on: boolean) => invoke<void>("voice_set_barge_in", { on }),
  setArmed: (on: boolean) => invoke<void>("voice_set_armed", { on }),
  finalizeTranscript: () => invoke<string>("voice_finalize_transcript"),
};

export type VoiceEvent =
  | "voice://amplitude"
  | "voice://speech-start"
  | "voice://speech-end"
  | "voice://barge-in"
  | "voice://transcript-final";

export function onVoiceEvent<T = unknown>(
  event: VoiceEvent,
  handler: (payload: T) => void,
): Promise<UnlistenFn> {
  return listen<T>(event, (e) => handler(e.payload));
}
