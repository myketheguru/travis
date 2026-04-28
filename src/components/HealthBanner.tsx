import { useEffect, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import {
  healthDismiss,
  healthSetOnline,
  healthStatus,
  onHealthChanged,
  type HealthState,
  type IssueKind,
} from "../lib/health";

const headlines: Record<IssueKind, string> = {
  offline: "You're offline",
  quotaExhausted: "LLM credits look exhausted",
  rateLimited: "LLM is rate limiting us",
  unauthorized: "LLM rejected the API key",
  serverError: "LLM service is having trouble",
  networkError: "Couldn't reach the LLM",
  provider: "LLM error",
};

const subtexts: Record<IssueKind, string> = {
  offline:
    "Travis paused background work. It'll pick back up when you're online.",
  quotaExhausted:
    "Travis paused background work. Top up your provider credits or switch model in Settings, then dismiss this banner to retry.",
  rateLimited:
    "Travis paused background work for now. Will retry next time you write or ask something.",
  unauthorized:
    "Open Settings → Model and re-enter your API key, then dismiss this banner.",
  serverError:
    "Travis paused background work. Will retry next time you write or ask something.",
  networkError:
    "Travis paused background work. Will retry next time you write or ask something.",
  provider:
    "Travis paused background work. Will retry next time you write or ask something.",
};

export default function HealthBanner() {
  const [state, setState] = useState<HealthState | null>(null);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | null = null;

    healthStatus()
      .then((s) => {
        if (!cancelled) setState(s);
      })
      .catch(() => {});

    onHealthChanged((s) => {
      if (!cancelled) setState(s);
    }).then((fn) => {
      if (cancelled) {
        fn();
      } else {
        unlisten = fn;
      }
    });

    // Push the browser's actual online state up to the backend now and on
    // every change. The backend assumes online at startup; this corrects it.
    const pushOnline = () => {
      healthSetOnline(navigator.onLine).catch(() => {});
    };
    pushOnline();
    window.addEventListener("online", pushOnline);
    window.addEventListener("offline", pushOnline);

    return () => {
      cancelled = true;
      window.removeEventListener("online", pushOnline);
      window.removeEventListener("offline", pushOnline);
      if (unlisten) unlisten();
    };
  }, []);

  // Compose what to show. Offline takes precedence over any LLM issue —
  // there's no point talking about quota when the network is down.
  const visible: { kind: IssueKind; detail: string | null } | null = (() => {
    if (!state) return null;
    if (!state.online) return { kind: "offline", detail: null };
    if (state.issue) return { kind: state.issue.kind, detail: state.issue.message };
    return null;
  })();

  return (
    <AnimatePresence>
      {visible && (
        <motion.div
          key={visible.kind}
          initial={{ y: -20, opacity: 0 }}
          animate={{ y: 0, opacity: 1 }}
          exit={{ y: -20, opacity: 0 }}
          transition={{ duration: 0.25, ease: [0.16, 1, 0.3, 1] }}
          className="fixed top-3 left-1/2 -translate-x-1/2 z-50 max-w-xl w-[calc(100%-32px)]"
        >
          <div
            className={
              "rounded-xl border px-4 py-2.5 backdrop-blur-md flex items-start gap-3 shadow-lg " +
              (visible.kind === "quotaExhausted" || visible.kind === "unauthorized"
                ? "border-warn/40 bg-warn/[0.10]"
                : "border-pulse-2/30 bg-pulse-2/[0.06]")
            }
            style={{
              boxShadow:
                "0 14px 40px -12px rgba(0,0,0,0.55), 0 4px 12px -4px rgba(124,92,255,0.18)",
            }}
          >
            <span
              className={
                "mt-1 h-2 w-2 rounded-full flex-shrink-0 " +
                (visible.kind === "quotaExhausted" || visible.kind === "unauthorized"
                  ? "bg-warn shadow-[0_0_8px_rgba(255,184,107,0.7)]"
                  : "bg-pulse-2 shadow-[0_0_8px_rgba(110,196,232,0.7)]")
              }
            />
            <div className="flex-1 min-w-0">
              <div
                className={
                  "text-xs tracking-wide font-medium " +
                  (visible.kind === "quotaExhausted" || visible.kind === "unauthorized"
                    ? "text-warn"
                    : "text-pulse-2")
                }
              >
                {headlines[visible.kind]}
              </div>
              <div className="text-bone-2 text-[11px] leading-relaxed mt-0.5">
                {subtexts[visible.kind]}
              </div>
              {visible.detail && (
                <div className="text-bone-3 text-[10px] mt-1 font-mono break-words">
                  {visible.detail}
                </div>
              )}
            </div>
            {visible.kind !== "offline" && (
              <button
                onClick={() => {
                  healthDismiss().catch(() => {});
                }}
                className="text-bone-3 hover:text-bone-2 text-[10px] tracking-wider self-start"
              >
                dismiss
              </button>
            )}
          </div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
