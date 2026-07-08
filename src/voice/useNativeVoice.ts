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
          // Only fires when armed (Rust-side gate). Set listening
          // activity so the canvas flips to voice + spheroid appears.
          setActivity("listening");
        }),
      );
      unlisteners.push(
        await onVoiceEvent<null>("voice://speech-end", async () => {
          if (finalizingRef.current) return;
          finalizingRef.current = true;
          const wasIntent = useAppStore.getState().activity === "listening";
          if (wasIntent) {
            playCue("heard");
            // v0.28.12 — show 'thinking' immediately so the user gets
            // feedback that we heard them + we're working. AskTab will
            // set thinking again when it starts the LLM turn; that's
            // a no-op state change.
            setActivity("thinking");
          }
          try {
            const text = await nativeVoice.finalizeTranscript();
            const trimmed = text.trim();
            if (trimmed.length > 0) {
              if (wasIntent) {
                setPendingComposerSubmit(trimmed);
              } else if (ambientRef.current) {
                // Ambient capture — save transcript for later review,
                // do NOT submit to LLM. User can browse ambient
                // transcripts from the canvas. Also persist to SQLite
                // via ambient_transcript_save so the
                // get_ambient_transcripts tool can query them.
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
            // Note: don't force activity to idle here on the success
            // path — AskTab will drive thinking -> idle as the LLM
            // turn completes. Forcing idle here caused the voice
            // spheroid to disappear before the response arrived.
            finalizingRef.current = false;
            if (wasIntent) {
              // Only auto-disarm when this was an intent capture.
              // If ambient is on, stay armed so the next utterance
              // still gets caught.
              try {
                if (!ambientRef.current) await nativeVoice.setArmed(false);
              } catch {
                /* best effort */
              }
            }
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
      if (wasListening && !finalizingRef.current) {
        finalizingRef.current = true;
        playCue("heard");
        setActivity("thinking");
        try {
          const text = await nativeVoice.finalizeTranscript();
          const trimmed = text.trim();
          if (trimmed.length > 0) {
            setPendingComposerSubmit(trimmed);
          } else {
            // Nothing captured — just go back to idle.
            setActivity("idle");
          }
        } catch (err) {
          console.warn("[voice] manual finalize failed:", err);
          setActivity("idle");
        } finally {
          finalizingRef.current = false;
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
