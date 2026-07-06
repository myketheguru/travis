/**
 * CanvasStage — v2 Shell 13.
 *
 * The mode router. Reads canvasMode from the store and mounts the
 * matching canvas component. AnimatePresence handles the crossfade
 * when Travis's activity or intent flips the canvas to a new surface.
 */
import { AnimatePresence, motion } from "framer-motion";
import { useAppStore } from "../../../stores/app";
import { ChatCanvas } from "./ChatCanvas";
import { VoiceCanvas } from "./VoiceCanvas";
import { MapCanvas } from "./MapCanvas";
import { IdleCanvas } from "./IdleCanvas";

export function CanvasStage() {
  const mode = useAppStore((s) => s.canvasMode);

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
          {mode === "chat" && <ChatCanvas />}
          {mode === "voice" && <VoiceCanvas />}
          {mode === "map" && <MapCanvas />}
          {mode === "idle" && <IdleCanvas />}
        </motion.div>
      </AnimatePresence>
    </div>
  );
}
