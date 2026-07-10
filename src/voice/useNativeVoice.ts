/**
 * useNativeVoice — v0.28.2.
 *
 * Owns the native mic pipeline lifecycle on the frontend side. Native
 * cpal + VAD run continuously (so amplitude events + spheroid can
 * react to ambient audio). Actual capture-for-submission only happens
 * when the mic is ARMED — via the mic button, the spacebar longpress,
 * or (later) a wake word.
 *
 *   voice://amplitude   -> setSpeechAmplitude  (always)
 *   voice://speech-start -> setActivity('listening')  (only when armed)
 *   voice://speech-end  -> finalizeTranscript + submit  (only when armed)
 *   voice://barge-in    -> dispatch 'travis:piper-stop'
 *
 * The arm/disarm surface is exposed via a window event so buttons +
 * global shortcuts can trigger it without wiring props through the
 * component tree.
 *   window.dispatchEvent(new CustomEvent('travis:arm-voice'))
 *
 * Every side-effect is best-effort. Never throws.
 */
import { useEffect, useRef } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { nativeVoice, onVoiceEvent } from "../lib/nativeVoice";
import { useAppStore } from "../stores/app";
import { playCue } from "./cues";

interface Options {
  enabled: boolean;
}

export function useNativeVoice({ enabled }: Options) {
  const setSpeechAmplitude = useAppStore((s) => s.setSpeechAmplitude);
  const setActivity = useAppStore((s) => s.setActivity);
  const setPendingComposerSubmit = useAppStore(
    (s) => s.setPendingComposerSubmit,
  );
  const activity = useAppStore((s) => s.activity);
  const ambientListening = useAppStore((s) => s.ambientListening);
  const appendAmbientTranscript = useAppStore(
    (s) => s.appendAmbientTranscript,
  );
  const finalizingRef = useRef(false);
  const ambientRef = useRef(ambientListening);
  ambientRef.current = ambientListening;
  // v0.28.19 — track whether the current arm state is user-initiated
  // (intent) vs ambient-driven. Fixes the 'spheroid appears when
  // music/loud voice plays in the background': previously any VAD
  // speech-start emit would flip activity=listening, even if the
  // Rust side was armed for ambient capture.
  const intentArmedRef = useRef(false);

  useEffect(() => {
    if (!enabled) return;
    let cancelled = false;
    const unlisteners: UnlistenFn[] = [];

    (async () => {
      try {
        await nativeVoice.start();
      } catch (err) {
        // Fail silent — user shouldn't see a mic startup error blow up
        // the UI. Console remains for diagnostics.
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
          // v0.28.19 — only surface the spheroid for INTENT captures.
          // Ambient captures also arm the Rust side (for buffered
          // transcription) but shouldn't flip the UI to voice mode.
          if (intentArmedRef.current) {
            setActivity("listening");
          }
        }),
      );
      unlisteners.push(
        await onVoiceEvent<null>("voice://speech-end", async () => {
          if (finalizingRef.current) return;
          const wasIntent = useAppStore.getState().activity === "listening";
          // v0.28.18 — VAD hangover is now 2500ms (up from 700ms) so
          // mid-sentence pauses don't trigger speech-end. That means
          // when speech-end DOES fire, the user is legitimately done —
          // finalize + submit like the pre-v0.28.13 behavior. Manual
          // finalize on mic-tap (v0.28.12) still works as an escape
          // hatch if VAD ever misses.
          const isAmbient = ambientRef.current;
          if (!wasIntent && !isAmbient) return;
          finalizingRef.current = true;
          if (wasIntent) {
            playCue("heard");
            setActivity("thinking");
            useAppStore.getState().setVoiceTranscribing(true);
          }
          try {
            const result = await nativeVoice.finalizeTranscript();
            const trimmed = result.text.trim();
            if (trimmed.length === 0) {
              if (wasIntent) setActivity("idle");
            } else if (wasIntent) {
              // Stash audio metadata so AskTab can link it to the
              // message after journal_ingest returns the row id.
              if (result.audioPath) {
                useAppStore.getState().setPendingVoiceAudio({
                  audioPath: result.audioPath,
                  durationMs: result.durationMs,
                  transcript: trimmed,
                });
              }
              // v0.28.25 — modality-matched TTS. Mark this turn as spoken
              // so ChatTurn narrates the assistant reply. Text turns
              // leave it false, keeping typed exchanges silent.
              useAppStore.getState().setSpeakNextResponse(true);
              setPendingComposerSubmit(trimmed);
              intentArmedRef.current = false;
              try {
                await nativeVoice.setArmed(false);
              } catch {
                /* best effort */
              }
            } else if (isAmbient) {
              // Wake-word check: 'hey travis' during ambient promotes
              // to an armed intent capture.
              const normalized = trimmed
                .toLowerCase()
                .replace(/[^a-z0-9 ]/g, " ");
              if (
                normalized.includes("hey travis") ||
                normalized.includes("hi travis") ||
                normalized.includes("okay travis") ||
                normalized.includes("ok travis")
              ) {
                window.dispatchEvent(new CustomEvent("travis:arm-voice"));
              } else {
                appendAmbientTranscript(trimmed);
                try {
                  const { invoke } = await import("@tauri-apps/api/core");
                  await invoke("ambient_transcript_save", { text: trimmed });
                } catch (err) {
                  console.warn("[voice] ambient_transcript_save failed:", err);
                }
              }
            }
          } catch (err) {
            console.warn("[voice] finalizeTranscript failed:", err);
            if (wasIntent) setActivity("idle");
          } finally {
            finalizingRef.current = false;
            useAppStore.getState().setVoiceTranscribing(false);
          }
        }),
      );
      unlisteners.push(
        await onVoiceEvent<null>("voice://barge-in", () => {
          window.dispatchEvent(new CustomEvent("travis:piper-stop"));
        }),
      );
    })();

    // Arm/disarm surface via window events so buttons / shortcuts
    // don't need direct access to nativeVoice.
    const onArm = () => {
      playCue("wake");
      intentArmedRef.current = true;
      setActivity("listening");
      void nativeVoice.setArmed(true).catch(() => {});
    };
    // v0.28.12 — tapping mic while armed used to just call setArmed(false)
    // which DISCARDED the accumulated utterance buffer. Now: if we were
    // listening, finalize + submit whatever was said (like a manual
    // end-of-speech), then disarm. Prevents the "spoke, tapped stop,
    // nothing happened, response appeared later in history" bug.
    const onDisarm = async () => {
      const wasListening =
        useAppStore.getState().activity === "listening";
      intentArmedRef.current = false;
      if (wasListening && !finalizingRef.current) {
        finalizingRef.current = true;
        playCue("heard");
        setActivity("thinking");
        // v0.28.17 — surface optimistic user bubble immediately so
        // the user sees an acknowledgement while whisper is chewing.
        useAppStore.getState().setVoiceTranscribing(true);
        try {
          const result = await nativeVoice.finalizeTranscript();
          const trimmed = result.text.trim();
          if (trimmed.length > 0) {
            if (result.audioPath) {
              useAppStore.getState().setPendingVoiceAudio({
                audioPath: result.audioPath,
                durationMs: result.durationMs,
                transcript: trimmed,
              });
            }
            setPendingComposerSubmit(trimmed);
          } else {
            setActivity("idle");
          }
        } catch (err) {
          console.warn("[voice] manual finalize failed:", err);
          setActivity("idle");
        } finally {
          finalizingRef.current = false;
          useAppStore.getState().setVoiceTranscribing(false);
          try {
            await nativeVoice.setArmed(false);
          } catch {
            /* best effort */
          }
        }
      } else {
        void nativeVoice.setArmed(false).catch(() => {});
        setActivity("idle");
      }
    };
    window.addEventListener("travis:arm-voice", onArm);
    window.addEventListener("travis:disarm-voice", onDisarm);

    // v0.28.25 — auto re-arm after Travis speaks a voice-initiated
    // reply. ChatTurn dispatches `travis:auto-arm-mic` when the TTS
    // promise resolves; we open a ~6 second window. If the user says
    // anything, useNativeVoice's normal listening path picks it up as
    // the next turn (with speakNextResponse=true). If silent, we
    // disarm quietly. Guard: never override an already-armed state.
    let autoArmTimeoutId: number | null = null;
    const onAutoArm = () => {
      if (intentArmedRef.current) return;
      if (finalizingRef.current) return;
      playCue("wake");
      intentArmedRef.current = true;
      setActivity("listening");
      // Mark the next captured utterance as voice-modality so the
      // conversation stays voice-first without the user re-saying
      // "hey travis" every turn.
      useAppStore.getState().setSpeakNextResponse(true);
      void nativeVoice.setArmed(true).catch(() => {});
      if (autoArmTimeoutId != null) window.clearTimeout(autoArmTimeoutId);
      autoArmTimeoutId = window.setTimeout(() => {
        // Only auto-disarm if we're still armed AND still listening
        // (not mid-finalize). If user spoke, activity moved to
        // "thinking" and this branch skips.
        if (
          intentArmedRef.current &&
          useAppStore.getState().activity === "listening"
        ) {
          intentArmedRef.current = false;
          useAppStore.getState().setSpeakNextResponse(false);
          void nativeVoice.setArmed(false).catch(() => {});
          setActivity("idle");
        }
        autoArmTimeoutId = null;
      }, 6000);
    };
    window.addEventListener("travis:auto-arm-mic", onAutoArm);

    // Ambient listening: when the user has flipped ambient mode on,
    // we ALSO tell Rust to accumulate every VAD-bounded utterance so
    // we can grab transcripts even without an explicit arm. The
    // distinction is: ambient captures go to the transcript store
    // (for later reference) instead of the composer submit path.
    // Rust-side "armed" is a superset here — ambient toggling on
    // sets armed=true so the utterance buffer accumulates; individual
    // transcripts get routed to ambient vs submit based on whether
    // the user explicitly requested attention (via the button).
    const setAmbientArm = (on: boolean) => {
      void nativeVoice.setArmed(on).catch(() => {});
    };
    if (ambientListening) setAmbientArm(true);

    return () => {
      cancelled = true;
      window.removeEventListener("travis:arm-voice", onArm);
      window.removeEventListener("travis:disarm-voice", onDisarm);
      window.removeEventListener("travis:auto-arm-mic", onAutoArm);
      if (autoArmTimeoutId != null) window.clearTimeout(autoArmTimeoutId);
      unlisteners.forEach((u) => u());
      void nativeVoice.stop().catch(() => {});
      setSpeechAmplitude(0);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [enabled]);

  useEffect(() => {
    if (!enabled) return;
    void nativeVoice.setBargeIn(activity === "speaking").catch(() => {});
  }, [enabled, activity]);

  // v0.28.2 — cue on the transition from speaking -> idle so the user
  // hears a soft "over to you" bell when Travis finishes talking.
  const prevActivityRef = useRef(activity);
  useEffect(() => {
    if (prevActivityRef.current === "speaking" && activity !== "speaking") {
      playCue("done");
    }
    prevActivityRef.current = activity;
  }, [activity]);
}
