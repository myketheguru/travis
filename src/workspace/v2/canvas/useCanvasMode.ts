/**
 * useCanvasMode — v2 Shell 13.
 *
 * Derives the canvas mode from Travis's current state:
 *
 *   voice — activity is listening or speaking
 *   map   — latest assistant response's first part is a map
 *   idle  — no messages yet AND (isFirstMoment OR inactive for 10min)
 *   chat  — everything else (default)
 *
 * Writes the derived value back to the app store so consumers can
 * subscribe. Also drives inactivity detection: after IDLE_MS of no
 * user activity, we fall to idle; any interaction snaps back.
 */
import { useEffect, useRef } from "react";
import { useAppStore } from "../../../stores/app";
import { useFocalContent } from "../useFocalContent";
import { parseRichResponse } from "../../../lib/richResponse";

const IDLE_MS = 5 * 60 * 1000;

export function useCanvasMode() {
  const activity = useAppStore((s) => s.activity);
  const isFirstMoment = useAppStore((s) => s.isFirstMoment);
  const setCanvasMode = useAppStore((s) => s.setCanvasMode);
  const canvasMode = useAppStore((s) => s.canvasMode);
  const { focal } = useFocalContent();

  // Track "last active" locally. Note: this is separate from the store's
  // isFirstMoment (which is at-mount snapshot) so we can flip mid-session.
  const lastActiveRef = useRef<number>(Date.now());
  const idleTickRef = useRef<number | null>(null);

  useEffect(() => {
    const markActive = () => {
      lastActiveRef.current = Date.now();
    };
    window.addEventListener("keydown", markActive);
    window.addEventListener("mousedown", markActive);
    window.addEventListener("mousemove", markActive);
    return () => {
      window.removeEventListener("keydown", markActive);
      window.removeEventListener("mousedown", markActive);
      window.removeEventListener("mousemove", markActive);
    };
  }, []);

  useEffect(() => {
    // v0.28.2 — airtight priority ladder. Only the FIRST matching
    // condition wins per tick. Every branch either flips the mode
    // (guarded so we only setCanvasMode when different, to avoid
    // spurious renders) or returns.
    //
    //  1. Voice — activity says something is happening on the audio
    //     path. Highest priority; wins over content/idle so we never
    //     hide the spheroid mid-utterance.
    if (activity === "listening" || activity === "speaking") {
      if (canvasMode !== "voice") setCanvasMode("voice");
      return;
    }

    //  2. Map — latest assistant response contains a map part
    //     anywhere in its rich payload. Wrapped in try so a malformed
    //     rich response can never propagate + crash the mode logic.
    if (focal) {
      let hasMap = false;
      try {
        const rich = parseRichResponse(focal.content);
        hasMap = rich?.parts.some((p) => p.kind === "map") ?? false;
      } catch {
        hasMap = false;
      }
      if (hasMap) {
        if (canvasMode !== "map") setCanvasMode("map");
        return;
      }
    }

    //  3. Idle — user hasn't touched anything for IDLE_MS OR it's the
    //     first moment of the session AND no conversation has started.
    //     The old `noMessages && ...` gate never let idle kick in when
    //     any focal message existed — fixed in v0.27.6.
    const idleForAWhile = Date.now() - lastActiveRef.current > IDLE_MS;
    const noMessages = focal === null;
    if (idleForAWhile || (noMessages && isFirstMoment)) {
      if (canvasMode !== "idle") setCanvasMode("idle");
      return;
    }

    //  4. Chat — default.
    if (canvasMode !== "chat") setCanvasMode("chat");
  }, [activity, focal, isFirstMoment, canvasMode, setCanvasMode]);

  // Keep an interval to re-check idle status when nothing else changes.
  // v0.27.6 — Poll every 15s so 5min idle threshold triggers within
  // 15s of crossing, not up to 30s later.
  useEffect(() => {
    idleTickRef.current = window.setInterval(() => {
      const idleForAWhile = Date.now() - lastActiveRef.current > IDLE_MS;
      const currentMode = useAppStore.getState().canvasMode;
      if (idleForAWhile && currentMode !== "idle" && currentMode !== "voice") {
        useAppStore.getState().setCanvasMode("idle");
      }
    }, 15_000);
    return () => {
      if (idleTickRef.current != null) window.clearInterval(idleTickRef.current);
    };
  }, []);
}
