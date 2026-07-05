/**
 * SentrySection — Sentry mode toggle (task 315).
 *
 * Opt-in foreground-window capture: samples app_name + window_title
 * every 30s, batches to /me/telemetry/ingest every 5 min. Cloud-side
 * consent (Sentry Phase 0) is a second layer — the ingest endpoint
 * discards events for kinds the user hasn't consented to.
 *
 * Off by default. Every user has to flip this on themselves.
 */
import { useCallback, useEffect, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { sentrySetEnabled, sentryStatus, type SentryStatus } from "../lib/cloud";

export function SentrySection() {
  const [status, setStatus] = useState<SentryStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const s = await sentryStatus();
      setStatus(s);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void refresh();
    const t = setInterval(refresh, 10_000);
    return () => clearInterval(t);
  }, [refresh]);

  async function toggle() {
    if (busy || !status) return;
    setBusy(true);
    setError(null);
    try {
      await sentrySetEnabled(!status.enabled);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  const enabled = status?.enabled ?? false;

  return (
    <div className="flex flex-col gap-3">
      <p className="text-bone-3 text-[11px] leading-relaxed">
        Sentry mode lets Travis passively observe what apps + windows
        you're using — no screen content, no keystrokes, just app +
        window title every 30 seconds. Off by default; you can pause
        or purge at any time.
      </p>

      <div
        className="flex items-center justify-between gap-3 px-3 py-2.5 rounded-md border"
        style={{
          borderColor: enabled
            ? "rgba(129, 199, 132, 0.35)"
            : "rgba(255, 255, 255, 0.1)",
          background: enabled
            ? "rgba(129, 199, 132, 0.06)"
            : "rgba(255, 255, 255, 0.02)",
        }}
      >
        <div className="min-w-0 flex-1">
          <div
            className="text-[12.5px]"
            style={{ color: "rgba(236, 236, 241, 0.9)" }}
          >
            Foreground window signal
          </div>
          <div
            className="text-[10.5px] font-mono opacity-60"
            style={{ color: "rgba(236, 236, 241, 0.7)" }}
          >
            {enabled
              ? `capturing · ${status?.buffered ?? 0} buffered`
              : "off"}
          </div>
        </div>
        <motion.button
          whileHover={{ scale: 1.03 }}
          whileTap={{ scale: 0.97 }}
          onClick={toggle}
          disabled={busy || !status}
          className="text-[11px] uppercase tracking-wider font-mono px-3 py-1.5 rounded-md transition-colors disabled:opacity-40"
          style={{
            background: enabled
              ? "rgba(129, 199, 132, 0.15)"
              : "rgba(189, 158, 255, 0.10)",
            color: enabled ? "rgb(129, 199, 132)" : "rgb(189, 158, 255)",
            border: `1px solid ${
              enabled
                ? "rgba(129, 199, 132, 0.4)"
                : "rgba(189, 158, 255, 0.4)"
            }`,
          }}
        >
          {busy ? "…" : enabled ? "on" : "turn on"}
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
