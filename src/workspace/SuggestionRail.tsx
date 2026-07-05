/**
 * SuggestionRail — Shell 9.
 *
 * Ambient horizontal strip of 3-5 clickable chips directly beneath
 * the command pill (or, in this shell iteration, above the primary
 * canvas). Each chip is a "next move" Travis anticipates the user
 * might want. Click = submits the prompt into the composer for you.
 *
 * Different from AttentionStrip (which surfaces already-happened
 * events). Suggestions are proposed, not required.
 *
 * Design:
 * - Chips ease in on mount, stagger by index
 * - Hover microstate (scale 1.02, subtle glow)
 * - No 'dismiss' — suggestions rotate with time of day, they don't
 *   need manual clearing
 */
import { motion, AnimatePresence } from "framer-motion";
import { useSuggestions, type Suggestion } from "./useSuggestions";

interface Props {
  /** Fired when the user clicks a chip. Parent should push
   *  `suggestion.prompt` into the command pill / submit. */
  onSuggestionClick?: (suggestion: Suggestion) => void;
  /** Optional: also submit the prompt directly rather than putting it
   *  in the input. */
  onSuggestionSubmit?: (suggestion: Suggestion) => void;
}

export function SuggestionRail({ onSuggestionClick }: Props) {
  const items = useSuggestions();

  if (items.length === 0) return null;

  return (
    <div className="px-3 py-2">
      <div
        className="flex items-center gap-2 overflow-x-auto"
        style={{ scrollbarWidth: "none", msOverflowStyle: "none" }}
      >
        <span
          className="shrink-0 text-[9px] uppercase tracking-[0.24em] font-mono select-none"
          style={{ color: "rgba(236, 236, 241, 0.28)" }}
        >
          Suggest
        </span>

        <AnimatePresence initial={false} mode="popLayout">
          {items.map((item, idx) => (
            <motion.button
              key={item.id}
              layout
              initial={{ opacity: 0, y: 4, scale: 0.96 }}
              animate={{ opacity: 1, y: 0, scale: 1 }}
              exit={{ opacity: 0, y: 4, scale: 0.96 }}
              transition={{
                duration: 0.3,
                ease: [0.22, 1, 0.36, 1],
                delay: Math.min(idx * 0.035, 0.18),
              }}
              whileHover={{ scale: 1.03 }}
              whileTap={{ scale: 0.97 }}
              onClick={() => onSuggestionClick?.(item)}
              title={item.detail ?? item.prompt}
              className="shrink-0 inline-flex items-center gap-1.5 px-3 py-1.5 rounded-full text-[11px] font-mono cursor-pointer transition-shadow"
              style={styleFor(item.kind)}
            >
              <span aria-hidden style={{ fontSize: 10, opacity: 0.7 }}>
                {glyphFor(item.kind)}
              </span>
              <span style={{ color: "rgba(236, 236, 241, 0.88)" }}>
                {item.label}
              </span>
            </motion.button>
          ))}
        </AnimatePresence>
      </div>
    </div>
  );
}

function styleFor(kind: Suggestion["kind"]): React.CSSProperties {
  const base: React.CSSProperties = {
    background: "rgba(255, 255, 255, 0.03)",
    border: "1px solid rgba(255, 255, 255, 0.08)",
  };
  switch (kind) {
    case "check_in":
      return {
        ...base,
        background: "rgba(110, 196, 232, 0.06)",
        border: "1px solid rgba(110, 196, 232, 0.22)",
      };
    case "plan":
      return {
        ...base,
        background: "rgba(189, 158, 255, 0.06)",
        border: "1px solid rgba(189, 158, 255, 0.22)",
      };
    case "wrap":
      return {
        ...base,
        background: "rgba(255, 179, 92, 0.06)",
        border: "1px solid rgba(255, 179, 92, 0.22)",
      };
    default:
      return base;
  }
}

function glyphFor(kind: Suggestion["kind"]): string {
  switch (kind) {
    case "check_in":
      return "◐";
    case "plan":
      return "◇";
    case "wrap":
      return "◒";
    case "calendar":
      return "◨";
    case "reminder":
      return "◈";
    case "recent":
      return "◍";
  }
}
