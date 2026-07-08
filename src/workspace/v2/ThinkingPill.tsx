/**
 * ThinkingPill — v0.28.7.
 *
 * A floating "Travis is thinking…" indicator that surfaces on top of
 * any canvas mode. ChatCanvas already renders a pending assistant
 * bubble inline, but on map / voice / idle canvas modes there was NO
 * visible signal that the LLM was working. The user saw silence.
 *
 * Renders when activity === "thinking" and NOT during voice mode
 * (the spheroid is the visual for that state).
 *
 * Position: top-center below the title bar, out of the way of the
 * HUD chips and the orb.
 */
import { motion, AnimatePresence } from "framer-motion";
import { useAppStore } from "../../stores/app";

export function ThinkingPill() {
  const activity = useAppStore((s) => s.activity);

  // v0.28.12 — also show during voice mode. Previously hidden because
  // the spheroid was the visual, but the spheroid doesn't say WHAT
  // Travis is doing. Now the pill overlays the top even in voice mode
  // so the user knows Travis heard them + is working on it.
  const visible = activity === "thinking";

  return (
    <AnimatePresence>
      {visible && (
        <motion.div
          key="thinking-pill"
          initial={{ opacity: 0, y: -6 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: -6 }}
          transition={{ duration: 0.22, ease: [0.22, 1, 0.36, 1] }}
          className="absolute top-14 left-1/2 -translate-x-1/2 z-30 pointer-events-none"
        >
          <div
            className="flex items-center gap-2 px-3.5 py-1.5 rounded-full backdrop-blur-md"
            style={{
              background: "rgba(20, 18, 30, 0.72)",
              border: "1px solid rgba(189, 158, 255, 0.45)",
              boxShadow: "0 6px 24px -10px rgba(189, 158, 255, 0.35)",
            }}
          >
            <span className="flex gap-1">
              <Dot delay={0} />
              <Dot delay={0.16} />
              <Dot delay={0.32} />
            </span>
            <span
              className="text-[11.5px] font-mono tracking-wide"
              style={{ color: "rgba(236, 236, 241, 0.88)" }}
            >
              Travis is thinking…
            </span>
          </div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}

function Dot({ delay }: { delay: number }) {
  return (
    <motion.span
      className="w-1.5 h-1.5 rounded-full"
      style={{ background: "rgba(189, 158, 255, 0.95)" }}
      animate={{ opacity: [0.3, 1, 0.3], y: [0, -2, 0] }}
      transition={{
        duration: 1.0,
        repeat: Infinity,
        ease: [0.42, 0, 0.58, 1],
        delay,
      }}
    />
  );
}
