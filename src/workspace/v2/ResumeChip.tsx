/**
 * ResumeChip — v2 Shell 9.
 *
 * When the canvas is in first-moment state AND the user has a recent
 * conversation from the past (< 30 days but > 24h), Travis surfaces a
 * subtle chip near the composer:
 *
 *   "Resume yesterday's Q4 CX thread?"
 *
 * One click accepts → sets the active conversation to that one and
 * fires resurrection via cardLifecycle so archived cards for that
 * thread re-materialize on the canvas. Any typing fades the chip
 * (via isFirstMoment going false).
 */
import { useEffect, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { useAppStore } from "../../stores/app";
import {
  listConversationsForSwitcher,
  type ConversationListItem,
} from "../../lib/conversation";

const MIN_AGE_MS = 60 * 60 * 1000;                     // ignore anything <1h old
const MAX_AGE_MS = 30 * 24 * 60 * 60 * 1000;           // ignore anything older than 30d

export function ResumeChip() {
  const isFirstMoment = useAppStore((s) => s.isFirstMoment);
  const setActiveConversationId = useAppStore(
    (s) => s.setActiveConversationId,
  );
  const noteUserActivity = useAppStore((s) => s.noteUserActivity);

  const [suggestion, setSuggestion] = useState<ConversationListItem | null>(
    null,
  );
  const [dismissed, setDismissed] = useState(false);

  useEffect(() => {
    if (!isFirstMoment || dismissed) return;
    let cancelled = false;
    async function load() {
      try {
        const list = await listConversationsForSwitcher(undefined, 10);
        if (cancelled) return;
        const now = Date.now();
        const candidate = list.find((c) => {
          const t = Date.parse(c.updatedAt);
          if (!Number.isFinite(t)) return false;
          const age = now - t;
          return age >= MIN_AGE_MS && age <= MAX_AGE_MS;
        });
        setSuggestion(candidate ?? null);
      } catch {
        // Silent — resume is a nicety, not required
      }
    }
    void load();
    return () => {
      cancelled = true;
    };
  }, [isFirstMoment, dismissed]);

  function handleAccept() {
    if (!suggestion) return;
    setActiveConversationId(suggestion.id);
    noteUserActivity();
    setSuggestion(null);
  }

  function handleDismiss() {
    setDismissed(true);
    setSuggestion(null);
  }

  const show = isFirstMoment && suggestion !== null && !dismissed;

  return (
    <AnimatePresence>
      {show && suggestion && (
        <motion.div
          key="resume-chip"
          initial={{ opacity: 0, y: 8 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: 4 }}
          transition={{ duration: 0.32, ease: [0.22, 1, 0.36, 1] }}
          className="flex justify-center px-6 pb-2 pointer-events-none"
        >
          <div
            className="pointer-events-auto inline-flex items-center gap-2 rounded-full pl-3 pr-1.5 py-1"
            style={{
              background: "rgba(255, 179, 92, 0.10)",
              border: "1px solid rgba(255, 179, 92, 0.35)",
              backdropFilter: "blur(6px)",
            }}
          >
            <span
              className="h-1.5 w-1.5 rounded-full"
              style={{
                background: "rgb(255, 179, 92)",
                boxShadow: "0 0 8px rgba(255, 179, 92, 0.6)",
              }}
            />
            <motion.button
              whileHover={{ scale: 1.02 }}
              whileTap={{ scale: 0.98 }}
              onClick={handleAccept}
              className="text-[11.5px] px-1 font-mono"
              style={{ color: "rgba(236, 236, 241, 0.92)" }}
            >
              Resume{" "}
              <span style={{ color: "rgb(255, 179, 92)" }}>
                {truncate(suggestion.title ?? "your last thread", 44)}
              </span>
              ?
            </motion.button>
            <button
              onClick={handleDismiss}
              title="Not now"
              className="w-6 h-6 rounded-full flex items-center justify-center"
              style={{
                color: "rgba(236, 236, 241, 0.5)",
              }}
            >
              ✕
            </button>
          </div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}

function truncate(s: string, n: number): string {
  return s.length <= n ? s : s.slice(0, n - 1).trimEnd() + "…";
}
