import { useCallback, useEffect, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  getActiveWorkflow,
  type WorkflowSurface,
} from "../lib/workflows";

interface Props {
  conversationId: number | null;
  /// Compact mode renders the pill inline (overlay); full mode is the
  /// expanded card (Manage → Ask).
  compact?: boolean;
}

/// Persistent "Travis is working on X" indicator. Subscribes to the
/// workflow-state-changed event so the pill refreshes whenever the
/// LLM updates the dialogue state, not just on user interaction.
export function ActiveWorkflowPill({ conversationId, compact = false }: Props) {
  const [wf, setWf] = useState<WorkflowSurface | null>(null);
  const [expanded, setExpanded] = useState(false);

  const refresh = useCallback(async () => {
    if (!conversationId) {
      setWf(null);
      return;
    }
    try {
      const surface = await getActiveWorkflow(conversationId);
      setWf(surface);
    } catch {
      setWf(null);
    }
  }, [conversationId]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  useEffect(() => {
    if (!conversationId) return;
    let unlisten: UnlistenFn | null = null;
    listen<number>("workflow-state-changed", (event) => {
      if (event.payload === conversationId) {
        refresh();
      }
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      try {
        unlisten?.();
      } catch {
        /* ignore */
      }
    };
  }, [conversationId, refresh]);

  if (!wf) return null;

  const progress = wf.requiredTotal === 0
    ? 1
    : Math.min(1, wf.filledCount / wf.requiredTotal);

  return (
    <div className={compact ? "pb-2" : "pb-3"}>
      <motion.div
        layout
        initial={{ opacity: 0, y: -4 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.22, ease: [0.16, 1, 0.3, 1] }}
        className={
          "rounded-lg overflow-hidden " +
          (expanded ? "" : "cursor-pointer")
        }
        style={{
          background: "rgba(74, 214, 255, 0.06)",
          border: "1px solid rgba(74, 214, 255, 0.22)",
        }}
        onClick={() => !expanded && setExpanded(true)}
      >
        {/* Pill header */}
        <div className="flex items-center gap-3 px-3 py-2">
          <span className="relative inline-flex">
            <span className="h-2 w-2 rounded-full bg-pulse-2" />
            <span className="absolute inset-0 h-2 w-2 rounded-full bg-pulse-2 animate-ping opacity-50" />
          </span>
          <span className="text-bone text-[12px] font-medium truncate flex-1">
            Travis is working on:{" "}
            <span className="text-pulse-2">{wf.displayName}</span>
          </span>
          <span className="text-bone-3 text-[11px] font-mono shrink-0">
            {wf.filledCount}/{wf.requiredTotal}
          </span>
          <button
            onClick={(e) => {
              e.stopPropagation();
              setExpanded((p) => !p);
            }}
            className="text-bone-3 hover:text-bone-2 text-[11px] px-1"
            data-no-drag
            title={expanded ? "Collapse" : "Expand"}
          >
            {expanded ? "−" : "+"}
          </button>
        </div>

        {/* Progress bar */}
        <div
          className="h-0.5"
          style={{
            background: `linear-gradient(to right, rgb(124, 92, 255) ${progress * 100}%, transparent ${progress * 100}%)`,
          }}
        />

        {/* Expanded detail */}
        <AnimatePresence>
          {expanded && (
            <motion.div
              key="expanded"
              initial={{ opacity: 0, height: 0 }}
              animate={{ opacity: 1, height: "auto" }}
              exit={{ opacity: 0, height: 0 }}
              transition={{ duration: 0.22, ease: [0.16, 1, 0.3, 1] }}
            >
              <div className="px-3 py-2 space-y-1.5 border-t border-pulse-2/15">
                {wf.startedIntent && (
                  <div className="text-bone-3 text-[11px] italic mb-2">
                    “{wf.startedIntent}”
                  </div>
                )}
                {wf.slots.map((slot) => (
                  <div
                    key={slot.name}
                    className="flex items-baseline gap-2 text-[11px]"
                  >
                    <span
                      className={
                        "shrink-0 w-3 text-center " +
                        (slot.filled
                          ? "text-pulse-2"
                          : slot.required
                            ? "text-warn"
                            : "text-bone-3")
                      }
                      title={
                        slot.filled
                          ? "filled"
                          : slot.required
                            ? "required"
                            : "optional"
                      }
                    >
                      {slot.filled ? "✓" : slot.required ? "○" : "·"}
                    </span>
                    <span className="text-bone-2 shrink-0 w-28 truncate">
                      {slot.label}
                    </span>
                    <span
                      className={
                        "truncate flex-1 " +
                        (slot.filled ? "text-bone" : "text-bone-3 italic")
                      }
                    >
                      {slot.filled
                        ? (slot.valuePreview ?? "—")
                        : slot.kind}
                    </span>
                  </div>
                ))}
                {wf.nextAsk && (
                  <div className="mt-2 pt-2 border-t border-pulse-2/10 text-[11px] text-bone-3">
                    <span className="text-pulse-2">→ Next:</span>{" "}
                    {wf.nextAsk.label}
                  </div>
                )}
              </div>
            </motion.div>
          )}
        </AnimatePresence>
      </motion.div>
    </div>
  );
}
