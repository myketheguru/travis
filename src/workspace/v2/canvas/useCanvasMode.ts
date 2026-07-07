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
    // Voice takes priority — spheroid should always win when Travis or
    // the user is actively speaking.
    if (activity === "listening" || activity === "speaking") {
      if (canvasMode !== "voice") setCanvasMode("voice");
      return;
    }

    // Map — latest assistant message has a map part in any position.
    if (focal) {
      const rich = parseRichResponse(focal.content);
      const hasMap = rich?.parts.some((p) => p.kind === "map") ?? false;
      if (hasMap) {
        if (canvasMode !== "map") setCanvasMode("map");
        return;
      }
    }

    // Idle — cold boot with no history OR user has been inactive
    // for IDLE_MS regardless of whether there is a prior conversation.
    // The old gate (`noMessages && ...`) never let idle kick in when
    // any focal message existed — that was the v0.27.5 bug.
    const idleForAWhile = Date.now() - lastActiveRef.current > IDLE_MS;
    const noMessages = focal === null;
    if (idleForAWhile || (noMessages && isFirstMoment)) {
      if (canvasMode !== "idle") setCanvasMode("idle");
      return;
    }

    // Default — chat.
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
