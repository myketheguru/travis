/**
 * WorkspaceV2 — canvas-first workspace with HUD overlays.
 *
 * v0.27 (v2 Shells 13-17) — the canvas is context-aware. It becomes
 * whatever Travis is doing right now:
 *   chat  → focus-shifting message stream (default with any convo)
 *   voice → spheroid + Listening/Speaking caption
 *   map   → full-bleed animated route + info overlay
 *   idle  → splash-style greeting (cold boot / 10min inactivity)
 * See canvas/useCanvasMode.ts for the derivation logic.
 *
 * Composer is always visible at the bottom with a bordered emphasis so
 * the user always knows where to type. AskTab remains mounted invisibly
 * to reuse its submit pipeline; the Composer bridges via
 * appStore.pendingComposerSubmit.
 *
 * HUD overlays:
 *   TL: orb (activity)
 *   TR: attention compass (compact chip)
 *   L:  thread rail
 *   R:  action rail
 *   BR: quick-access dock (settings, history, docs)
 */
import { useEffect } from "react";
import { motion } from "framer-motion";
import { useAppStore } from "../../stores/app";
import { AttentionCompass } from "./AttentionCompass";
import { AmbientToggle } from "./AmbientToggle";
import { ThinkingPill } from "./ThinkingPill";
import { CanvasBackdrop } from "./CanvasBackdrop";
import { ThreadRail } from "./ThreadRail";
import { SettingsOverlay } from "./SettingsOverlay";
import { HistoryOverlay } from "./HistoryOverlay";
import { ResumeChip } from "./ResumeChip";
import { QuickAccessDock } from "./QuickAccessDock";
import { DocumentsOverlay } from "./DocumentsOverlay";
import { ContactsOverlay } from "./ContactsOverlay";
import { CanvasStage } from "./canvas/CanvasStage";
import { useCanvasMode, useMapAutoExpand } from "./canvas/useCanvasMode";
import { useNativeVoice } from "../../voice/useNativeVoice";
import { useConversationStream } from "../../chat/useConversationStream";
import { useHydrateChatStore } from "../../chat/useHydrateChatStore";
import { Composer } from "./Composer";
import { useFocalContent } from "./useFocalContent";
import AskTab from "../../manage/tabs/AskTab";

