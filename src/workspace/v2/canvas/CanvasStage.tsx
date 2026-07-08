/**
 * CanvasStage — v2 Shell 13.
 *
 * The mode router. Reads canvasMode from the store and mounts the
 * matching canvas component. AnimatePresence handles the crossfade
 * when Travis's activity or intent flips the canvas to a new surface.
 */
import { AnimatePresence, motion } from "framer-motion";
import { ChatCanvas } from "./ChatCanvas";
import { VoiceCanvas } from "./VoiceCanvas";
import { MapCanvas } from "./MapCanvas";
import { IdleCanvas } from "./IdleCanvas";
import { CanvasErrorBoundary } from "./CanvasErrorBoundary";
import { useCanvasMode } from "./useCanvasMode";

export function CanvasStage() {
  const mode = useCanvasMode();

  return (
    <div className="absolute inset-0">
      <AnimatePresence mode="wait">
        <motion.div
          key={mode}
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.36, ease: [0.22, 1, 0.36, 1] }}
          className="absolute inset-0"
        >
          <CanvasErrorBoundary>
            {mode === "chat" && <ChatCanvas />}
            {mode === "voice" && <VoiceCanvas />}
            {mode === "map" && <MapCanvas />}
            {mode === "idle" && <IdleCanvas />}
          </CanvasErrorBoundary>
        </motion.div>
      </AnimatePresence>
    </div>
  );
}
