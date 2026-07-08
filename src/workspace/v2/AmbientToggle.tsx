/**
 * AmbientToggle — v0.28.2.
 *
 * Canvas-level toggle for ambient listening mode. When on, the mic
 * transcribes all detected speech + saves it locally (meetings,
 * calls, thinking-out-loud) without submitting to Travis. Turn it
 * on before a meeting; ask Travis about it later.
 *
 * Placed on the top-right HUD row next to the attention compass so
 * it's always in view. Tap to toggle. Small dot indicates state.
 * Number badge shows how many transcript segments have been captured
 * this session — click to open the transcript viewer.
 */
import { useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { useAppStore } from "../../stores/app";

export function AmbientToggle() {
  const [openList, setOpenList] = useState(false);
  const ambient = useAppStore((s) => s.ambientListening);
  const setAmbient = useAppStore((s) => s.setAmbientListening);
  const transcripts = useAppStore((s) => s.ambientTranscripts);
  const clearTranscripts = useAppStore((s) => s.clearAmbientTranscripts);

  const dotColor = ambient
    ? "rgba(255, 179, 92, 0.95)"
    : "rgba(236, 236, 241, 0.35)";

  return (
    <div className="relative">
      <motion.button
        whileHover={{ scale: 1.04 }}
        whileTap={{ scale: 0.96 }}
        onClick={() => {
          if (transcripts.length > 0) {
            setOpenList((o) => !o);
          } else {
            setAmbient(!ambient);
          }
        }}
        onDoubleClick={() => setAmbient(!ambient)}
        className="flex items-center gap-2 px-3 py-1.5 rounded-full backdrop-blur-md"
        style={{
          // v0.28.5 — denser bg so chip reads on light canvases (map).
          background: "rgba(0, 0, 0, 0.68)",
          border: `1px solid ${
            ambient
              ? "rgba(255, 179, 92, 0.60)"
              : "rgba(255, 255, 255, 0.18)"
          }`,
          boxShadow: "0 4px 16px -8px rgba(0, 0, 0, 0.6)",
        }}
        aria-label={ambient ? "Ambient listening on" : "Ambient listening off"}
        title={
          ambient
            ? "Ambient listening on — capturing everything. Tap to view; double-tap to stop."
            : "Ambient listening off — tap to enable"
        }
      >
        <span
          className="text-[9px] uppercase tracking-[0.24em] font-mono"
          style={{ color: "rgba(236, 236, 241, 0.55)" }}
        >
          ambient
        </span>
        <motion.span
          animate={ambient ? { opacity: [1, 0.5, 1] } : { opacity: 1 }}
          transition={{
            duration: 1.6,
            repeat: ambient ? Infinity : 0,
            ease: [0.42, 0, 0.58, 1],
          }}
          className="w-1.5 h-1.5 rounded-full"
          style={{
            background: dotColor,
            boxShadow: ambient ? `0 0 8px ${dotColor}` : "none",
          }}
        />
        {transcripts.length > 0 && (
          <span
            className="text-[11px] font-mono"
            style={{ color: "rgba(236, 236, 241, 0.85)" }}
          >
            {transcripts.length}
          </span>
        )}
      </motion.button>

      <AnimatePresence>
        {openList && (
          <motion.div
            key="ambient-list"
            initial={{ opacity: 0, y: -4 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -4 }}
            transition={{ duration: 0.22, ease: [0.22, 1, 0.36, 1] }}
            className="absolute right-0 mt-2 w-[360px] rounded-2xl backdrop-blur-md p-3"
            style={{
              background: "rgba(0, 0, 0, 0.65)",
              border: "1px solid rgba(255, 255, 255, 0.10)",
            }}
          >
            <div className="flex items-center justify-between mb-2">
              <div
                className="text-[10px] uppercase tracking-[0.22em] font-mono"
                style={{ color: "rgba(236, 236, 241, 0.55)" }}
              >
                ambient capture · {transcripts.length}
              </div>
              <div className="flex items-center gap-2">
                <button
                  onClick={() => setAmbient(!ambient)}
                  className="text-[10px] tracking-wide px-2 py-0.5 rounded-full"
                  style={{
                    background: ambient
                      ? "rgba(255, 179, 92, 0.14)"
                      : "rgba(255, 255, 255, 0.06)",
                    border: `1px solid ${
                      ambient
                        ? "rgba(255, 179, 92, 0.55)"
                        : "rgba(255, 255, 255, 0.14)"
                    }`,
                    color: ambient
                      ? "rgba(255, 179, 92, 0.95)"
                      : "rgba(236, 236, 241, 0.75)",
                  }}
                >
                  {ambient ? "stop" : "start"}
                </button>
                <button
                  onClick={() => {
                    if (transcripts.length > 0) clearTranscripts();
                  }}
                  className="text-[10px] tracking-wide px-2 py-0.5 rounded-full"
                  style={{
                    background: "transparent",
                    border: "1px solid rgba(255, 255, 255, 0.14)",
                    color: "rgba(236, 236, 241, 0.55)",
                  }}
                >
                  clear
                </button>
              </div>
            </div>
            <div className="flex flex-col gap-1.5 max-h-[320px] overflow-y-auto">
              {transcripts
                .slice()
                .reverse()
                .map((t) => (
                  <div
                    key={t.id}
                    className="rounded-xl px-3 py-2"
                    style={{ background: "rgba(255, 255, 255, 0.04)" }}
                  >
                    <div
                      className="text-[9px] uppercase tracking-wider font-mono"
                      style={{ color: "rgba(236, 236, 241, 0.4)" }}
                    >
                      {new Date(t.occurredAt).toLocaleTimeString()}
                    </div>
                    <div
                      className="text-[13px] mt-0.5 leading-snug"
                      style={{ color: "rgba(236, 236, 241, 0.92)" }}
                    >
                      {t.text}
                    </div>
                  </div>
                ))}
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