export function WorkspaceV2() {
  const noteUserActivity = useAppStore((s) => s.noteUserActivity);
  const setHistoryOverlayOpen = useAppStore((s) => s.setHistoryOverlayOpen);
  const setDocumentsOverlayOpen = useAppStore((s) => s.setDocumentsOverlayOpen);
  const setSettingsOverlayOpen = useAppStore((s) => s.setSettingsOverlayOpen);
  const setContactsOverlayOpen = useAppStore((s) => s.setContactsOverlayOpen);
  const setPendingPairToken = useAppStore((s) => s.setPendingPairToken);
  const canvasMode = useCanvasMode();
  useFocalContent();

  // v0.28.8 — auto-expand fresh assistant map focals. Kept in its own
  // hook so useCanvasMode remains pure derivation.
  useMapAutoExpand();

  // v0.28 — native mic pipeline: cpal capture, VAD, auto-transcribe
  // on end-of-utterance, barge-in during Piper playback. Enabled by
  // default; a Settings toggle will let users opt out.
  useNativeVoice({ enabled: true });

  // v0.28.70 — full conversation stream. This hook replaces
  // useAssistantStream and handles the ENTIRE chat lifecycle: user
  // message insert, assistant message create (with tmpId), text
  // deltas, reasoning deltas, tool call starts, and the final swap
  // tmp→realId on assistant-done. All events land in chatStore's
  // messagesMap; ChatCanvas renders from there.
  useConversationStream();
  // Load persisted DB messages into chatStore when the active
  // conversation changes so history reappears on switch.
  useHydrateChatStore();

  // Global shortcuts.
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if ((e.metaKey || e.ctrlKey) && e.key === ",") {
        e.preventDefault();
        setSettingsOverlayOpen(true);
        return;
      }
      if ((e.metaKey || e.ctrlKey) && (e.key === "k" || e.key === "K")) {
        e.preventDefault();
        setHistoryOverlayOpen(true);
        return;
      }
      if ((e.metaKey || e.ctrlKey) && (e.key === "d" || e.key === "D")) {
        e.preventDefault();
        setDocumentsOverlayOpen(true);
        return;
      }
      // v0.28.45 — ⌘⇧C opens Travis contacts overlay. Uses shift so
      // it doesn't collide with copy.
      if ((e.metaKey || e.ctrlKey) && e.shiftKey && (e.key === "c" || e.key === "C")) {
        e.preventDefault();
        setContactsOverlayOpen(true);
        return;
      }
      if (!e.metaKey && !e.ctrlKey && !e.altKey && e.key.length === 1) {
        noteUserActivity();
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [
    setSettingsOverlayOpen,
    setHistoryOverlayOpen,
    setDocumentsOverlayOpen,
    setContactsOverlayOpen,
    noteUserActivity,
  ]);

  // v0.28.46 — travis://pair?tok=… deep link. Rust dispatches
  // `travis://pair` on window; open contacts + stash the token so
  // the overlay can auto-redeem once mounted.
  useEffect(() => {
    const onPair = (e: Event) => {
      const detail = (e as CustomEvent<{ token: string }>).detail;
      if (detail?.token) {
        setPendingPairToken(detail.token);
        setContactsOverlayOpen(true);
      }
    };
    window.addEventListener("travis://pair" as keyof WindowEventMap, onPair);
    return () =>
      window.removeEventListener("travis://pair" as keyof WindowEventMap, onPair);
  }, [setContactsOverlayOpen, setPendingPairToken]);

  // v0.27.6 — Spacebar longpress (1.5s) push-to-talk. Only fires when
  // focus is NOT inside an input/textarea so it never eats a real
  // spacebar keystroke. Dispatches `travis:wake` which VoiceInputButton
  // picks up to arm the mic. Release stops the recording.
  useEffect(() => {
    let holdTimer: number | null = null;
    let waking = false;

    const isTypingTarget = (t: EventTarget | null) => {
      const el = t as HTMLElement | null;
      if (!el) return false;
      const tag = el.tagName;
      return (
        tag === "INPUT" ||
        tag === "TEXTAREA" ||
        el.isContentEditable === true
      );
    };

    function onDown(e: KeyboardEvent) {
      if (e.code !== "Space" || e.repeat) return;
      if (isTypingTarget(e.target)) return;
      if (holdTimer != null) return;
      holdTimer = window.setTimeout(() => {
        waking = true;
        window.dispatchEvent(new CustomEvent("travis:arm-voice"));
      }, 1500);
    }
    function onUp(e: KeyboardEvent) {
      if (e.code !== "Space") return;
      if (holdTimer != null) {
        window.clearTimeout(holdTimer);
        holdTimer = null;
      }
      if (waking) {
        window.dispatchEvent(new CustomEvent("travis:disarm-voice"));
        waking = false;
      }
    }
    window.addEventListener("keydown", onDown);
    window.addEventListener("keyup", onUp);
    return () => {
      window.removeEventListener("keydown", onDown);
      window.removeEventListener("keyup", onUp);
      if (holdTimer != null) window.clearTimeout(holdTimer);
    };
  }, []);

  // In map / voice modes, softly dim the rails so the canvas can shine.
  const railOpacity = canvasMode === "map" || canvasMode === "voice" ? 0.35 : 1;

  return (
    // v0.28.27 — mouse events no longer dismiss the splash. Only
    // keyboard input (typed characters) and mic arm/voice engagement
    // signal that the user has actually started interacting. Mouse
    // moves / clicks on the empty canvas leave the splash intact.
    <div className="relative h-full min-h-0 overflow-hidden">
      {/* Base canvas backdrop — ambient depth behind everything */}
      <CanvasBackdrop />

      {/* The context-aware canvas — chat, voice, map, or idle */}
      <div className="absolute inset-0 z-10">
        <CanvasStage />
      </div>

      {/* HUD: orb TL */}
      <OrbHud />

      {/* HUD: TR row — ambient toggle + attention compass */}
      <motion.div
        animate={{ opacity: railOpacity }}
        transition={{ duration: 0.32 }}
        className="absolute top-3 right-3 z-20 pointer-events-auto flex items-center gap-2"
      >
        <AmbientToggle />
        <AttentionCompass />
      </motion.div>

      {/* Thinking indicator — floats top-center on any non-voice canvas */}
      <ThinkingPill />

      {/* HUD: thread rail L */}
      <motion.div
        animate={{ opacity: railOpacity }}
        transition={{ duration: 0.32 }}
      >
        <ThreadRail />
      </motion.div>

      {/* Left-middle quick-access dock (v0.27.2) */}
      <QuickAccessDock />

      {/* Resume chip lands just above the composer */}
      <div className="absolute bottom-[86px] left-0 right-0 z-30 pointer-events-none">
        <ResumeChip />
      </div>

      {/* Always-visible composer — the anchor for every mode */}
      <Composer />

      {/* AskTab stays mounted for its submit pipeline. Hidden off-canvas
          so it stays functional but never renders visually. When
          Composer sets pendingComposerSubmit, AskTab picks it up and
          fires its real submit. */}
      <div
        aria-hidden
        style={{
          position: "absolute",
          left: -99999,
          top: -99999,
          width: 800,
          height: 600,
          overflow: "hidden",
          opacity: 0,
          pointerEvents: "none",
        }}
      >
        <AskTab />
      </div>

      {/* Overlays */}
      <SettingsOverlay />
      <HistoryOverlay />
      <DocumentsOverlay />
      <ContactsOverlay />
    </div>
  );
}

