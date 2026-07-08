/**
 * useCanvasMode — v0.28.8 redesign.
 *
 * PURE DERIVATION. No writeback. No side effects. Returns the
 * current canvas mode as a function of (activity, focal, mapExpanded,
 * isFirstMoment, inactivity). Consumers subscribe via the hook and
 * get the up-to-date value on every render.
 *
 * Priority ladder (first match wins):
 *   1. voice — activity is listening or speaking (never hide the spheroid)
 *   2. map   — user has expanded a map focal AND that focal actually
 *              contains a map part
 *   3. idle  — user has been inactive for IDLE_MS OR it's the first
 *              moment of the session with no messages
 *   4. chat  — default
 *
 * The auto-expand behavior (fresh assistant map focal -> expand) and
 * the inactivity clock live in useCanvasSideEffects — a separate hook
 * so this derivation stays pure.
 */
import { useEffect, useRef, useState } from "react";
import { useAppStore } from "../../../stores/app";
import { useFocalContent } from "../useFocalContent";
import { parseRichResponse } from "../../../lib/richResponse";

export type CanvasMode = "voice" | "map" | "idle" | "chat";

const IDLE_MS = 5 * 60 * 1000;
const INACTIVITY_TICK_MS = 15_000;
const FRESH_FOCAL_MS = 30_000;

/**
 * Pure derivation of the current canvas mode. Call from any consumer
 * that needs to know which canvas to render.
 */
export function useCanvasMode(): CanvasMode {
  const activity = useAppStore((s) => s.activity);
  const isFirstMoment = useAppStore((s) => s.isFirstMoment);
  const mapExpanded = useAppStore((s) => s.mapExpanded);
  const { focal } = useFocalContent();
  const inactive = useInactivityTick();

  if (activity === "listening" || activity === "speaking") return "voice";

  if (focal && mapExpanded && focalHasMap(focal.content)) return "map";

  const noMessages = focal === null;
  if (inactive || (noMessages && isFirstMoment)) return "idle";

  return "chat";
}

/**
 * Auto-expand the map when a fresh assistant response arrives with a
 * map part. Fresh = focal.createdAt within the last FRESH_FOCAL_MS.
 * Any older focal (loaded from history / conv switch) is marked as
 * handled without expanding, so the user sees chat with the map as
 * a clickable inline card.
 *
 * Run once from WorkspaceV2. Keeping this side effect out of the
 * derivation hook prevents render-driven state cascades.
 */
export function useMapAutoExpand(): void {
  const setMapExpanded = useAppStore((s) => s.setMapExpanded);
  const mapExpandedFor = useAppStore((s) => s.mapExpandedFor);
  const { focal } = useFocalContent();

  useEffect(() => {
    if (!focal) return;
    const focalId = String(focal.id);
    if (mapExpandedFor === focalId) return;
    if (!focalHasMap(focal.content)) return;
    const created = focal.createdAt ? Date.parse(focal.createdAt) : NaN;
    const ageMs = Number.isFinite(created) ? Date.now() - created : Infinity;
    if (ageMs < FRESH_FOCAL_MS) {
      setMapExpanded(true, focalId);
    } else {
      setMapExpanded(false, focalId);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [focal?.id]);
}

/**
 * Tracks user idle time. Returns true when the user has been inactive
 * for IDLE_MS. Kept in its own hook so it can be reused and so it
 * doesn't churn the mode derivation.
 */
function useInactivityTick(): boolean {
  const [tick, setTick] = useState(0);
  const lastActiveRef = useRef<number>(Date.now());

  useEffect(() => {
    const markActive = () => {
      lastActiveRef.current = Date.now();
      // Force re-eval so mode transitions out of idle immediately.
      setTick((n) => n + 1);
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
    const id = window.setInterval(
      () => setTick((n) => n + 1),
      INACTIVITY_TICK_MS,
    );
    return () => window.clearInterval(id);
  }, []);

  // `tick` participates in the deps so the caller re-derives; the
  // boolean itself is what consumers care about.
  void tick;
  return Date.now() - lastActiveRef.current > IDLE_MS;
}

function focalHasMap(content: string | undefined): boolean {
  if (!content) return false;
  try {
    const rich = parseRichResponse(content);
    return rich?.parts.some((p) => p.kind === "map") ?? false;
  } catch {
    return false;
  }
}
