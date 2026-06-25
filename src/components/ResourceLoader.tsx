/**
 * Sleek, consumer-friendly loader for the lazy Python bootstrap.
 *
 * Listens for `runtime-progress` events. Mounts at the App root so it
 * can overlay any surface. Stays out of the way when nothing's in
 * flight; flies in with a smooth bar + message when bootstrap kicks
 * off; auto-dismisses after a beat when ready.
 *
 * Copy never mentions Python, downloads, or installs — it just says
 * "Travis is getting additional resources to continue." The user
 * doesn't need to know about the implementation detail.
 */
import { useEffect, useRef, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import {
  onRuntimeProgress,
  overallProgress,
  pythonRuntimeStatus,
  type RuntimeProgress,
} from "../lib/pythonRuntime";

interface State {
  visible: boolean;
  phase: RuntimeProgress["phase"];
  pct: number;
  message: string;
  error?: string;
}

export function ResourceLoader() {
  const [state, setState] = useState<State>({
    visible: false,
    phase: "ready",
    pct: 0,
    message: "",
  });
  const hideTimer = useRef<number | null>(null);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;

    // If a bootstrap is somehow already running when the app loads
    // (e.g. user closed + reopened during install), pick it up.
    pythonRuntimeStatus()
      .then((s) => {
        if (cancelled) return;
        if (s.inProgress) {
          setState({
            visible: true,
            phase: "downloading",
            pct: 0,
            message: "Travis is getting additional resources to continue",
          });
        }
      })
      .catch(() => {});

    onRuntimeProgress((p) => {
      if (hideTimer.current) {
        window.clearTimeout(hideTimer.current);
        hideTimer.current = null;
      }
      if (p.phase === "ready") {
        // Snap to 100, hold a beat, then fade out.
        setState({ visible: true, phase: "ready", pct: 100, message: "Ready" });
        hideTimer.current = window.setTimeout(() => {
          setState((s) => ({ ...s, visible: false }));
        }, 800);
        return;
      }
      if (p.phase === "error") {
        setState({
          visible: true,
          phase: "error",
          pct: 0,
          message: "Something interrupted the setup",
          error: p.error,
        });
        // Errors linger longer so the user can read them.
        hideTimer.current = window.setTimeout(() => {
          setState((s) => ({ ...s, visible: false }));
        }, 5000);
        return;
      }
      setState({
        visible: true,
        phase: p.phase,
        pct: overallProgress(p),
        message: p.message,
      });
    }).then((un) => {
      unlisten = un;
    });

    return () => {
      cancelled = true;
      if (hideTimer.current) window.clearTimeout(hideTimer.current);
      unlisten?.();
    };
  }, []);

  return (
    <AnimatePresence>
      {state.visible && (
        <motion.div
          key="resource-loader"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.25, ease: [0.16, 1, 0.3, 1] }}
          className="fixed inset-0 z-50 flex items-center justify-center bg-[#07080b]/85 backdrop-blur-md pointer-events-auto"
        >
          <motion.div
            initial={{ y: 8, opacity: 0 }}
            animate={{ y: 0, opacity: 1 }}
            exit={{ y: -4, opacity: 0 }}
            transition={{ duration: 0.35, ease: [0.16, 1, 0.3, 1], delay: 0.05 }}
            className="w-[420px] max-w-[88vw] rounded-2xl border border-white/[0.08] bg-[#0c0d12]/95 px-8 py-7 shadow-[0_20px_60px_-15px_rgba(0,0,0,0.6)]"
          >
            {/* Animated dot — slow pulse so the loader doesn't read as
                "frozen" when the bar isn't moving much. */}
            <div className="flex items-center gap-3 mb-5">
              <motion.div
                animate={{
                  scale: [1, 1.18, 1],
                  opacity: [0.55, 1, 0.55],
                }}
                transition={{
                  duration: 2.2,
                  repeat: Infinity,
                  ease: "easeInOut",
                }}
                className="h-2 w-2 rounded-full bg-gradient-to-br from-[#7c5cff] to-[#6ec4e8] shadow-[0_0_10px_rgba(124,92,255,0.55)]"
              />
              <div className="text-[10px] tracking-[0.22em] uppercase text-white/35">
                {state.phase === "error" ? "// interrupted" : "// just a moment"}
              </div>
            </div>

            <h2 className="text-white text-base font-light leading-snug mb-4 max-w-[300px]">
              {state.error
                ? "Travis hit a snag getting set up."
                : state.message}
            </h2>

            {/* Progress bar — shimmery, not too literal. */}
            {state.phase !== "error" && (
              <div className="relative h-[3px] rounded-full bg-white/[0.06] overflow-hidden">
                <motion.div
                  initial={{ width: 0 }}
                  animate={{ width: `${state.pct}%` }}
                  transition={{ duration: 0.5, ease: [0.16, 1, 0.3, 1] }}
                  className="absolute inset-y-0 left-0 bg-gradient-to-r from-[#7c5cff] via-[#9d7dff] to-[#6ec4e8]"
                />
                {/* Sheen overlay that drifts across the bar. */}
                <motion.div
                  animate={{ x: ["-30%", "130%"] }}
                  transition={{ duration: 2.8, repeat: Infinity, ease: "linear" }}
                  className="absolute inset-y-0 w-[30%] bg-gradient-to-r from-transparent via-white/[0.15] to-transparent"
                />
              </div>
            )}

            {state.error && (
              <p className="text-[11px] text-[#e89a9a] leading-relaxed mt-3 font-mono">
                {state.error.length > 200
                  ? state.error.slice(0, 200) + "…"
                  : state.error}
              </p>
            )}

            <p className="text-[11px] text-white/30 leading-relaxed mt-4">
              {state.error
                ? "Check your internet connection and try the action again."
                : "First time only — we'll remember next time."}
            </p>
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
