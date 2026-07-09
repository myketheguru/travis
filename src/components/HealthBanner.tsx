import { useEffect, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  healthDismiss,
  healthSetOnline,
  healthStatus,
  onHealthChanged,
  type HealthState,
  type IssueKind,
} from "../lib/health";

// v0.28.24 — kinds where the resolution is 'upgrade your plan'. The
// banner shows an Upgrade CTA that opens Settings on usetravis.com.
const UPGRADE_KINDS = new Set<IssueKind>(["quotaExhausted"]);
const PLAN_URL = "https://usetravis.com/app/settings";

// v0.28.22 — user-facing copy. Never mentions LLMs, providers, rate
// limits, quotas, or any internal machinery. The user learns that
// Travis is taking a moment; the fix (if any) is framed as their next
// action, not our error.
const headlines: Record<IssueKind, string> = {
  offline: "You're offline",
  quotaExhausted: "Travis is out of runway for now",
  rateLimited: "Travis needs a beat",
  unauthorized: "Travis lost access",
  serverError: "Travis is catching its breath",
  networkError: "Travis can't get through",
  provider: "Travis hit a snag",
};

const subtexts: Record<IssueKind, string> = {
  offline:
    "Background work is paused. It'll pick back up when you're online.",
  quotaExhausted:
    "You've hit your monthly usage. Upgrade to keep Travis working through the month.",
  rateLimited:
    "Just a short pause on background work — it'll resume next time you ask something.",
  unauthorized:
    "Open Settings → Account to reconnect, then dismiss this banner.",
  serverError:
    "Background work is paused for a moment. It'll pick back up next time you ask something.",
  networkError:
    "Background work is paused for a moment. It'll pick back up next time you ask something.",
  provider:
    "Background work is paused for a moment. It'll pick back up next time you ask something.",
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

  // v0.28.22 — restore the banner for all kinds. The user needs to
  // know Travis is having a moment; they just don't need to see how
  // or why. Technical `state.issue.message` never renders.
  const visible: { kind: IssueKind; detail: string | null } | null = (() => {
    if (!state) return null;
    if (!state.online) return { kind: "offline", detail: null };
    if (state.issue) return { kind: state.issue.kind, detail: null };
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
              {/* v0.28.22 — technical detail intentionally not rendered. */}
            </div>
            <div className="flex flex-col items-end gap-1.5 self-start">
              {UPGRADE_KINDS.has(visible.kind) && (
                <button
                  onClick={() => {
                    void openUrl(PLAN_URL).catch(() => {});
                  }}
                  className="rounded-md bg-warn/25 hover:bg-warn/35 text-warn text-[10px] tracking-wider font-medium px-2.5 py-1 transition-colors"
                >
                  Upgrade
                </button>
              )}
              {visible.kind !== "offline" && (
                <button
                  onClick={() => {
                    healthDismiss().catch(() => {});
                  }}
                  className="text-bone-3 hover:text-bone-2 text-[10px] tracking-wider"
                >
                  dismiss
                </button>
              )}
            </div>
          </div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
