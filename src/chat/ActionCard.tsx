/**
 * v0.20.0 — shared ActionCard component used by both the Cmd+J
 * overlay AND the main chat surface. Renders one proposed_action as
 * a confirm/dismiss card with the rationale, a one-line param
 * summary, and (for technical actions like shell commands or
 * emails) a collapsible reveal of the literal payload.
 *
 * Extracted from `src/overlay/Overlay.tsx` so consent-gated changes
 * the LTE pack records (lte_engagement_critical_change,
 * lte_invoice_critical_change) actually show up in the chat thread
 * where the conversation happened — not only in the overlay.
 */
import { useState } from "react";
import { motion } from "framer-motion";
import {
  actionDetails,
  actionHasTechnicalDetails,
  actionLabel,
  actionTechnicalDetails,
  type ProposedAction,
} from "../lib/actions";

interface Props {
  action: ProposedAction;
  onConfirm: () => Promise<void>;
  onDecline: () => Promise<void>;
}

export function ActionCard({ action, onConfirm, onDecline }: Props) {
  const [busy, setBusy] = useState<"confirm" | "decline" | null>(null);
  const handle = async (kind: "confirm" | "decline") => {
    if (busy) return;
    setBusy(kind);
    try {
      if (kind === "confirm") await onConfirm();
      else await onDecline();
    } finally {
      setBusy(null);
    }
  };

  // Risk styling: shell + email are warn-tinted, money-critical
  // changes get the same warn treatment, anything else stays pulse.
  const isHighRisk =
    action.kind === "run_shell_command" ||
    action.kind === "lte_engagement_critical_change" ||
    action.kind === "lte_invoice_critical_change";

  return (
    <motion.div
      layout
      initial={{ opacity: 0, y: 4 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, y: -4 }}
      transition={{ duration: 0.25 }}
      className={
        isHighRisk
          ? "rounded-xl border border-warn/30 bg-warn/[0.05] p-3"
          : "rounded-xl border border-pulse/30 bg-pulse/[0.07] p-3"
      }
    >
      <div className="flex items-start gap-3">
        <div className="flex-shrink-0 mt-0.5">
          <span
            className={
              "block h-2 w-2 rounded-full " +
              (isHighRisk
                ? "bg-warn shadow-[0_0_8px_rgba(255,184,107,0.7)]"
                : "bg-pulse shadow-[0_0_8px_rgba(124,92,255,0.7)]")
            }
          />
        </div>
        <div className="flex-1 min-w-0">
          <div
            className={
              "text-[10px] tracking-[0.18em] uppercase mb-1 " +
              (isHighRisk ? "text-warn" : "text-pulse-2")
            }
          >
            {actionLabel(action.kind)}
          </div>
          <p className="text-bone-2 text-sm leading-relaxed">
            {action.rationale ?? "(no rationale provided)"}
          </p>
          {actionDetails(action.kind, action.paramsJson) && (
            <p className="text-bone-3 text-[11px] mt-1 font-mono">
              {actionDetails(action.kind, action.paramsJson)}
            </p>
          )}
          {actionHasTechnicalDetails(action.kind) && (
            <details className="mt-2 group">
              <summary className="cursor-pointer text-bone-3 hover:text-bone-2 text-[10px] tracking-wider list-none flex items-center gap-1">
                <span className="transition-transform group-open:rotate-90">›</span>
                <span>
                  {action.kind === "send_email" ? "show full email" : "show command"}
                </span>
              </summary>
              <pre className="mt-1.5 px-2.5 py-1.5 rounded bg-ink/60 border border-ink-3/40 text-bone-2 text-[11px] font-mono whitespace-pre-wrap">
                {actionTechnicalDetails(action.kind, action.paramsJson)}
              </pre>
            </details>
          )}
        </div>
        <div className="flex flex-col gap-1.5 items-end" data-no-drag>
          <button
            onClick={() => handle("confirm")}
            disabled={busy !== null}
            className={
              "px-3 py-1 rounded-full text-[11px] font-medium disabled:opacity-30 transition-colors " +
              (isHighRisk
                ? "bg-warn/90 text-ink hover:bg-warn"
                : "bg-bone/95 text-ink hover:bg-bone")
            }
          >
            {busy === "confirm"
              ? "Doing…"
              : action.kind === "run_shell_command"
              ? "Allow"
              : "Confirm"}
          </button>
          <button
            onClick={() => handle("decline")}
            disabled={busy !== null}
            className="text-bone-3 hover:text-bone-2 text-[11px] underline-offset-4 hover:underline disabled:opacity-30"
          >
            {busy === "decline" ? "…" : "Dismiss"}
          </button>
        </div>
      </div>
    </motion.div>
  );
}
