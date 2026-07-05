/**
 * AttentionStrip — Shell 6.
 *
 * A persistent horizontal row at the top of the workspace showing what
 * currently deserves the user's attention: running workflows, T2T
 * queries needing a reply, drafts awaiting approval, upcoming reminders.
 *
 * Design principles enforced:
 * - Always visible, regardless of canvas state (survives Clear-all)
 * - Compact: each item is a single-line chip; the strip scrolls
 *   horizontally if there are many
 * - Real-time-ish: 30s poll (see useAttentionItems)
 * - Smooth-as-Apple motion: staggered enter, spring settle, ease-out exit
 * - No hard states: an empty strip renders a subtle "all clear" hint,
 *   not a blank div (or, when configured, hides entirely)
 */
import { motion, AnimatePresence } from "framer-motion";
import { useAttentionItems, type AttentionItem } from "./useAttentionItems";

interface Props {
  /** Whether to hide the strip when there's nothing to show. Default
   *  false — an "all clear" pip is a positive signal in itself. */
  hideWhenEmpty?: boolean;
  onItemClick?: (item: AttentionItem) => void;
}

export function AttentionStrip({
  hideWhenEmpty = false,
  onItemClick,
}: Props) {
  const { items, loading } = useAttentionItems();

  if (hideWhenEmpty && !loading && items.length === 0) return null;

  return (
    <div
      className="relative w-full"
      style={{
        // Provide breathing room on the top row of the workspace; matches
        // the command pill's outer padding so they align visually.
        padding: "8px 12px",
      }}
    >
      <div
        className="flex items-center gap-2 overflow-x-auto"
        style={{
          scrollbarWidth: "none",
          msOverflowStyle: "none",
        }}
      >
        <span
          className="shrink-0 text-[9px] uppercase tracking-[0.24em] font-mono select-none"
          style={{ color: "rgba(236, 236, 241, 0.35)" }}
        >
          Attention
        </span>

        <AnimatePresence initial={false} mode="popLayout">
          {loading && items.length === 0 && (
            <motion.span
              key="loading"
              initial={{ opacity: 0 }}
              animate={{ opacity: 0.5 }}
              exit={{ opacity: 0 }}
              transition={{ duration: 0.2, ease: [0.22, 1, 0.36, 1] }}
              className="text-[11px] font-mono"
              style={{ color: "rgba(236, 236, 241, 0.4)" }}
            >
              scanning…
            </motion.span>
          )}

          {!loading && items.length === 0 && (
            <motion.span
              key="clear"
              initial={{ opacity: 0, scale: 0.95 }}
              animate={{ opacity: 1, scale: 1 }}
              exit={{ opacity: 0, scale: 0.95 }}
              transition={{ duration: 0.24, ease: [0.22, 1, 0.36, 1] }}
              className="text-[11px] font-mono inline-flex items-center gap-1.5"
              style={{ color: "rgba(129, 199, 132, 0.7)" }}
            >
              <span
                className="h-1.5 w-1.5 rounded-full"
                style={{
                  background: "rgb(129, 199, 132)",
                  boxShadow: "0 0 8px rgba(129, 199, 132, 0.5)",
                }}
              />
              all clear
            </motion.span>
          )}

          {items.map((item, idx) => (
            <motion.button
              key={item.id}
              layout
              initial={{ opacity: 0, y: 6, scale: 0.96 }}
              animate={{ opacity: 1, y: 0, scale: 1 }}
              exit={{ opacity: 0, y: 6, scale: 0.96 }}
              transition={{
                duration: 0.28,
                ease: [0.22, 1, 0.36, 1],
                delay: Math.min(idx * 0.03, 0.15),
              }}
              whileHover={{ scale: 1.02 }}
              whileTap={{ scale: 0.98 }}
              onClick={() => onItemClick?.(item)}
              title={item.detail}
              className="shrink-0 inline-flex items-center gap-2 px-3 py-1.5 rounded-full text-[11px] font-mono cursor-pointer"
              style={styleFor(item.kind)}
            >
              <span
                className="h-1.5 w-1.5 rounded-full"
                style={{
                  background: dotFor(item.kind),
                  boxShadow: `0 0 6px ${glowFor(item.kind)}`,
                }}
              />
              <span
                className="truncate"
                style={{ maxWidth: "26ch", color: "rgba(236, 236, 241, 0.9)" }}
              >
                {item.label}
              </span>
            </motion.button>
          ))}
        </AnimatePresence>
      </div>

      {/* Hide the horizontal scrollbar visually while keeping the
          overflow-x-auto behavior for wheel + touch drag scrolling. */}
      <style>{`
        div[data-attention-strip] > div::-webkit-scrollbar { display: none; }
      `}</style>
    </div>
  );
}

// ─── Per-kind styling (smooth-as-Apple palette) ──────────────────

function styleFor(kind: AttentionItem["kind"]): React.CSSProperties {
  switch (kind) {
    case "t2t_pending":
      return {
        background: "rgba(110, 196, 232, 0.10)",
        border: "1px solid rgba(110, 196, 232, 0.35)",
      };
    case "t2t_drafted":
      return {
        background: "rgba(189, 158, 255, 0.10)",
        border: "1px solid rgba(189, 158, 255, 0.35)",
      };
    case "workflow_awaiting_approval":
      return {
        background: "rgba(255, 179, 92, 0.10)",
        border: "1px solid rgba(255, 179, 92, 0.35)",
      };
    case "workflow_running":
      return {
        background: "rgba(255, 255, 255, 0.04)",
        border: "1px solid rgba(255, 255, 255, 0.12)",
      };
  }
}

function dotFor(kind: AttentionItem["kind"]): string {
  switch (kind) {
    case "t2t_pending":
      return "rgb(110, 196, 232)";
    case "t2t_drafted":
      return "rgb(189, 158, 255)";
    case "workflow_awaiting_approval":
      return "rgb(255, 179, 92)";
    case "workflow_running":
      return "rgb(236, 236, 241)";
  }
}

function glowFor(kind: AttentionItem["kind"]): string {
  switch (kind) {
    case "t2t_pending":
      return "rgba(110, 196, 232, 0.6)";
    case "t2t_drafted":
      return "rgba(189, 158, 255, 0.6)";
    case "workflow_awaiting_approval":
      return "rgba(255, 179, 92, 0.6)";
    case "workflow_running":
      return "rgba(236, 236, 241, 0.3)";
  }
}
