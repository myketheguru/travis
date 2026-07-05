/**
 * OrbitalStack — right-side vertical stack of recent-but-not-focal
 * assistant messages.
 *
 * Each orbit is a compact card showing the message's first line or its
 * rich-part summary. Clicking one promotes it to focal (planned; today
 * onSelect fires + parent handles).
 */
import { motion, AnimatePresence } from "framer-motion";
import type { ConversationMessage } from "../../lib/conversation";
import { parseRichResponse } from "../../lib/richResponse";

interface Props {
  orbits: ConversationMessage[];
  onSelect?: (m: ConversationMessage) => void;
}

export function OrbitalStack({ orbits, onSelect }: Props) {
  if (orbits.length === 0) return null;
  return (
    <div className="flex flex-col gap-2 w-full max-w-[220px]">
      <div
        className="text-[9px] uppercase tracking-[0.22em] font-mono pl-2"
        style={{ color: "rgba(236, 236, 241, 0.35)" }}
      >
        Recent
      </div>
      <AnimatePresence initial={false}>
        {orbits.map((m, i) => (
          <motion.button
            key={m.id}
            layout
            initial={{ opacity: 0, x: 12, scale: 0.96 }}
            animate={{ opacity: 0.85 - i * 0.12, x: 0, scale: 1 - i * 0.02 }}
            exit={{ opacity: 0, x: 12, scale: 0.96 }}
            transition={{
              duration: 0.32,
              ease: [0.22, 1, 0.36, 1],
              delay: i * 0.04,
            }}
            whileHover={{ opacity: 1, scale: 1.02 }}
            whileTap={{ scale: 0.98 }}
            onClick={() => onSelect?.(m)}
            className="text-left rounded-lg px-2.5 py-1.5"
            style={{
              background: "rgba(255, 255, 255, 0.03)",
              border: "1px solid rgba(255, 255, 255, 0.08)",
              backdropFilter: "blur(4px)",
            }}
          >
            <div
              className="text-[11px] line-clamp-2 leading-snug"
              style={{ color: "rgba(236, 236, 241, 0.9)" }}
            >
              {summarize(m.content)}
            </div>
          </motion.button>
        ))}
      </AnimatePresence>
    </div>
  );
}

function summarize(content: string): string {
  const rich = parseRichResponse(content);
  if (rich) {
    // Prefer narration on the first non-text part; fall back to text markdown.
    const first = rich.parts[0];
    if (first) {
      if (first.kind === "text") return truncate(first.markdown, 90);
      const narration = (first as { narration?: string }).narration;
      if (narration) return truncate(narration, 90);
      return `[${first.kind}]`;
    }
  }
  return truncate(content.trim(), 90);
}

function truncate(s: string, n: number): string {
  return s.length <= n ? s : s.slice(0, n - 1).trimEnd() + "…";
}
