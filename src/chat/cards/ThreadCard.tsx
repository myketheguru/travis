/**
 * ThreadCard — first-class card kind for long-running, open-ended
 * context (INTERFACE.md, feedback_threaded_cards).
 *
 * Cards contain conversations when the topic wants one. A thread card
 * looks like a normal card in the canvas — a hero title, a summary,
 * a "last turn" preview — but expanded, it reveals the full scrollable
 * chat with embedded rich sub-cards + a thread-local composer at the
 * bottom.
 *
 * Turns render each their `parts` through the top-level renderer
 * recursively, so a thread turn can hold a map, a list, an action
 * proposal — the same rich types the top level supports.
 *
 * Composer is stubbed for now — Shell 4 (context-aware pill) wires
 * the routing so typing while the thread is focused adds to it.
 */

import { useEffect, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import type { MessagePart } from "../../lib/richResponse";
import { RichResponseRenderer } from "./RichResponseRenderer";
import { useAppStore } from "../../stores/app";
import { useCardLifecycle } from "../../stores/cardLifecycle";

interface ThreadTurn {
  author: "user" | "travis";
  parts: MessagePart[];
}

interface Props {
  threadId?: string;
  title: string;
  summary?: string;
  turns: ThreadTurn[];
  /** LLM-supplied pinned hint. Overridden by client store when the
   *  user has explicitly (un)pinned in the UI. */
  pinned?: boolean;
  narration?: string;
  onFocus?: () => void;
  onPin?: () => void;
}

export function ThreadCard({
  threadId,
  title,
  summary,
  turns,
  pinned: pinnedProp,
  onFocus,
  onPin,
}: Props) {
  const [expanded, setExpanded] = useState(false);
  const focusedThread = useAppStore((s) => s.focusedThread);
  const setFocusedThread = useAppStore((s) => s.setFocusedThread);
  // v0.22.15 (Shell 5) — 24h card lifecycle. Client-side store tracks
  // pin state + last-interaction timestamps (extending the visibility
  // window). LLM-supplied pinned prop wins when explicitly set.
  const cardId = threadId ?? `thread:${title}`;
  const clientPinned = useCardLifecycle((s) => s.isPinned(cardId));
  const pinFn = useCardLifecycle((s) => s.pin);
  const unpinFn = useCardLifecycle((s) => s.unpin);
  const noteInteraction = useCardLifecycle((s) => s.noteInteraction);
  const effectivePinned = pinnedProp ?? clientPinned;
  const lastTurn = turns.length > 0 ? turns[turns.length - 1] : null;

  const isFocused =
    focusedThread !== null &&
    ((threadId != null && focusedThread.id === threadId) ||
      (threadId == null && focusedThread.title === title));

  function toggle() {
    const next = !expanded;
    setExpanded(next);
    // Any expand or collapse counts as interaction — extends the
    // card's 24h visibility window from now.
    noteInteraction(cardId);
    if (next) {
      setFocusedThread({ id: threadId ?? null, title });
      onFocus?.();
    } else if (isFocused) {
      setFocusedThread(null);
    }
  }

  function handlePin() {
    if (clientPinned) unpinFn(cardId);
    else pinFn(cardId);
    onPin?.();
  }

  // Global esc handler: pressing escape while focused defocuses +
  // collapses the thread. Enables the "esc to leave" chip semantics.
  useEffect(() => {
    if (!isFocused) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setFocusedThread(null);
        setExpanded(false);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [isFocused, setFocusedThread]);

  return (
    <motion.div
      layout
      className="rounded-2xl overflow-hidden"
      animate={{
        boxShadow: isFocused
          ? "0 6px 40px -12px rgba(124, 92, 255, 0.35)"
          : "0 4px 24px -12px rgba(0, 0, 0, 0.5)",
      }}
      transition={{
        layout: { duration: 0.32, ease: [0.22, 1, 0.36, 1] },
        boxShadow: { duration: 0.28, ease: [0.22, 1, 0.36, 1] },
      }}
      style={{
        border: isFocused
          ? "1px solid rgba(124, 92, 255, 0.55)"
          : effectivePinned
            ? "1px solid rgba(189, 158, 255, 0.55)"
            : "1px solid var(--hairline-2, rgba(255,255,255,0.1))",
        background:
          "linear-gradient(180deg, rgba(124, 92, 255, 0.05), rgba(255, 255, 255, 0.015))",
      }}
    >
      {/* Hero header — always visible */}
      <button
        onClick={toggle}
        className="w-full text-left px-4 py-3 flex items-start justify-between gap-4 hover:bg-white/[0.02] transition-colors"
      >
        <div className="min-w-0 flex-1">
          <div
            className="text-[10px] tracking-[0.18em] uppercase font-mono mb-1"
            style={{ color: "rgba(236, 236, 241, 0.5)" }}
          >
            // thread
            {effectivePinned && (
              <span
                className="ml-2 px-1.5 py-0.5 rounded text-[9px]"
                style={{
                  background: "rgba(189, 158, 255, 0.15)",
                  color: "rgb(189, 158, 255)",
                }}
              >
                effectivePinned
              </span>
            )}
          </div>
          <div
            className="text-[15px] font-medium truncate"
            style={{ color: "rgb(236, 236, 241)" }}
          >
            {title}
          </div>
          {summary && !expanded && (
            <div
              className="text-[12px] mt-1 leading-relaxed line-clamp-1"
              style={{ color: "rgba(236, 236, 241, 0.6)" }}
            >
              {summary}
            </div>
          )}
        </div>
        <span
          className="shrink-0 text-[11px] uppercase tracking-wider font-mono transition-transform"
          style={{
            color: "rgba(236, 236, 241, 0.4)",
            transform: expanded ? "rotate(180deg)" : "rotate(0deg)",
          }}
        >
          ▾
        </span>
      </button>

      {/* Collapsed: show a peek of the last turn */}
      {!expanded && lastTurn && (
        <div
          className="px-4 pb-3 border-t"
          style={{ borderColor: "rgba(255, 255, 255, 0.04)" }}
        >
          <div
            className="text-[10px] uppercase tracking-wider font-mono mt-2 mb-1.5"
            style={{ color: "rgba(236, 236, 241, 0.4)" }}
          >
            {lastTurn.author === "user" ? "you" : "travis"}
          </div>
          <div className="max-h-24 overflow-hidden">
            <RichResponseRenderer response={{ parts: lastTurn.parts }} />
          </div>
        </div>
      )}

      {/* Expanded: full turns + composer */}
      <AnimatePresence initial={false}>
        {expanded && (
          <motion.div
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: "auto", opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            transition={{ duration: 0.25, ease: [0.16, 1, 0.3, 1] }}
            className="overflow-hidden"
          >
            <div
              className="border-t"
              style={{ borderColor: "rgba(255, 255, 255, 0.06)" }}
            >
              <div className="max-h-[420px] overflow-y-auto px-4 py-3 flex flex-col gap-3">
                {turns.map((turn, i) => (
                  <div key={i}>
                    <div
                      className="text-[10px] uppercase tracking-wider font-mono mb-1.5"
                      style={{
                        color:
                          turn.author === "user"
                            ? "rgba(110, 196, 232, 0.65)"
                            : "rgba(189, 158, 255, 0.65)",
                      }}
                    >
                      {turn.author === "user" ? "you" : "travis"}
                    </div>
                    <RichResponseRenderer response={{ parts: turn.parts }} />
                  </div>
                ))}
              </div>

              {/* Thread-local composer stub. Shell 4 wires this up. */}
              <div
                className="px-4 py-3 border-t flex items-center gap-2"
                style={{ borderColor: "rgba(255, 255, 255, 0.06)" }}
              >
                <input
                  placeholder={`Continue ${title}…`}
                  className="flex-1 bg-white/[0.02] border rounded-lg px-3 py-2 text-sm text-white placeholder:text-white/40 focus:outline-none focus:border-[#bd9eff]/40 disabled:opacity-40"
                  style={{ borderColor: "rgba(255, 255, 255, 0.1)" }}
                  disabled
                  title="Shell 4 wires this up"
                />
                <button
                  onClick={handlePin}
                  className="text-[11px] uppercase tracking-wider font-mono px-3 py-2 rounded-lg transition-colors"
                    style={{
                      background: effectivePinned
                        ? "rgba(189, 158, 255, 0.15)"
                        : "rgba(255, 255, 255, 0.04)",
                      color: effectivePinned
                        ? "rgb(189, 158, 255)"
                        : "rgba(236, 236, 241, 0.7)",
                      border: `1px solid ${
                        effectivePinned
                          ? "rgba(189, 158, 255, 0.4)"
                          : "rgba(255, 255, 255, 0.1)"
                      }`,
                    }}
                >
                  {effectivePinned ? "unpin" : "pin"}
                </button>
              </div>
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </motion.div>
  );
}
