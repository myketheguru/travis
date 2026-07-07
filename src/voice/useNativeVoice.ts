/**
 * useNativeVoice — v0.28.
 *
 * Owns the native mic pipeline lifecycle on the frontend side. When
 * enabled, calls voice_start on mount, subscribes to all voice://*
 * events, wires them to the app store + auto-transcribe flow, and
 * cleans up on unmount.
 *
 * Behavior wired here:
 *   - voice://amplitude   -> setSpeechAmplitude
 *   - voice://speech-start -> setActivity('listening')
 *   - voice://speech-end   -> call finalizeTranscript, submit result
 *   - voice://barge-in     -> stop Piper (via 'travis:piper-stop' event)
 *
 * Silence between utterances is normal — the mic stays on but VAD
 * suppresses noise. Consumers control ON/OFF via the `enabled` flag.
 */
import { useEffect, useRef } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { nativeVoice, onVoiceEvent } from "../lib/nativeVoice";
import { useAppStore } from "../stores/app";

interface Options {
  enabled: boolean;
}

export function useNativeVoice({ enabled }: Options) {
  const setSpeechAmplitude = useAppStore((s) => s.setSpeechAmplitude);
  const setActivity = useAppStore((s) => s.setActivity);
  const setPendingComposerSubmit = useAppStore(
    (s) => s.setPendingComposerSubmit,
  );
  // Whether Travis (Piper) is currently speaking — drives barge-in arm.
  const activity = useAppStore((s) => s.activity);
  const finalizingRef = useRef(false);

  useEffect(() => {
    if (!enabled) return;
    let cancelled = false;
    const unlisteners: UnlistenFn[] = [];

    (async () => {
      try {
        await nativeVoice.start();
      } catch (err) {
        console.warn("[voice] native start failed:", err);
        return;
      }
      if (cancelled) return;

      unlisteners.push(
        await onVoiceEvent<number>("voice://amplitude", (a) => {
          setSpeechAmplitude(a);
        }),
      );
      unlisteners.push(
        await onVoiceEvent<null>("voice://speech-start", () => {
          setActivity("listening");
        }),
      );
      unlisteners.push(
        await onVoiceEvent<null>("voice://speech-end", async () => {
          if (finalizingRef.current) return;
          finalizingRef.current = true;
          try {
            const text = await nativeVoice.finalizeTranscript();
            const trimmed = text.trim();
            if (trimmed.length > 0) {
              setPendingComposerSubmit(trimmed);
            }
          } catch (err) {
            console.warn("[voice] finalizeTranscript failed:", err);
          } finally {
            setActivity("idle");
            finalizingRef.current = false;
          }
        }),
      );
      unlisteners.push(
        await onVoiceEvent<null>("voice://barge-in", () => {
          // Signal Piper to stop playback immediately; TTS player listens.
          window.dispatchEvent(new CustomEvent("travis:piper-stop"));
        }),
      );
    })();

    return () => {
      cancelled = true;
      unlisteners.forEach((u) => u());
      void nativeVoice.stop();
      setSpeechAmplitude(0);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [enabled]);

  // Arm barge-in whenever Travis is speaking (Piper playback).
  useEffect(() => {
    if (!enabled) return;
    void nativeVoice.setBargeIn(activity === "speaking");
  }, [enabled, activity]);
}
