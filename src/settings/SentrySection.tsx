/**
 * SentrySection — Sentry mode toggle + snapshot activity.
 *
 * v0.28.45: turning Sentry on now goes through SentryConsentModal.
 * v0.28.52: on top of the window signal we now show live screenshot
 * activity — a running count, rolled-up disk footprint, and a small
 * gallery of the most recent thumbnails so the user can see what's
 * being stored on their machine (nothing is uploaded yet — that's
 * the next release). A "Capture now" button lets them exercise the
 * pipeline without waiting for the 5-minute cadence.
 *
 * Off by default. Turning it off never requires re-consent; only
 * turning it back on after a scope expansion does.
 */
import { useCallback, useEffect, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { convertFileSrc } from "@tauri-apps/api/core";
import {
  sentryCaptureNow,
  sentryListSnapshots,
  sentrySetEnabled,
  sentryStatus,
  type SentrySnapshotInfo,
  type SentryStatus,
} from "../lib/cloud";
import {
  SentryConsentModal,
  hasCurrentSentryConsent,
} from "./SentryConsentModal";

function formatBytes(n: number): string {
  if (n === 0) return "0 B";
  const units = ["B", "KB", "MB", "GB"];
  const i = Math.floor(Math.log(n) / Math.log(1024));
  return `${(n / Math.pow(1024, i)).toFixed(i === 0 ? 0 : 1)} ${units[i]}`;
}

function formatRelative(iso: string): string {
  if (!iso) return "";
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return "";
  const diff = Date.now() - then;
  if (diff < 60_000) return "just now";
  if (diff < 3_600_000) return `${Math.floor(diff / 60_000)}m ago`;
  if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)}h ago`;
  return `${Math.floor(diff / 86_400_000)}d ago`;
}

export function SentrySection() {
  const [status, setStatus] = useState<SentryStatus | null>(null);
  const [snapshots, setSnapshots] = useState<SentrySnapshotInfo[]>([]);
  const [busy, setBusy] = useState(false);
  const [capturing, setCapturing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [consentOpen, setConsentOpen] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const [s, snaps] = await Promise.all([
        sentryStatus(),
        sentryListSnapshots(6).catch(() => [] as SentrySnapshotInfo[]),
      ]);
      setStatus(s);
      setSnapshots(snaps);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void refresh();
    const t = setInterval(refresh, 15_000);
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
    if (status.enabled) {
      await setEnabled(false);
      return;
    }
    if (hasCurrentSentryConsent()) {
      await setEnabled(true);
    } else {
      setConsentOpen(true);
    }
  }

  async function captureNow() {
    if (capturing || !status?.enabled) return;
    setCapturing(true);
    setError(null);
    try {
      await sentryCaptureNow();
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setCapturing(false);
    }
  }

  const enabled = status?.enabled ?? false;

  return (
    <div className="flex flex-col gap-3">
      <p className="text-bone-3 text-[11px] leading-relaxed">
        Sentry mode lets Travis passively observe your workflow so it
        can serve you better — noticing patterns, spotting overdue
        items, resuming where you left off. Captures foreground app +
        window every 30 seconds, and a resized JPEG screenshot every
        five minutes. Snapshots stay on your machine (rolling window
        of the most recent {status?.snapshot_count ?? 20}). Off by
        default; pause or purge any time.
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
            Foreground window + screenshots
          </div>
          <div
            className="text-[10.5px] font-mono opacity-60"
            style={{ color: "rgba(236, 236, 241, 0.7)" }}
          >
            {enabled
              ? `${status?.buffered ?? 0} buffered · ${
                  status?.snapshot_count ?? 0
                } snapshots · ${formatBytes(status?.snapshot_bytes ?? 0)}`
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
        {enabled && (
          <motion.div
            key="gallery"
            initial={{ opacity: 0, y: -4 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -4 }}
            transition={{ duration: 0.25, ease: [0.22, 1, 0.36, 1] }}
            className="flex flex-col gap-2"
          >
            <div className="flex items-center justify-between">
              <div className="text-[10.5px] uppercase tracking-wider font-mono text-bone-3 opacity-70">
                recent snapshots
              </div>
              <motion.button
                whileHover={{ scale: 1.03 }}
                whileTap={{ scale: 0.97 }}
                onClick={captureNow}
                disabled={capturing}
                className="text-[10.5px] uppercase tracking-wider font-mono px-2.5 py-1 rounded-md transition-colors disabled:opacity-40"
                style={{
                  background: "rgba(189, 158, 255, 0.08)",
                  color: "rgb(189, 158, 255)",
                  border: "1px solid rgba(189, 158, 255, 0.35)",
                }}
              >
                {capturing ? "capturing…" : "capture now"}
              </motion.button>
            </div>
            {snapshots.length === 0 ? (
              <div className="text-[11px] font-mono opacity-50 text-bone-3">
                no snapshots yet — first one arrives ~5 minutes after
                you turn Sentry on.
              </div>
            ) : (
              <div className="grid grid-cols-3 gap-2">
                {snapshots.slice(0, 6).map((snap) => (
                  <a
                    key={snap.filename}
                    href={convertFileSrc(snap.path)}
                    target="_blank"
                    rel="noreferrer"
                    className="group block rounded-md overflow-hidden border relative"
                    style={{
                      borderColor: "rgba(255, 255, 255, 0.08)",
                      background: "rgba(0, 0, 0, 0.25)",
                    }}
                    title={`${snap.filename} · ${formatBytes(snap.bytes)}`}
                  >
                    <img
                      src={convertFileSrc(snap.path)}
                      alt={snap.filename}
                      className="w-full aspect-video object-cover transition-transform duration-300 ease-out group-hover:scale-[1.03]"
                      loading="lazy"
                    />
                    <div
                      className="absolute inset-x-0 bottom-0 px-1.5 py-1 text-[9.5px] font-mono flex items-center justify-between"
                      style={{
                        background:
                          "linear-gradient(to top, rgba(0,0,0,0.75), rgba(0,0,0,0))",
                        color: "rgba(236, 236, 241, 0.85)",
                      }}
                    >
                      <span>{formatRelative(snap.captured_at)}</span>
                      <span className="opacity-60">
                        {formatBytes(snap.bytes)}
                      </span>
                    </div>
                  </a>
                ))}
              </div>
            )}
          </motion.div>
        )}
      </AnimatePresence>

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
