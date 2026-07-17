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
          // v0.28.61 — disarm IMMEDIATELY, before we await whisper.
          // Prior version put this in `finally`, which meant Rust
          // stayed armed for ~700ms during finalize. If ambient
          // noise triggered VAD in that window, Rust emitted
          // speech-start with armed=true, the frontend handler saw
          // intentArmedRef=true, and setActivity("listening") popped
          // the spheroid mid-thinking. Disarming first (setArmed
          // doesn't drain the utterance — that's TakeUtterance's job)
          // closes the window entirely.
          intentArmedRef.current = false;
          try {
            await nativeVoice.setArmed(false);
          } catch {
            /* best effort */
          }
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
              // v0.28.72 — voice submits skip Composer.handleSubmit
              // (they flow through AskTab via pendingComposerSubmit).
              // AskTab doesn't insert into chatStore, so ChatCanvas
              // stayed empty until journal://user-inserted landed ~9s
              // later. Insert the optimistic user message directly
              // here so the audio card + transcript render the
              // instant whisper returns, before journal_ingest even
              // starts.
              const convId =
                useAppStore.getState().activeConversationId;
              if (convId !== null) {
                try {
                  const audio =
                    useAppStore.getState().pendingVoiceAudio ??
                    undefined;
                  const { insertOptimisticUserMessage } =
                    await import(
                      "../chat/useConversationStream"
                    );
                  insertOptimisticUserMessage(convId, trimmed, audio);
                } catch (err) {
                  console.warn(
                    "[voice] optimistic chatStore insert failed:",
                    err,
                  );
                }
              }
              setPendingComposerSubmit(trimmed);
            }
          } catch (err) {
            console.warn("[voice] finalizeTranscript failed:", err);
            setActivity("idle");
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
      // v0.28.60 — speculative whisper prewarm. Rust emits this at
      // the VAD Speech→ProbablySilence edge (i.e. the moment the
      // user starts pausing). Kicking off transcription NOW means
      // whisper runs in parallel with the ~1500ms VAD hangover;
      // by the time speech-end fires, finalize can reuse the
      // prewarmed transcript instead of paying the 500-1000ms
      // inference cost after the fact. Only useful during an intent
      // capture (Rust only emits when armed).
      unlisteners.push(
        await onVoiceEvent<null>("voice://speech-pausing", () => {
          if (!intentArmedRef.current) return;
          if (finalizingRef.current) return;
          void nativeVoice.prewarmTranscript().catch((err) => {
            console.warn("[voice] prewarm dispatch failed:", err);
          });
        }),
      );
      // v0.28.58 — openWakeWord fires here when "Hey Jarvis" is
      // detected. Wake is the second of the two allowed voice entry
      // points (mic click is the first); it dispatches the same
      // `travis:arm-voice` event the mic button uses, so the whole
      // downstream flow is identical.
      // v0.28.59 — also gated on chatBusy / thinking / speaking so
      // ambient TV/phone-video false positives during a live turn
      // can't fire arm. (Rust also pauses wake in these states —
      // this is the second gate.)
      unlisteners.push(
        await onVoiceEvent<number>("voice://wake-detected", () => {
          const s = useAppStore.getState();
          if (
            intentArmedRef.current ||
            finalizingRef.current ||
            s.chatBusy ||
            s.activity === "thinking" ||
            s.activity === "speaking"
          ) {
            return;
          }
          window.dispatchEvent(new CustomEvent("travis:arm-voice"));
        }),
      );

      // v0.28.68 — Rust emits this the instant the WAV is saved,
      // BEFORE running whisper (which takes ~700ms). Frontend renders
      // the audio card immediately with an empty transcript; the
      // transcript fills in when finalize returns. Kills the wait
      // between mic-release and card-visible.
      unlisteners.push(
        await listen<{ audioPath: string; durationMs: number }>(
          "voice://audio-ready",
          (evt) => {
            const store = useAppStore.getState();
            const cur = store.pendingVoiceAudio;
            store.setPendingVoiceAudio({
              audioPath: evt.payload.audioPath,
              durationMs: evt.payload.durationMs,
              // Preserve any transcript the store already has (e.g. an
              // in-progress finalize path where speech-end handler set
              // it after finalize returned). Empty string otherwise.
              transcript: cur?.transcript ?? "",
            });
          },
        ),
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
            // v0.28.63 — DON'T clear pendingVoiceAudio here. In
            // v0.28.61 we cleared eagerly and lost the audio card
            // during the canvas-mount race (voice→chat transition
            // remounts ChatCanvas after this event fires, so the
            // useRef snapshot never captures the audio). Keep the
            // store value alive; a persistent overlay in
            // WorkspaceV2 renders it live, and it clears itself
            // when the real message with linked audio appears in
            // the thread. Also stash the target message id so the
            // overlay can detect the handoff.
            store.setVoiceAudioLinkedMessageId(userMessageId);
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
      // v0.28.59 — hard guard: reject arm attempts while a turn is
      // in flight. This blocks (1) accidental double-arms from a
      // second mic click, (2) wake-word false positives that fired
      // during thinking, and (3) any programmatic arm dispatched
      // during TTS. Users reported "conversation starts while
      // another is loading" — this is the root of that class of
      // race, and the frontend guard is the last line of defence
      // even though we also pause wake in Rust.
      const s = useAppStore.getState();
      if (
        s.chatBusy ||
        s.activity === "thinking" ||
        s.activity === "speaking" ||
        finalizingRef.current
      ) {
        return;
      }
      playCue("wake");
      intentArmedRef.current = true;
      setActivity("listening");
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

  // v0.28.59 — pause the wake worker in Rust while a turn is in
  // flight. Belt-and-braces with the frontend guards above: even if
  // wake somehow fires, the Rust side skips the inference entirely
  // when paused, so no `voice://wake-detected` event is emitted.
  const chatBusy = useAppStore((s) => s.chatBusy);
  useEffect(() => {
    if (!enabled) return;
    const busy =
      chatBusy || activity === "thinking" || activity === "speaking";
    void nativeVoice.setWakePaused(busy).catch(() => {});
  }, [enabled, chatBusy, activity]);

  // v0.28.59 — a completing turn should dismiss the splash. If a
  // user's been away and their reply arrives (or their previously-
  // fired mic press returns), we need the chat visible, not the
  // idle overlay. Bump activityBeat on chatBusy true→false so the
  // canvas mode derivation exits idle.
  const chatBusyRef = useRef(chatBusy);
  useEffect(() => {
    if (chatBusyRef.current === true && chatBusy === false) {
      useAppStore.getState().noteUserActivity();
    }
    chatBusyRef.current = chatBusy;
  }, [chatBusy]);

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
