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
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
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
  const finalizingRef = useRef(false);
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
          // v0.28.57 — INTENT-ONLY. speech-end only ever does work if
          // the user actively opened the mic (mic button or wake
          // shortcut). Ambient/passive captures were removed — Rust
          // stays disarmed between explicit sessions, so bg noise
          // cannot produce a transcript, cannot enter the composer,
          // and cannot start a turn.
          const wasIntent = useAppStore.getState().activity === "listening";
          if (!wasIntent) return;
          finalizingRef.current = true;
          playCue("heard");
          setActivity("thinking");
          useAppStore.getState().setVoiceTranscribing(true);
          try {
            const result = await nativeVoice.finalizeTranscript();
            const trimmed = result.text.trim();
            if (trimmed.length === 0) {
              setActivity("idle");
            } else {
              if (result.audioPath) {
                useAppStore.getState().setPendingVoiceAudio({
                  audioPath: result.audioPath,
                  durationMs: result.durationMs,
                  transcript: trimmed,
                });
              }
              useAppStore.getState().setSpeakNextResponse(true);
              setPendingComposerSubmit(trimmed);
            }
          } catch (err) {
            console.warn("[voice] finalizeTranscript failed:", err);
            setActivity("idle");
          } finally {
            // v0.28.58 — ALWAYS reset the intent-arm state at the end
            // of a capture cycle, no matter what path we took. The
            // previous code only reset on the "non-empty transcript"
            // success path, so an empty/failed transcription left
            // both intentArmedRef=true AND Rust armed. Next VAD
            // trigger (any noise) then fired speech-start with
            // armed=true, which flipped activity="listening" and
            // popped the spheroid without the user having done
            // anything — the exact random-listening symptom users
            // reported after previously clicking the mic.
            intentArmedRef.current = false;
            try {
              await nativeVoice.setArmed(false);
            } catch {
              /* best effort */
            }
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
      // v0.28.58 — openWakeWord fires here when "Hey Jarvis" is
      // detected. Wake is the second of the two allowed voice entry
      // points (mic click is the first); it dispatches the same
      // `travis:arm-voice` event the mic button uses, so the whole
      // downstream flow is identical.
      unlisteners.push(
        await onVoiceEvent<number>("voice://wake-detected", () => {
          if (intentArmedRef.current || finalizingRef.current) return;
          window.dispatchEvent(new CustomEvent("travis:arm-voice"));
        }),
      );

      // v0.28.57 — journal_ingest emits this the moment the user
      // message row lands in the DB, before the (slow) LLM turn
      // begins. Flip the active conversation + link any pending voice
      // audio right away so the canvas shows a real convo + playable
      // audio card while the assistant reply is still being generated.
      unlisteners.push(
        await listen<{
          conversationId: number;
          userMessageId: number;
          content: string;
        }>("journal://user-inserted", (evt) => {
          const { conversationId, userMessageId } = evt.payload;
          const store = useAppStore.getState();
          if (store.activeConversationId !== conversationId) {
            store.setActiveConversationId(conversationId);
          }
          const voiceAudio = store.pendingVoiceAudio;
          if (voiceAudio) {
            store.setPendingVoiceAudio(null);
            void nativeVoice
              .linkUtterance({
                messageId: userMessageId,
                audioPath: voiceAudio.audioPath,
                durationMs: voiceAudio.durationMs,
                transcript: voiceAudio.transcript,
              })
              .catch((err) => {
                console.warn("[voice] eager link failed:", err);
              });
          }
        }),
      );
    })();

    // Arm/disarm surface via window events so buttons / shortcuts
    // don't need direct access to nativeVoice.
    const onArm = () => {
      playCue("wake");
      intentArmedRef.current = true;
      setActivity("listening");
      // v0.28.27 — mic arm counts as user engagement, so the splash
      // dismisses when Travis wakes for the user's first utterance.
      useAppStore.getState().noteUserActivity();
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

    // v0.28.57 — removed: `travis:auto-arm-mic` listener + the
    // ambient-listening arms-Rust effect. New contract: voice
    // capture only starts on explicit user intent — mic button click
    // or the wake shortcut (Ctrl+Alt+Space). Nothing else opens the
    // pipeline. Ambient listening as a passive-transcription mode
    // was removed because it processed every VAD-bounded utterance,
    // which meant background conversation showed up as user input.

    return () => {
      cancelled = true;
      window.removeEventListener("travis:arm-voice", onArm);
      window.removeEventListener("travis:disarm-voice", onDisarm);
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
