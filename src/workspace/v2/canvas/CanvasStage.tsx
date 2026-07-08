/**
 * CanvasStage — v0.28.11.
 *
 * The mode router. Reads canvasMode from the pure derivation hook and
 * mounts the matching canvas component.
 *
 * Previously used AnimatePresence with `mode="wait"` which deadlocked
 * whenever MapCanvas's async cleanup (MapLibre .remove()) didn't fire
 * fast enough — the new canvas never mounted, leaving the surface
 * blank with elements present in the DOM but invisible. Fixed by
 * dropping the wait-mode orchestration and letting each canvas own its
 * own entry animation. The stage itself no longer animates the
 * transitions; the individual canvases already do (framer-motion on
 * their content) so nothing visual is lost.
 */
import { ChatCanvas } from "./ChatCanvas";
import { VoiceCanvas } from "./VoiceCanvas";
import { MapCanvas } from "./MapCanvas";
import { IdleCanvas } from "./IdleCanvas";
import { CanvasErrorBoundary } from "./CanvasErrorBoundary";
import { useCanvasMode } from "./useCanvasMode";
import { useFocalContent } from "../useFocalContent";

export function CanvasStage() {
  const mode = useCanvasMode();
  const { focal } = useFocalContent();

  return (
    <div className="absolute inset-0">
      <CanvasErrorBoundary key={mode}>
        {mode === "chat" && <ChatCanvas />}
        {mode === "voice" && <VoiceCanvas />}
        {mode === "map" && (
          // Key on focal.id so MapCanvas fully remounts when the user
          // switches conversations. Prevents MapLibre from trying to
          // reuse its container across a stale/new focal, which was
          // the "blank on rapid map switch" bug in v0.28.10.
          <MapCanvas key={`map-${focal?.id ?? "empty"}`} />
        )}
        {mode === "idle" && <IdleCanvas />}
      </CanvasErrorBoundary>
    </div>
  );
}