/* ─── Orb (TL) ───────────────────────────────────────────────────── */

function OrbHud() {
  const activity = useAppStore((s) => s.activity);
  const setSettingsOverlayOpen = useAppStore((s) => s.setSettingsOverlayOpen);
  const pulseColor = pulseColorFor(activity);
  return (
    <div className="absolute top-3 left-3 z-20 pointer-events-auto">
      <motion.button
        whileHover={{ scale: 1.12 }}
        whileTap={{ scale: 0.92 }}
        animate={{
          scale:
            activity === "thinking" || activity === "listening" ? [1, 1.08, 1] : 1,
        }}
        transition={{
          duration: 1.6,
          repeat: Infinity,
          ease: [0.22, 1, 0.36, 1],
        }}
        onClick={() => setSettingsOverlayOpen(true)}
        className="h-6 w-6 rounded-full"
        style={{
          background: `radial-gradient(circle at 35% 30%, ${pulseColor.core}, ${pulseColor.rim} 70%)`,
          boxShadow: `0 0 24px ${pulseColor.glow}`,
        }}
        title="Open Settings (⌘,)"
        aria-label="Open Settings"
      />
    </div>
  );
}

function pulseColorFor(activity: string): {
  core: string;
  rim: string;
  glow: string;
} {
  switch (activity) {
    case "thinking":
      return {
        core: "rgba(189, 158, 255, 0.95)",
        rim: "rgba(124, 92, 255, 0.6)",
        glow: "rgba(124, 92, 255, 0.5)",
      };
    case "listening":
      return {
        core: "rgba(110, 196, 232, 0.95)",
        rim: "rgba(110, 196, 232, 0.5)",
        glow: "rgba(110, 196, 232, 0.5)",
      };
    case "speaking":
      return {
        core: "rgba(129, 199, 132, 0.95)",
        rim: "rgba(129, 199, 132, 0.5)",
        glow: "rgba(129, 199, 132, 0.5)",
      };
    case "typing":
      return {
        core: "rgba(255, 179, 92, 0.95)",
        rim: "rgba(255, 179, 92, 0.5)",
        glow: "rgba(255, 179, 92, 0.4)",
      };
    default:
      return {
        core: "rgba(236, 236, 241, 0.9)",
        rim: "rgba(236, 236, 241, 0.4)",
        glow: "rgba(236, 236, 241, 0.15)",
      };
  }
}
