/**
 * SentrySection — Sentry mode toggle.
 *
 * v0.28.45: turning Sentry on now goes through SentryConsentModal.
 * The modal spells out what's captured, where it's stored, why we're
 * asking, and what will never be captured — and the user has to
 * scroll through and explicitly agree before enabling.
 *
 * Off by default. Turning it off never requires re-consent; only
 * turning it back on after a scope expansion does.
 */
import { useCallback, useEffect, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { sentrySetEnabled, sentryStatus, type SentryStatus } from "../lib/cloud";
import {
  SentryConsentModal,
  hasCurrentSentryConsent,
} from "./SentryConsentModal";

export function SentrySection() {
  const [status, setStatus] = useState<SentryStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [consentOpen, setConsentOpen] = useState(false);

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

  const setEnabled = async (next: boolean) => {
    if (busy || !status) return;
    setBusy(true);
    setError(null);
    try {
      await sentrySetEnabled(next);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  async function toggle() {
    if (!status || busy) return;
    // Turning OFF never needs consent.
    if (status.enabled) {
      await setEnabled(false);
      return;
    }
    // Turning ON: gate through the consent modal unless the user has
    // already agreed to the current version.
    if (hasCurrentSentryConsent()) {
      await setEnabled(true);
    } else {
      setConsentOpen(true);
    }
  }

  const enabled = status?.enabled ?? false;

  return (
    <div className="flex flex-col gap-3">
      <p className="text-bone-3 text-[11px] leading-relaxed">
        Sentry mode lets Travis passively observe your workflow so it
        can serve you better — noticing patterns, spotting overdue
        items, resuming where you left off. Currently captures app +
        window title every 30 seconds; screen snapshots are coming
        next. Off by default; you can pause or purge at any time.
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

      <SentryConsentModal
        open={consentOpen}
        onAgree={async () => {
          setConsentOpen(false);
          await setEnabled(true);
        }}
        onCancel={() => setConsentOpen(false)}
      />
    </div>
  );
}
