/**
 * WakeWordSection — v0.28.58.
 *
 * Opt-in toggle for the openWakeWord "Hey Jarvis" detector. Off by
 * default. When on, the capture thread loads a ~4MB ONNX chain and
 * runs it on every 80ms of decimated audio; on detection, the same
 * `travis:arm-voice` event that the mic button fires goes out.
 *
 * The wake phrase is "Hey Jarvis" as a placeholder — openWakeWord
 * doesn't ship a pre-trained "Hey Travis" model. A custom-trained
 * model is a separate follow-up.
 */
import { useCallback, useEffect, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { nativeVoice } from "../lib/nativeVoice";

export function WakeWordSection() {
  const [enabled, setEnabled] = useState<boolean | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      setEnabled(await nativeVoice.wakeEnabled());
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const toggle = async () => {
    if (busy || enabled == null) return;
    const next = !enabled;
    setBusy(true);
    setError(null);
    try {
      await nativeVoice.setWakeEnabled(next);
      setEnabled(next);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const on = enabled === true;

  return (
    <div className="flex flex-col gap-3">
      <p className="text-bone-3 text-[11px] leading-relaxed">
        Say <strong>"Hey Jarvis"</strong> to open the mic without touching
        the keyboard. Runs a small ~1% CPU model on-device; nothing is
        transcribed or uploaded until the wake phrase fires. Off by
        default. Wake phrase is "Hey Jarvis" as a placeholder — a
        "Hey Travis" model requires a training run that ships in a
        future release.
      </p>

      <div
        className="flex items-center justify-between gap-3 px-3 py-2.5 rounded-md border"
        style={{
          borderColor: on
            ? "rgba(129, 199, 132, 0.35)"
            : "rgba(255, 255, 255, 0.1)",
          background: on
            ? "rgba(129, 199, 132, 0.06)"
            : "rgba(255, 255, 255, 0.02)",
        }}
      >
        <div className="min-w-0 flex-1">
          <div
            className="text-[12.5px]"
            style={{ color: "rgba(236, 236, 241, 0.9)" }}
          >
            Wake phrase: "Hey Jarvis"
          </div>
          <div
            className="text-[10.5px] font-mono opacity-60"
            style={{ color: "rgba(236, 236, 241, 0.7)" }}
          >
            {on ? "listening for wake phrase" : "off"}
          </div>
        </div>
        <motion.button
          whileHover={{ scale: 1.03 }}
          whileTap={{ scale: 0.97 }}
          onClick={toggle}
          disabled={busy || enabled == null}
          className="text-[11px] uppercase tracking-wider font-mono px-3 py-1.5 rounded-md transition-colors disabled:opacity-40"
          style={{
            background: on
              ? "rgba(129, 199, 132, 0.15)"
              : "rgba(189, 158, 255, 0.10)",
            color: on ? "rgb(129, 199, 132)" : "rgb(189, 158, 255)",
            border: `1px solid ${
              on
                ? "rgba(129, 199, 132, 0.4)"
                : "rgba(189, 158, 255, 0.4)"
            }`,
          }}
        >
          {busy ? "…" : on ? "on" : "turn on"}
        </motion.button>
      </div>

      <AnimatePresence>
        {error && (
          <motion.div
            key="err"
            initial={{ opacity: 0, y: -4 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -4 }}
            transition={{ duration: 0.2, ease: [0.22, 1, 0.36, 1] }}
            className="text-[11px] font-mono"
            style={{ color: "rgba(255, 130, 130, 0.9)" }}
          >
            {error}
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
