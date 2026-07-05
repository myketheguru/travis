/**
 * T2tConvoCard — Travis-to-Travis conversation card.
 *
 * Renders a query flowing between two Travises. State-aware:
 *
 *   sending / delivered:  outbound query, waiting for other side
 *   considering:          incoming, other side's Travis is thinking
 *   drafted:              draft reply ready for user approval
 *   answered:             final reply landed
 *   declined:             other side declined
 *
 * Actions on drafted state (the user-facing decision point):
 *   [Approve]  → sends the drafted response as final
 *   [Edit]     → opens the draft inline for edit, then approves
 *   [Decline]  → drops the query with an optional reason
 *
 * Wired into Shell 8's canvas: Travis renders this card when a T2T
 * query state changes or when the LLM references it explicitly.
 * Focused threads on this card behave like ordinary Thread cards —
 * turns continue between the two Travises.
 */
import { useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import type { T2tConvoState } from "../../lib/richResponse";
import {
  t2tApproveReply,
  t2tDeclineReply,
  t2tDraftReply,
} from "../../lib/cloud";

interface Props {
  queryId: string;
  fromDisplay: string;
  toDisplay: string;
  question: string;
  draftedResponse?: string;
  finalResponse?: string;
  state: T2tConvoState;
  narration?: string;
  /** Optional callback to refresh parent state after an action. */
  onStateChanged?: () => void;
}

export function T2tConvoCard({
  queryId,
  fromDisplay,
  toDisplay,
  question,
  draftedResponse,
  finalResponse,
  state,
  onStateChanged,
}: Props) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(draftedResponse ?? "");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handleApprove() {
    setBusy(true);
    setError(null);
    try {
      if (editing && draft.trim() !== (draftedResponse ?? "")) {
        // User edited — save new draft first, then approve with the new
        // text. approve_reply(id, finalResponse) accepts an override.
        await t2tDraftReply(queryId, draft);
        await t2tApproveReply(queryId, draft);
      } else {
        await t2tApproveReply(queryId);
      }
      onStateChanged?.();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
      setEditing(false);
    }
  }

  async function handleDecline() {
    setBusy(true);
    setError(null);
    try {
      await t2tDeclineReply(queryId);
      onStateChanged?.();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <motion.div
      layout
      className="rounded-2xl overflow-hidden"
      transition={{
        layout: { duration: 0.32, ease: [0.22, 1, 0.36, 1] },
      }}
      style={{
        border: "1px solid " + accentBorder(state),
        background: "linear-gradient(180deg, " + accentTint(state) + ", rgba(255,255,255,0.015))",
        boxShadow: "0 4px 24px -12px rgba(0, 0, 0, 0.5)",
      }}
    >
      {/* Header — who ↔ who + state pill */}
      <div className="px-4 py-3 flex items-start justify-between gap-4">
        <div className="min-w-0 flex-1">
          <div
            className="text-[10px] tracking-[0.18em] uppercase font-mono mb-1"
            style={{ color: "rgba(236, 236, 241, 0.5)" }}
          >
            // travis ↔ travis
          </div>
          <div
            className="text-[13px] font-mono"
            style={{ color: "rgba(236, 236, 241, 0.75)" }}
          >
            <span style={{ color: "rgb(110, 196, 232)" }}>{fromDisplay}</span>
            <span style={{ opacity: 0.5 }}> → </span>
            <span style={{ color: "rgb(189, 158, 255)" }}>{toDisplay}</span>
          </div>
        </div>
        <StatePill state={state} />
      </div>

      {/* Question */}
      <div
        className="px-4 pb-3 text-[14px] leading-relaxed"
        style={{ color: "rgb(236, 236, 241)" }}
      >
        {question}
      </div>

      {/* Reply body (drafted / answered / declined) */}
      <AnimatePresence initial={false} mode="popLayout">
        {state === "drafted" && (
          <motion.div
            key="drafted"
            initial={{ opacity: 0, height: 0 }}
            animate={{ opacity: 1, height: "auto" }}
            exit={{ opacity: 0, height: 0 }}
            transition={{ duration: 0.28, ease: [0.22, 1, 0.36, 1] }}
            className="overflow-hidden"
          >
            <div
              className="mx-4 mb-3 rounded-lg border p-3"
              style={{
                borderColor: "rgba(189, 158, 255, 0.28)",
                background: "rgba(189, 158, 255, 0.04)",
              }}
            >
              <div
                className="text-[10px] uppercase tracking-wider font-mono mb-2"
                style={{ color: "rgb(189, 158, 255)" }}
              >
                Draft reply — your review
              </div>
              {editing ? (
                <textarea
                  value={draft}
                  onChange={(e) => setDraft(e.target.value)}
                  autoFocus
                  className="w-full bg-white/[0.02] border rounded-md p-2 text-sm text-white placeholder:text-white/40 focus:outline-none focus:border-[#bd9eff]/40 min-h-[80px] resize-y"
                  style={{ borderColor: "rgba(255, 255, 255, 0.1)" }}
                />
              ) : (
                <div
                  className="text-[13.5px] leading-relaxed whitespace-pre-wrap"
                  style={{ color: "rgba(236, 236, 241, 0.95)" }}
                >
                  {draft || draftedResponse}
                </div>
              )}
            </div>
          </motion.div>
        )}

        {state === "answered" && finalResponse && (
          <motion.div
            key="answered"
            initial={{ opacity: 0, height: 0 }}
            animate={{ opacity: 1, height: "auto" }}
            exit={{ opacity: 0, height: 0 }}
            transition={{ duration: 0.28, ease: [0.22, 1, 0.36, 1] }}
            className="overflow-hidden"
          >
            <div
              className="mx-4 mb-3 rounded-lg border p-3"
              style={{
                borderColor: "rgba(129, 199, 132, 0.28)",
                background: "rgba(129, 199, 132, 0.04)",
              }}
            >
              <div
                className="text-[10px] uppercase tracking-wider font-mono mb-2"
                style={{ color: "rgb(129, 199, 132)" }}
              >
                Reply
              </div>
              <div
                className="text-[13.5px] leading-relaxed whitespace-pre-wrap"
                style={{ color: "rgba(236, 236, 241, 0.95)" }}
              >
                {finalResponse}
              </div>
            </div>
          </motion.div>
        )}
      </AnimatePresence>

      {/* Actions on drafted state */}
      {state === "drafted" && (
        <div
          className="px-4 py-3 border-t flex items-center gap-2"
          style={{ borderColor: "rgba(255, 255, 255, 0.06)" }}
        >
          <motion.button
            whileHover={{ scale: 1.02 }}
            whileTap={{ scale: 0.98 }}
            onClick={handleApprove}
            disabled={busy}
            className="text-[11px] uppercase tracking-wider font-mono px-3 py-2 rounded-lg transition-colors disabled:opacity-50"
            style={{
              background: "rgba(129, 199, 132, 0.15)",
              color: "rgb(129, 199, 132)",
              border: "1px solid rgba(129, 199, 132, 0.4)",
            }}
          >
            {editing ? "save + send" : "approve + send"}
          </motion.button>
          {!editing && (
            <motion.button
              whileHover={{ scale: 1.02 }}
              whileTap={{ scale: 0.98 }}
              onClick={() => setEditing(true)}
              disabled={busy}
              className="text-[11px] uppercase tracking-wider font-mono px-3 py-2 rounded-lg transition-colors disabled:opacity-50"
              style={{
                background: "rgba(255, 255, 255, 0.04)",
                color: "rgba(236, 236, 241, 0.85)",
                border: "1px solid rgba(255, 255, 255, 0.15)",
              }}
            >
              edit
            </motion.button>
          )}
          <motion.button
            whileHover={{ scale: 1.02 }}
            whileTap={{ scale: 0.98 }}
            onClick={handleDecline}
            disabled={busy}
            className="ml-auto text-[11px] uppercase tracking-wider font-mono px-3 py-2 rounded-lg transition-colors disabled:opacity-50"
            style={{
              background: "rgba(255, 100, 100, 0.06)",
              color: "rgba(255, 180, 180, 0.85)",
              border: "1px solid rgba(255, 100, 100, 0.25)",
            }}
          >
            decline
          </motion.button>
        </div>
      )}

      {error && (
        <div className="px-4 py-2 text-[11px] font-mono" style={{ color: "rgba(255, 120, 120, 0.85)" }}>
          {error}
        </div>
      )}
    </motion.div>
  );
}

// ─── State visuals ───────────────────────────────────────────────

function StatePill({ state }: { state: T2tConvoState }) {
  const label = stateLabel(state);
  return (
    <span
      className="shrink-0 inline-flex items-center gap-1.5 text-[10px] uppercase tracking-wider font-mono px-2 py-1 rounded-full"
      style={{
        background: accentTint(state),
        color: accentColor(state),
        border: "1px solid " + accentBorder(state),
      }}
    >
      <span
        className="h-1.5 w-1.5 rounded-full"
        style={{
          background: accentColor(state),
          boxShadow: `0 0 6px ${accentColor(state)}`,
        }}
      />
      {label}
    </span>
  );
}

function stateLabel(s: T2tConvoState): string {
  switch (s) {
    case "sending":
      return "sending";
    case "delivered":
      return "delivered";
    case "considering":
      return "considering";
    case "drafted":
      return "review draft";
    case "answered":
      return "answered";
    case "declined":
      return "declined";
  }
}

function accentColor(s: T2tConvoState): string {
  switch (s) {
    case "sending":
    case "delivered":
    case "considering":
      return "rgb(110, 196, 232)";
    case "drafted":
      return "rgb(189, 158, 255)";
    case "answered":
      return "rgb(129, 199, 132)";
    case "declined":
      return "rgb(255, 130, 130)";
  }
}

function accentBorder(s: T2tConvoState): string {
  return accentColor(s).replace("rgb(", "rgba(").replace(")", ", 0.35)");
}

function accentTint(s: T2tConvoState): string {
  return accentColor(s).replace("rgb(", "rgba(").replace(")", ", 0.06)");
}
