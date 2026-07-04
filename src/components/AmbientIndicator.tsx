/**
 * Ambient listener top-bar indicator + wake overlay (v0.22.14).
 *
 * Renders nothing until ambient mode is on. When on, shows a small
 * pulse in the corner. On wake ("Hey Travis"), flashes an overlay
 * card during the 5-second command capture window, then fades while
 * whisper transcribes.
 */
import { useEffect, useRef, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";
import {
  readAmbientEnabled,
  readAmbientName,
  startAmbient,
  type AmbientState,
} from "../lib/ambientListener";

interface Props {
  onCommand(text: string): void;
}

export function AmbientIndicator({ onCommand }: Props) {
  const [enabled, setEnabled] = useState<boolean>(false);
  const [state, setState] = useState<AmbientState>("idle");
  const stopRef = useRef<(() => void) | null>(null);

  // Read the localStorage state on mount + poll periodically so that
  // toggling in Settings takes effect without needing a page reload.
  useEffect(() => {
    const read = () => setEnabled(readAmbientEnabled());
    read();
    const t = window.setInterval(read, 2000);
    return () => window.clearInterval(t);
  }, []);

  // Start/stop the listener as enabled flips.
  useEffect(() => {
    let cancelled = false;
    (async () => {
      if (!enabled) {
        stopRef.current?.();
        stopRef.current = null;
        setState("idle");
        return;
      }
      const stop = await startAmbient({
        onWake() {},
        onCommand: (text) => onCommand(text),
        onStateChange: (s) => {
          if (!cancelled) setState(s);
        },
        onError(msg) {
          console.warn("ambient listener error:", msg);
        },
      });
      if (cancelled) {
        stop();
        return;
      }
      stopRef.current = stop;
    })();
    return () => {
      cancelled = true;
      stopRef.current?.();
      stopRef.current = null;
    };
  }, [enabled, onCommand]);

  if (!enabled) return null;

  const name = readAmbientName();
  const active = state === "captured" || state === "transcribing";

  return (
    <>
      {/* Persistent corner pill — reassures the user Travis is actually
          listening (or paused). */}
      <div className="fixed top-3 right-4 z-40 pointer-events-none flex items-center gap-2 rounded-full px-3 py-1.5"
           style={{
             background: "rgba(7, 8, 11, 0.6)",
             backdropFilter: "blur(10px)",
             border: `1px solid ${active ? "rgba(124, 92, 255, 0.4)" : "rgba(255, 255, 255, 0.08)"}`,
             color: "rgba(236, 236, 241, 0.8)",
           }}>
        <motion.span
          animate={{
            scale: [1, active ? 1.4 : 1.2, 1],
            opacity: [0.55, 1, 0.55],
          }}
          transition={{
            duration: active ? 1.0 : 2.2,
            repeat: Infinity,
            ease: "easeInOut",
          }}
          className="h-1.5 w-1.5 rounded-full"
          style={{
            background: active ? "rgb(124, 92, 255)" : "rgb(110, 196, 232)",
            boxShadow: active
              ? "0 0 10px rgba(124, 92, 255, 0.7)"
              : "0 0 8px rgba(110, 196, 232, 0.6)",
          }}
        />
        <span className="text-[10px] tracking-[0.15em] uppercase font-mono">
          {state === "captured"
            ? "listening"
            : state === "transcribing"
              ? "thinking"
              : `say "hey ${name}"`}
        </span>
      </div>

      {/* Wake overlay — big visual affordance during the command capture
          window so the user is confident they were heard. */}
      <AnimatePresence>
        {active && (
          <motion.div
            initial={{ opacity: 0, scale: 0.96 }}
            animate={{ opacity: 1, scale: 1 }}
            exit={{ opacity: 0, scale: 0.98 }}
            transition={{ duration: 0.25, ease: [0.16, 1, 0.3, 1] }}
            className="fixed inset-0 z-30 flex items-end justify-center pointer-events-none pb-16"
          >
            <div
              className="rounded-2xl px-6 py-4"
              style={{
                background: "rgba(7, 8, 11, 0.72)",
                backdropFilter: "blur(14px)",
                border: "1px solid rgba(124, 92, 255, 0.35)",
                boxShadow: "0 20px 60px -20px rgba(0, 0, 0, 0.6)",
                color: "rgba(236, 236, 241, 0.9)",
              }}
            >
              <div className="flex items-center gap-3">
                <motion.div
                  animate={{
                    scale: [1, 1.5, 1],
                    opacity: [0.55, 1, 0.55],
                  }}
                  transition={{ duration: 1.0, repeat: Infinity, ease: "easeInOut" }}
                  className="h-2 w-2 rounded-full"
                  style={{
                    background: "rgb(124, 92, 255)",
                    boxShadow: "0 0 12px rgba(124, 92, 255, 0.75)",
                  }}
                />
                <span className="text-sm font-light">
                  {state === "captured"
                    ? "I'm listening…"
                    : "Just a second…"}
                </span>
              </div>
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </>
  );
}
