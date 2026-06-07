import { useState } from "react";
import { motion, AnimatePresence } from "framer-motion";

interface Props {
  text: string;
  defaultExpanded?: boolean;
}

/// Collapsed by default. Shows "▸ Thinking…" header that expands to
/// reveal italic gray internal reasoning, Claude-style.
export function ThinkingSection({ text, defaultExpanded = false }: Props) {
  const [expanded, setExpanded] = useState(defaultExpanded);
  if (!text.trim()) return null;
  return (
    <div className="my-2">
      <button
        onClick={() => setExpanded((p) => !p)}
        className="text-bone-3 hover:text-bone-2 text-[11px] font-mono inline-flex items-center gap-1.5"
      >
        <span
          className="inline-block transition-transform"
          style={{
            transform: expanded ? "rotate(90deg)" : "rotate(0deg)",
          }}
        >
          ▸
        </span>
        <span>Thinking</span>
      </button>
      <AnimatePresence>
        {expanded && (
          <motion.div
            initial={{ opacity: 0, height: 0 }}
            animate={{ opacity: 1, height: "auto" }}
            exit={{ opacity: 0, height: 0 }}
            transition={{ duration: 0.22, ease: [0.16, 1, 0.3, 1] }}
            className="mt-1 text-[12px] text-bone-3 italic leading-relaxed border-l-2 border-bone-3/15 pl-3 whitespace-pre-wrap"
          >
            {text}
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
